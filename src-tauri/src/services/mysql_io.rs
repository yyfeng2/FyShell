//! MySQL 导入导出服务（契约 P2 阶段）：CSV / JSON / SQL 三种格式。
//!
//! 连接复用 services::mysql 的公开助手（pool_of / take_tx_conn / give_tx_conn，
//! 与 mysql_edit 等模块同模式），不触碰其私有注册表。
//!
//! 防注入关键设计（与 mysql_edit.rs 同策略）：
//! - 表名/列名来自前端或导入文件，属 SQL 标识符（无法参数化），统一反引号包裹
//!   并校验非空、不含反引号字符，杜绝标识符逃逸出上下文；
//! - SQL 导出的值按字符串字面量写入（先转反斜杠再成对转义单引号，避免 `\'` 歧义）；
//! - CSV 导入用 exec_iter 二进制协议 + `?` 占位符填充（Value::Bytes），不做字面量拼接。
//!
//! 事务语义：连接已处于用户显式事务内（mysql_begin 开启）时，用 SAVEPOINT 包裹
//! 本次导入，失败仅回滚到保存点、不影响用户事务其余内容；否则开隐式事务包裹
//! 整批，任一批次失败整体回滚，成功后统一提交。
//!
//! 文件 IO 用 std::fs（导出量桌面工具场景有限，不需要流式）。

use std::time::Instant;

use mysql_async::prelude::*;
use mysql_async::{Conn, Value};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::error::AppError;
use crate::models::mysql_io::{
    MySqlExportFormat, MySqlExportOptions, MySqlImportFormat, MySqlImportOptions, MySqlIoResult,
};
use crate::services::mysql::{give_tx_conn, pool_of, take_tx_conn};

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql.rs 同策略）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 标识符校验并用反引号包裹：非空、不含反引号字符（防标识符逃逸）
fn quote_ident(name: &str) -> Result<String, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::general("标识符（表名/列名）不能为空"));
    }
    if name.contains('`') {
        return Err(AppError::general(format!(
            "标识符含非法字符（反引号）: {name}"
        )));
    }
    Ok(format!("`{name}`"))
}

