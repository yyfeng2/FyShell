//! MySQL 备份/还原 + 自动运行档案服务（对齐 Navicat 备份功能）。
//!
//! 连接复用 services::mysql 的公开助手（pool_of / take_tx_conn / give_tx_conn，
//! 与 mysql_io 等模块同模式），不触碰其私有注册表。
//!
//! mysqldump 风格备份：
//! - 逐表 SHOW CREATE TABLE 导出建表结构（保留引擎/字符集/索引等细节）；
//! - 可选 SELECT 全量拉取生成多值 INSERT（每 100 行一条）；
//! - 文件头部写 `-- FyShell backup` 注释与时间戳；std::fs::write 覆盖写入。
//!
//! 还原：读文件按分号拆分逐条执行（跳过字符串字面量与注释内的分号），
//! 全程事务包裹（用户显式事务内用 SAVEPOINT，否则 START TRANSACTION）。
//!
//! 自动运行（本轮范围）：保存任务配置 + 立即运行 + 运行历史。
//! 真实定时调度（cron）不在本轮范围，后续版本接入；档案持久化用 SQLite
//! （同 fyshell.db 独立 Connection，参照 tunnel.rs 模式，init(app_data_dir) 注入）。

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use mysql_async::prelude::*;
use mysql_async::{Conn, Value};
use rusqlite::params;

use crate::error::AppError;
use crate::models::mysql_backup::{MySqlBackupProfile, MySqlBackupRun};
use crate::services::mysql::{give_tx_conn, pool_of, take_tx_conn};

// ---------------------------------------------------------------------------
// 公共辅助（与 services/mysql_io.rs 等价的模块内实现——该文件的
// quote_ident / quote_value / value_to_string / split_sql_statements 均为私有）
// ---------------------------------------------------------------------------

/// mysql_async::Error -> AppError 归入 General 变体（与 mysql_io 同策略）
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

/// 值 -> 契约的 Option<String>：NULL -> None，其余统一转字符串（与 mysql_io 同款实现）
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
        // MySQL TIME 可超 24 小时（如 "838:59:59"），天数折算进小时
        Value::Time(neg, days, h, m, s, us) => {
            let hours = days * 24 + h as u32;
            let sign = if neg { "-" } else { "" };
            let micro = if us == 0 { String::new() } else { format!(".{us:06}") };
            Some(format!("{sign}{hours:03}:{m:02}:{s:02}{micro}"))
        }
    }
}

/// 按分号拆分 SQL 语句（简化状态机：跳过单引号/双引号字面量内的分号，
/// 以及 -- 行注释与 /* 块注释 */ 内的内容），返回去除首尾空白的语句列表。
/// 与 services/mysql_io.rs 的 split_sql_statements 同款实现。
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

// ---------------------------------------------------------------------------
// 备份 / 还原
// ---------------------------------------------------------------------------