/// 值 -> SQL 字面量：单引号包裹并转义（先转反斜杠再成对转义单引号，避免 `\'` 歧义）
fn quote_value(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

/// mysql_export：query_iter 文本协议全量拉取（不分页），按格式渲染后写目标文件
/// （覆盖写入——前端保存对话框已让用户确认路径）。
/// - CSV：首行列头，逗号分隔，RFC 4180 转义，NULL 输出空串；
/// - JSON：数组对象形式 [{col: val}]，NULL 为 null，值统一转字符串；
/// - SQL：可选 DROP+CREATE TABLE（SHOW CREATE TABLE 原文）+ INSERT 批量语句
///   （每 100 行一条多值 INSERT，值转义单引号）。
pub async fn export(conn_id: &str, opts: &MySqlExportOptions) -> Result<MySqlIoResult, AppError> {
    if opts.file_path.trim().is_empty() {
        return Err(AppError::general("导出文件路径为空"));
    }
    if opts.sql.trim().is_empty() {
        return Err(AppError::general("SQL 语句为空"));
    }
    let start = Instant::now();

    // 优先复用事务内独占连接（导出只读，与用户会话同一上下文），用完放回
    let tx_conn = take_tx_conn(conn_id);
    let from_tx = tx_conn.is_some();
    let mut conn = match tx_conn {
        Some(conn) => conn,
        None => pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?,
    };
    let outcome = do_export(&mut conn, opts).await;
    if from_tx {
        give_tx_conn(conn_id, conn);
    }
    let rows_total = outcome?;
    Ok(MySqlIoResult {
        rows_total,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// 导出主体（拆出以便错误后仍归还事务连接）
async fn do_export(conn: &mut Conn, opts: &MySqlExportOptions) -> Result<u64, AppError> {
    // 全量拉取：文本协议 query_iter，不分页
    let mut result = conn
        .query_iter(opts.sql.as_str())
        .await
        .map_err(mysql_err)?;
    let columns: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let mut out = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            // take::<Option<Value>> 承接 NULL（None），再统一转字符串
            let value = row.take::<Option<Value>, _>(i).flatten();
            out.push(value.and_then(value_to_string));
        }
        rows.push(out);
    }
    // 消费剩余结果集/清理语句（全量遍历后调用是安全的）
    result.drop_result().await.map_err(mysql_err)?;

    let content = match opts.format {
        MySqlExportFormat::Csv => to_csv(&columns, &rows),
        MySqlExportFormat::Json => to_json(&columns, &rows)?,
        MySqlExportFormat::Sql => build_sql_export(conn, opts, &columns, &rows).await?,
    };
    std::fs::write(&opts.file_path, content)
        .map_err(|e| AppError::general(format!("写入导出文件失败: {e}")))?;
    Ok(rows.len() as u64)
}

/// CSV 渲染：首行列头 + 数据行，逗号分隔；值含逗号/引号/换行时双引号包裹
/// 并转义内部引号（RFC 4180）；NULL 输出空串，空串输出带引号的 ""（二者区分，
/// 导出/导入可无损往返）
fn to_csv(columns: &[String], rows: &[Vec<Option<String>>]) -> String {
    let mut out = String::new();
    out.push_str(
        &columns
            .iter()
            .map(|c| csv_escape(Some(c)))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push_str("\r\n");
    for row in rows {
        let line = row
            .iter()
            .map(|v| csv_escape(v.as_deref()))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push_str("\r\n");
    }
    out
}

/// RFC 4180 字段渲染：NULL（None）输出空串；空串输出带引号的 ""；
/// 含逗号/双引号/回车/换行时双引号包裹并成对转义内部引号
fn csv_escape(value: Option<&str>) -> String {
    match value {
        None => String::new(),
        Some(v) if v.is_empty() => "\"\"".to_string(),
        Some(v) => {
            if v.contains(',') || v.contains('"') || v.contains('\n') || v.contains('\r') {
                format!("\"{}\"", v.replace('"', "\"\""))
            } else {
                v.to_string()
            }
        }
    }
}

/// JSON 渲染：数组对象形式 [{col: val}]，NULL 为 null，值统一转字符串
fn to_json(columns: &[String], rows: &[Vec<Option<String>>]) -> Result<String, AppError> {
    let mut arr = Vec::with_capacity(rows.len());
    for row in rows {
        let mut obj = JsonMap::with_capacity(columns.len());
        for (i, col) in columns.iter().enumerate() {
            let value = match row.get(i) {
                Some(Some(v)) => JsonValue::String(v.clone()),
                // 缺失字段与 NULL（None）统一输出 null
                _ => JsonValue::Null,
            };
            obj.insert(col.clone(), value);
        }
        arr.push(JsonValue::Object(obj));
    }
    Ok(serde_json::to_string_pretty(&arr)?)
}

/// SQL 渲染：可选 DROP+CREATE TABLE（SHOW CREATE TABLE 原文）+ INSERT 批量语句
/// （每 100 行一条多值 INSERT，值转义单引号，NULL 输出 NULL）
async fn build_sql_export(
    conn: &mut Conn,
    opts: &MySqlExportOptions,
    columns: &[String],
    rows: &[Vec<Option<String>>],
) -> Result<String, AppError> {
    let mut out = String::from("-- FyShell MySQL 导出\n");

    // 从导出 SQL 解析目标表名（SHOW CREATE TABLE 与 INSERT 均需要）
    let table = extract_table_name(&opts.sql)
        .ok_or_else(|| AppError::general("无法从 SQL 中解析表名，无法生成 SQL 导出"))?;
    let quoted = quote_ident(&table)?;

    if opts.include_create_table {
        // SHOW CREATE TABLE 原文（保留引擎/字符集/索引等细节）
        let show_sql = format!("SHOW CREATE TABLE {quoted}");
        let (_, create): (Option<String>, Option<String>) = conn
            .query_first(&show_sql)
            .await
            .map_err(mysql_err)?
            .ok_or_else(|| AppError::general(format!("读取表结构失败，表不存在: {table}")))?;
        if let Some(create) = create {
            out.push_str(&format!("DROP TABLE IF EXISTS {quoted};\n{create};\n\n"));
        }
    }

    let col_list = columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");

    // INSERT 批量语句：每 100 行一条多值 INSERT，NULL 输出 NULL
    for chunk in rows.chunks(100) {
        let values = chunk
            .iter()
            .map(|row| {
                let cells = row
                    .iter()
                    .map(|v| match v {
                        Some(s) => quote_value(s),
                        None => "NULL".to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({cells})")
            })
            .collect::<Vec<_>>()
            .join(",\n  ");
        out.push_str(&format!(
            "INSERT INTO {quoted} ({col_list}) VALUES\n  {values};\n"
        ));
    }
    Ok(out)
}

/// 从导出 SQL 中解析表名（词边界匹配 FROM 后的标识符，支持反引号包裹）。
///
/// 简化说明：仅取第一个 FROM 后的标识符，JOIN/多表查询取主表——与
/// contains_limit_clause 同属可接受的简化，前端 SQL 编辑器通常导出单表查询。
/// 位置基于小写副本计算，但切片取原文（保持表名大小写，避免 Linux 下大小写敏感）。
fn extract_table_name(sql: &str) -> Option<String> {
    let lowered = sql.to_ascii_lowercase();
    let from_end = find_word(&lowered, "from")? + "from".len();
    let mut rest = sql[from_end..].trim_start();
    let mut last: Option<String> = None;

    // 逐段读取标识符（支持 `db`.`t` / db.t 限定形式），最后一段为表名
    loop {
        let ident_end;
        let remaining;
        if let Some(stripped) = rest.strip_prefix('`') {
            let end = stripped.find('`')?;
            ident_end = end;
            remaining = &stripped[end + 1..];
        } else {
            ident_end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            if ident_end == 0 {
                break;
            }
            remaining = &rest[ident_end..];
        }
        last = Some(rest[..ident_end].to_string());
        rest = remaining.trim_start();
        if rest.starts_with('.') {
            rest = rest[1..].trim_start();
        } else {
            break;
        }
    }
    last
}

/// 词边界查找：返回关键字在原文中的起始下标（找不到返回 None）
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let n = needle.len();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0
            || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
        let after = abs + n;
        let after_ok = after >= bytes.len()
            || !bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_';
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

/// mysql_import：导入前置校验 + 事务包裹 + 按格式执行，保证原子性。
/// - CSV：解析文件（首行列头 + 数据行），批量 INSERT（占位符二进制协议填充）；
/// - SQL：按分号拆分语句（跳过字符串字面量与注释内的分号），逐条执行；
/// - replace=true 的覆盖确认由前端负责，后端直接执行。
pub async fn import(conn_id: &str, imp: &MySqlImportOptions) -> Result<MySqlIoResult, AppError> {
    validate_import(imp)?;
    let start = Instant::now();
    let rows_total = import_inner(conn_id, imp).await?;
    Ok(MySqlIoResult {
        rows_total,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// 导入主体（事务包裹策略见模块注释）：
/// - 连接已处于用户显式事务内：SAVEPOINT 包裹，失败仅回滚到保存点；
/// - 无活跃事务：START TRANSACTION 包裹，失败整体 ROLLBACK，成功 COMMIT。
async fn import_inner(conn_id: &str, imp: &MySqlImportOptions) -> Result<u64, AppError> {
    if let Some(mut conn) = take_tx_conn(conn_id) {
        let outcome = with_savepoint(&mut conn, imp).await;
        give_tx_conn(conn_id, conn);
        return outcome;
    }

    // 无活跃事务：开隐式事务包裹整批
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    if let Err(e) = conn.query_drop("START TRANSACTION").await {
        // START TRANSACTION 失败时事务未开，无需回滚，连接随 drop 归还池
        return Err(mysql_err(e));
    }
    match do_import(&mut conn, imp).await {
        Ok(rows_total) => {
            conn.query_drop("COMMIT").await.map_err(mysql_err)?;
            Ok(rows_total)
        }
        Err(e) => {
            // 任一批次失败：回滚整批（隐式事务保证原子性）
            let _ = conn.query_drop("ROLLBACK").await;
            Err(e)
        }
    }
}

/// 用户事务内：SAVEPOINT 包裹本次导入，失败 ROLLBACK TO SAVEPOINT，成功 RELEASE
async fn with_savepoint(conn: &mut Conn, imp: &MySqlImportOptions) -> Result<u64, AppError> {
    conn.query_drop("SAVEPOINT _fyshell_import")
        .await
        .map_err(mysql_err)?;
    match do_import(conn, imp).await {
        Ok(rows_total) => {
            conn.query_drop("RELEASE SAVEPOINT _fyshell_import")
                .await
                .map_err(mysql_err)?;
            Ok(rows_total)
        }
        Err(e) => {
            let _ = conn.query_drop("ROLLBACK TO SAVEPOINT _fyshell_import").await;
            Err(e)
        }
    }
}

/// 文本解码探测：UTF-8 合法则直接按 UTF-8；否则视为 GBK/GB18030 用 encoding_rs 解码
/// （GBK 为 GB18030 子集，解码器覆盖两类文件；Windows 导出的中文 CSV / 含中文注释的
/// SQL 文件是常见场景，原 read_to_string 对 GBK 必失败）。
/// 引入 encoding_rs 的理由：GBK->UTF-8 依赖完整映射表，手写不可行且易错；encoding_rs
/// 为纯 Rust 标准实现（无进程外转换），零额外传递依赖，本项目桌面工具场景成熟可靠。
fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _, _) = encoding_rs::GB18030.decode(bytes);
            cow.into_owned()
        }
    }
}

/// 导入执行主体：二进制读取文件并按格式分发（编码探测见 decode_text）
async fn do_import(conn: &mut Conn, imp: &MySqlImportOptions) -> Result<u64, AppError> {
    let raw = std::fs::read(&imp.file_path)
        .map_err(|e| AppError::general(format!("读取导入文件失败: {e}")))?;
    let content = decode_text(&raw);
    match imp.format {
        MySqlImportFormat::Csv => import_csv(conn, imp, &content).await,
        MySqlImportFormat::Sql => import_sql(conn, imp, &content).await,
    }
}

/// CSV 导入：首行为列头，数据行批量插入（每批 batch_size 行）。
/// INSERT 语句用 `?` 占位符（exec_iter 二进制协议填充，Value::Bytes），
/// 未加引号的空字段视为 NULL（与字符串 "NULL" 区分）；replace=true 时用
/// REPLACE INTO 覆盖已有主键。
async fn import_csv(
    conn: &mut Conn,
    imp: &MySqlImportOptions,
    content: &str,
) -> Result<u64, AppError> {
    let records = parse_csv(content)?;
    let header = records
        .first()
        .ok_or_else(|| AppError::general("导入文件为空（缺少列头行）"))?;
    let table = quote_ident(&imp.table)?;
    let col_list = header
        .iter()
        .map(|c| quote_ident(c.as_deref().unwrap_or("")))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");

    // 列数校验：数据行列数与表头不一致视为非法输入，防止错位写入
    for (idx, row) in records.iter().skip(1).enumerate() {
        if row.len() != header.len() {
            return Err(AppError::general(format!(
                "CSV 第 {} 行列数与表头不一致（{} != {}）",
                idx + 2,
                row.len(),
                header.len()
            )));
        }
    }

    let keyword = if imp.replace { "REPLACE INTO" } else { "INSERT INTO" };
    let batch_size = imp.batch_size.clamp(1, 1000) as usize;
    let row_ph = format!("({})", vec!["?"; header.len()].join(", "));

    let mut rows_total = 0u64;
    for chunk in records[1..].chunks(batch_size) {
        let values_sql = vec![row_ph.as_str(); chunk.len()].join(", ");
        let mut params: Vec<Value> = Vec::with_capacity(chunk.len() * header.len());
        for row in chunk {
            for cell in row {
                match cell {
                    Some(v) => params.push(Value::Bytes(v.clone().into_bytes())),
                    // 空字段（未加引号的空串）视为 NULL
                    None => params.push(Value::NULL),
                }
            }
        }
        let sql = format!("{keyword} {table} ({col_list}) VALUES {values_sql}");
        let result = conn.exec_iter(sql.as_str(), params).await.map_err(mysql_err)?;
        // 消费剩余结果集/清理语句
        result.drop_result().await.map_err(mysql_err)?;
        rows_total += chunk.len() as u64;
    }
    Ok(rows_total)
}

/// SQL 导入：逐条执行文件中的语句（按分号拆分，字符串字面量与注释内的分号
/// 不参与拆分），rows_total 取总受影响行数
async fn import_sql(
    conn: &mut Conn,
    imp: &MySqlImportOptions,
    content: &str,
) -> Result<u64, AppError> {
    let statements = split_sql_statements(content);
    if statements.is_empty() {
        return Err(AppError::general(format!(
            "导入文件中未找到可执行的 SQL 语句: {}",
            imp.file_path
        )));
    }
    let mut rows_total = 0u64;
    for (idx, stmt) in statements.iter().enumerate() {
        let result = match conn.query_iter(stmt.as_str()).await {
            Ok(result) => result,
            Err(e) => {
                return Err(AppError::general(format!(
                    "SQL 导入第 {} 条语句失败: {e}",
                    idx + 1
                )));
            }
        };
        let affected = result.affected_rows();
        // 消费剩余结果集/清理语句
        result.drop_result().await.map_err(mysql_err)?;
        rows_total += affected;
    }
    Ok(rows_total)
}

/// CSV 解析（RFC 4180）：双引号包裹字段、内部 "" 转义、字段内逗号/换行。
/// 未加引号的空字段返回 None（导入时视为 NULL）；带引号的空串 "" 保留为
/// 空字符串，与 NULL 区分，导出/导入可无损往返。
fn parse_csv(content: &str) -> Result<Vec<Vec<Option<String>>>, AppError> {
    let mut records: Vec<Vec<Option<String>>> = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut quoted = false; // 当前字段是否被双引号包裹（含空串 ""）
    let mut in_quotes = false; // 当前处于双引号字面量内部
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    // "" 转义：字面双引号
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => {
                    in_quotes = true;
                    quoted = true;
                }
                ',' => {
                    row.push(end_field(&mut field, quoted));
                    quoted = false;
                }
                '\n' => {
                    // 空行（无任何字段与内容）不产生记录：导出文件末尾的
                    // 多余空行不应变成一条空记录写入。区分空值行（如 ",,"，
                    // row 非空）与真正空行（row/field 全空）。
                    if row.is_empty() && field.is_empty() && !quoted {
                        continue;
                    }
                    row.push(end_field(&mut field, quoted));
                    quoted = false;
                    records.push(std::mem::take(&mut row));
                }
                // \r\n 换行由 \n 统一处理（导出端行尾符）；孤立 \r 忽略
                '\r' => {}
                _ => field.push(c),
            }
        }
    }
    if in_quotes {
        return Err(AppError::general("CSV 解析失败：引号未闭合"));
    }
    // EOF 收尾：还有未落行的字段/记录时补一行（空文件不产生记录）
    if !field.is_empty() || quoted || !row.is_empty() {
        row.push(end_field(&mut field, quoted));
        records.push(row);
    }
    Ok(records)
}

/// 字段结束：未加引号的空字段视为 NULL（None），其余保留字符串
fn end_field(field: &mut String, quoted: bool) -> Option<String> {
    if field.is_empty() && !quoted {
        None
    } else {
        Some(std::mem::take(field))
    }
}

/// 按分号拆分 SQL 语句（简化状态机：跳过单引号/双引号字面量内的分号，
/// 以及 -/`#` 行注释与 /* 块注释 */ 内的内容），返回去除首尾空白的语句列表。
///
/// 简化说明：`--` 无需后随空格即视为注释起始，属可接受的边界情况，
/// mysqldump 产物与前端 SQL 编辑器通常不产生此类输入。
fn split_sql_statements(content: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = content.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                current.push(c);
            }
            continue;
        }
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_single {
            match c {
                // 反斜杠转义下一字符（MySQL 默认模式）
                '\\' => {
                    if let Some(n) = chars.next() {
                        current.push('\\');
                        current.push(n);
                    }
                }
                // '' 转义：字面单引号
                '\'' => {
                    if chars.peek() == Some(&'\'') {
                        current.push('\'');
                        chars.next();
                    } else {
                        in_single = false;
                    }
                    current.push('\'');
                }
                _ => current.push(c),
            }
            continue;
        }
        if in_double {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        current.push('\\');
                        current.push(n);
                    }
                }
                // "" 转义：字面双引号
                '"' => {
                    if chars.peek() == Some(&'"') {
                        current.push('"');
                        chars.next();
                    } else {
                        in_double = false;
                    }
                    current.push('"');
                }
                _ => current.push(c),
            }
            continue;
        }
        match c {
            '\'' => {
                in_single = true;
                current.push(c);
            }
            '"' => {
                in_double = true;
                current.push(c);
            }
            '-' => {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    in_line_comment = true;
                } else {
                    current.push(c);
                }
            }
            '/' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    in_block_comment = true;
                } else {
                    current.push(c);
                }
            }
            // MySQL 单行注释 #...（# 后无需空格），与 -- 一致进入行注释；
            // 字符串字面量内的 # 已由上方 in_single/in_double 状态规避
            '#' => {
                in_line_comment = true;
            }
            ';' => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    statements.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
    statements
}

/// 导入前置校验：表名非空（CSV 格式必填；SQL 格式的语句自带表名）、文件存在
pub fn validate_import(opts: &MySqlImportOptions) -> Result<(), AppError> {
    if opts.format == MySqlImportFormat::Csv && opts.table.trim().is_empty() {
        return Err(AppError::general("导入表名为空"));
    }
    if opts.file_path.trim().is_empty() {
        return Err(AppError::general("导入文件路径为空"));
    }
    if !std::path::Path::new(&opts.file_path).exists() {
        return Err(AppError::general(format!(
            "导入文件不存在: {}",
            opts.file_path
        )));
    }
    Ok(())
}
/// 值 -> 契约的 Option<String>：NULL -> None，其余统一转字符串（与 services/mysql 同款实现）
fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::NULL => None,
        Value::Bytes(b) => Some(String::from_utf8_lossy(&b).into_owned()),
        Value::Int(i) => Some(i.to_string()),
        Value::UInt(u) => Some(u.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Double(d) => Some(d.to_string()),
        Value::Date(y, mo, d, h, mi, s, us) => {
            let micro = if us == 0 { String::new() } else { format!(".{us:06}") };
            Some(format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}{micro}"))
        }
        Value::Time(neg, days, h, m, s, us) => {
            let hours = days * 24 + h as u32;
            let sign = if neg { "-" } else { "" };
            let micro = if us == 0 { String::new() } else { format!(".{us:06}") };
            Some(format!("{sign}{hours:03}:{m:02}:{s:02}{micro}"))
        }
    }
}