/// mysql_backup：mysqldump 风格备份——逐表 SHOW CREATE TABLE（结构）+ 可选
/// SELECT 全量拉取生成多值 INSERT（每 100 行一条，值转义单引号）。
/// tables=None 时先经 information_schema 查全部表名。
/// 文件 std::fs::write 覆盖写入（前端保存对话框已让用户确认路径）。
/// 返回导出的数据行数（仅结构时为 0）。
pub async fn backup(
    conn_id: &str,
    tables: Option<&[String]>,
    file_path: &str,
    include_data: bool,
    include_drop: bool,
) -> Result<u64, AppError> {
    if file_path.trim().is_empty() {
        return Err(AppError::general("备份文件路径为空"));
    }

    // 优先复用事务内独占连接（备份只读，与用户会话同一上下文），用完放回
    let tx_conn = take_tx_conn(conn_id);
    let from_tx = tx_conn.is_some();
    let mut conn = match tx_conn {
        Some(conn) => conn,
        None => pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?,
    };
    let outcome = do_backup(&mut conn, tables, file_path, include_data, include_drop).await;
    if from_tx {
        give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 备份主体（拆出以便错误后仍归还事务连接）
async fn do_backup(
    conn: &mut Conn,
    tables: Option<&[String]>,
    file_path: &str,
    include_data: bool,
    include_drop: bool,
) -> Result<u64, AppError> {
    // tables=None：information_schema 查当前库全部表名（参照 mysql::list_tables 的兜底方式）
    let table_names: Vec<String> = match tables {
        Some(list) if !list.is_empty() => list.to_vec(),
        // None 与空列表语义统一（None = 全库所有表）
        _ => {
            let schema: String = conn
                .query_first::<Option<String>, _>("SELECT DATABASE()")
                .await
                .map_err(mysql_err)?
                .flatten()
                .ok_or_else(|| {
                    AppError::general("MySQL 未选择数据库（schema 为空），请指定默认库")
                })?;
            let sql = "SELECT TABLE_NAME FROM information_schema.TABLES \
                       WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
                       ORDER BY TABLE_NAME";
            let mut result = conn
                .exec_iter(sql, (schema,))
                .await
                .map_err(mysql_err)?;
            let mut rows = Vec::new();
            while let Some(mut row) = result.next().await.map_err(mysql_err)? {
                let name = row
                    .take::<Option<String>, _>(0)
                    .flatten()
                    .unwrap_or_default();
                rows.push(name);
            }
            result.drop_result().await.map_err(mysql_err)?;
            rows
        }
    };

    // 文件头部：FyShell 备份注释与时间戳
    let mut out = String::from("-- FyShell backup\n");
    out.push_str(&format!(
        "-- 生成时间: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));

    let mut rows_total = 0u64;
    for table in &table_names {
        let quoted = quote_ident(table)?;

        // 结构：SHOW CREATE TABLE 原文（保留引擎/字符集/索引等细节）
        let show_sql = format!("SHOW CREATE TABLE {quoted}");
        let (_, create): (Option<String>, Option<String>) = conn
            .query_first(&show_sql)
            .await
            .map_err(mysql_err)?
            .ok_or_else(|| AppError::general(format!("读取表结构失败，表不存在: {table}")))?;
        if let Some(create) = create {
            if include_drop {
                out.push_str(&format!("DROP TABLE IF EXISTS {quoted};\n"));
            }
            out.push_str(&format!("{create};\n\n"));
        }

        // 数据：SELECT * 全量拉取生成多值 INSERT（每 100 行一条）
        if include_data {
            rows_total += append_table_data(conn, table, &mut out).await?;
        }
    }

    std::fs::write(file_path, out)
        .map_err(|e| AppError::general(format!("写入备份文件失败: {e}")))?;
    Ok(rows_total)
}

/// 单表数据导出：SELECT 全量拉取（文本协议 query_iter，不分页），
/// 生成多值 INSERT（每 100 行一条，值转义单引号，NULL 输出 NULL）
async fn append_table_data(
    conn: &mut Conn,
    table: &str,
    out: &mut String,
) -> Result<u64, AppError> {
    let quoted = quote_ident(table)?;
    let select_sql = format!("SELECT * FROM {quoted}");
    let mut result = conn
        .query_iter(select_sql.as_str())
        .await
        .map_err(mysql_err)?;
    let columns: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let mut out_row = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            // take::<Option<Value>> 承接 NULL（None），再统一转字符串
            let value = row.take::<Option<Value>, _>(i).flatten();
            out_row.push(value.and_then(value_to_string));
        }
        rows.push(out_row);
    }
    // 消费剩余结果集/清理语句（全量遍历后调用是安全的）
    result.drop_result().await.map_err(mysql_err)?;

    if rows.is_empty() {
        return Ok(0);
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
    Ok(rows.len() as u64)
}

/// mysql_restore：读文件按分号拆分逐条执行（参照 mysql_io 的 SQL 导入状态机拆分），
/// 全程事务包裹，保证原子性。confirmed 流程与 mysql_io 的 replace 一致：
/// 覆盖确认由前端负责，后端直接执行（该参数仅作语义兼容，None/Some 均执行）。
/// 返回总受影响行数。
pub async fn restore(
    conn_id: &str,
    file_path: &str,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    // confirmed 仅作语义兼容：前端已负责确认，None/Some(true/false) 均直接执行
    let _ = confirmed;
    if file_path.trim().is_empty() {
        return Err(AppError::general("还原文件路径为空"));
    }
    if !std::path::Path::new(file_path).exists() {
        return Err(AppError::general(format!("还原文件不存在: {file_path}")));
    }

    // 用户显式事务内：SAVEPOINT 包裹，失败仅回滚到保存点、不影响用户事务其余内容
    if let Some(mut conn) = take_tx_conn(conn_id) {
        let outcome = with_savepoint(&mut conn, file_path).await;
        give_tx_conn(conn_id, conn);
        return outcome;
    }

    // 无活跃事务：START TRANSACTION 包裹整批，任一条失败整体 ROLLBACK
    let mut conn = pool_of(conn_id)?.get_conn().await.map_err(mysql_err)?;
    if let Err(e) = conn.query_drop("START TRANSACTION").await {
        // START TRANSACTION 失败时事务未开，无需回滚，连接随 drop 归还池
        return Err(mysql_err(e));
    }
    let rows_total = do_restore(&mut conn, file_path).await;
    match rows_total {
        Ok(rows_total) => {
            conn.query_drop("COMMIT").await.map_err(mysql_err)?;
            Ok(rows_total)
        }
        Err(e) => {
            // 任一条语句失败：回滚整批（隐式事务保证原子性）
            let _ = conn.query_drop("ROLLBACK").await;
            Err(e)
        }
    }
}

/// 还原执行主体：读取文件、按分号拆分并逐条执行，rows_total 取总受影响行数
async fn do_restore(conn: &mut Conn, file_path: &str) -> Result<u64, AppError> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| AppError::general(format!("读取还原文件失败: {e}")))?;
    let statements = split_sql_statements(&content);
    if statements.is_empty() {
        return Err(AppError::general(format!(
            "还原文件中未找到可执行的 SQL 语句: {file_path}"
        )));
    }
    let mut rows_total = 0u64;
    for (idx, stmt) in statements.iter().enumerate() {
        let mut result = match conn.query_iter(stmt.as_str()).await {
            Ok(result) => result,
            Err(e) => {
                return Err(AppError::general(format!(
                    "还原第 {} 条语句失败: {e}",
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

/// 用户事务内：SAVEPOINT 包裹本次还原，失败 ROLLBACK TO SAVEPOINT，成功 RELEASE
async fn with_savepoint(conn: &mut Conn, file_path: &str) -> Result<u64, AppError> {
    conn.query_drop("SAVEPOINT _fyshell_restore")
        .await
        .map_err(mysql_err)?;
    match do_restore(conn, file_path).await {
        Ok(rows_total) => {
            conn.query_drop("RELEASE SAVEPOINT _fyshell_restore")
                .await
                .map_err(mysql_err)?;
            Ok(rows_total)
        }
        Err(e) => {
            let _ = conn.query_drop("ROLLBACK TO SAVEPOINT _fyshell_restore").await;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// 自动运行档案持久化（SQLite，同 fyshell.db 独立 Connection，参照 tunnel.rs 模式）
// ---------------------------------------------------------------------------

/// SQLite 连接注册表：与 config_store / tunnel 同用 fyshell.db，但持独立
/// Connection，避免与各模块内部锁互相竞争。路径依赖 Tauri API（只能在应用
/// 启动后获取），故用模块级 OnceLock<Mutex<Connection>> + init(app_data_dir) 注入。
static CONN: OnceLock<Mutex<rusqlite::Connection>> = OnceLock::new();

/// 锁住 SQLite 连接（init 未调用视为编程错误）
fn lock_conn() -> MutexGuard<'static, rusqlite::Connection> {
    CONN.get()
        .expect("mysql_backup 数据库未初始化（init 未调用）")
        .lock()
        .expect("mysql_backup 数据库连接锁中毒")
}

/// 初始化：打开同 fyshell.db（独立 Connection）并完成建表迁移。
/// 由应用启动流程（setup / 集成层）调用一次；重复调用幂等。
/// - backup_profiles：备份档案配置表；
/// - backup_runs：备份运行历史表。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    // 建库时自动创建目录
    std::fs::create_dir_all(app_data_dir)?;
    let conn = rusqlite::Connection::open(app_data_dir.join("fyshell.db"))?;
    let cell = CONN.get_or_init(|| Mutex::new(conn));
    let guard = cell.lock().expect("mysql_backup 数据库连接锁中毒");
    guard.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS backup_profiles (
             id             TEXT PRIMARY KEY,
             name           TEXT NOT NULL,
             conn_host      TEXT NOT NULL,
             tables         TEXT NOT NULL,
             include_data   INTEGER NOT NULL,
             include_create INTEGER NOT NULL,
             created_at     TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS backup_runs (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             profile_id  TEXT,
             file_path   TEXT NOT NULL,
             rows_total  INTEGER NOT NULL,
             duration_ms INTEGER NOT NULL DEFAULT 0,
             created_at  TEXT NOT NULL,
             success     INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_backup_runs_profile ON backup_runs (profile_id);
         COMMIT;",
    )?;
    Ok(())
}

/// 行 -> MySqlBackupProfile 的公共映射函数（tables 列为 JSON 数组字符串）
fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<MySqlBackupProfile> {
    let tables: String = row.get("tables")?;
    Ok(MySqlBackupProfile {
        id: row.get("id")?,
        name: row.get("name")?,
        conn_host: row.get("conn_host")?,
        // tables 列存 JSON 数组字符串，解析失败视为空（= 全库）
        tables: serde_json::from_str(&tables).unwrap_or_default(),
        include_data: {
            let v: i64 = row.get("include_data")?;
            v != 0
        },
        include_create: {
            let v: i64 = row.get("include_create")?;
            v != 0
        },
        created_at: row.get("created_at")?,
    })
}

/// 备份档案列表（按名称排序，展示顺序稳定可预期）
pub fn profile_list() -> Result<Vec<MySqlBackupProfile>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, conn_host, tables, include_data, include_create, created_at
         FROM backup_profiles
         ORDER BY name",
    )?;
    let profiles: Vec<MySqlBackupProfile> = stmt
        .query_map([], row_to_profile)?
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// 保存备份档案（按 id upsert；新档案的 id 由 commands 层生成后传入）
pub fn profile_save(profile: &MySqlBackupProfile) -> Result<(), AppError> {
    // 参数校验（薄校验留在服务层，commands 层保持更薄）
    if profile.name.trim().is_empty() {
        return Err(AppError::general("备份档案名称不能为空"));
    }
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO backup_profiles (id, name, conn_host, tables, include_data,
                                      include_create, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
             name = excluded.name,
             conn_host = excluded.conn_host,
             tables = excluded.tables,
             include_data = excluded.include_data,
             include_create = excluded.include_create",
        params![
            profile.id,
            profile.name,
            profile.conn_host,
            serde_json::to_string(&profile.tables)?,
            profile.include_data as i64,
            profile.include_create as i64,
            profile.created_at,
        ],
    )?;
    Ok(())
}

/// 删除单个备份档案（运行历史保留，不级联删除）
pub fn profile_delete(id: &str) -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute("DELETE FROM backup_profiles WHERE id = ?1", [id])?;
    Ok(())
}

/// 新增一条备份运行历史（由执行方在备份完成后调用：
/// 本轮「立即运行」由命令层记录；真实 cron 调度接入后由调度器记录）
pub fn run_history_add(
    profile_id: Option<&str>,
    file_path: &str,
    rows_total: u64,
    success: bool,
) -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO backup_runs (profile_id, file_path, rows_total, duration_ms, created_at, success)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)",
        params![
            profile_id,
            file_path,
            rows_total as i64,
            chrono::Local::now().to_rfc3339(),
            success as i64,
        ],
    )?;
    Ok(())
}

/// 备份运行历史列表（按 id 倒序，limit 限制条数）
pub fn run_history_list(limit: u32) -> Result<Vec<MySqlBackupRun>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, profile_id, file_path, rows_total, duration_ms, created_at, success
         FROM backup_runs
         ORDER BY id DESC
         LIMIT ?1",
    )?;
    let rows: Vec<MySqlBackupRun> = stmt
        .query_map([i64::from(limit)], |row| {
            Ok(MySqlBackupRun {
                id: row.get("id")?,
                profile_id: row.get("profile_id")?,
                file_path: row.get("file_path")?,
                rows_total: {
                    let v: i64 = row.get("rows_total")?;
                    v.max(0) as u64
                },
                duration_ms: {
                    let v: i64 = row.get("duration_ms")?;
                    v.max(0) as u64
                },
                created_at: row.get("created_at")?,
                success: {
                    let v: i64 = row.get("success")?;
                    v != 0
                },
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}
