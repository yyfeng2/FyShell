//! MySQL 工具级功能服务（对齐 Navicat 的数据传输/数据生成/数据同步/结构同步）。
//!
//! 连接复用 services::mysql 的 pool_of / take_tx_conn / give_tx_conn；
//! 目标连接由前端 mysql_connect 预先建立（target_conn_id 直接 pool_of 可得），
//! 同一连接可同时充当源与目标（同服务器跨库，池内取两条连接互不干扰）。
//!
//! 防注入：库/表名来自前端，属 SQL 标识符，统一校验非空、不含反引号后反引号包裹；
//! 值按字符串字面量写入（先转反斜杠再成对转义单引号，与 mysql_io.rs 同策略）。
//!
//! 跨库限定：全部 SQL 以 `db`.`table` 限定，不依赖连接当前库上下文
//! （池连接归还时会 COM_RESET_CONNECTION 重置当前库，USE 不可靠）。
//!
//! 事务语义：源端读取优先复用事务内独占连接（与用户会话同一上下文），
//! 目标端写入一律取池连接；传输/同步中途出错不回滚已写入的批次
//! （目标表可由 recreate 重新传输覆盖，属桌面工具可接受语义）。

use mysql_async::prelude::*;
use mysql_async::Conn;
use rand::Rng;
use rand::SeedableRng;

use crate::error::AppError;
use crate::models::mysql_tools::{
    MySqlDataSyncOptions, MySqlDataSyncOutcome, MySqlGenerateColumnRule, MySqlGenerateOptions,
    MySqlStructureSyncItem, MySqlStructureSyncOptions, MySqlStructureSyncPlan,
    MySqlTransferOptions, MySqlTransferTableResult,
};
use crate::services::mysql::{give_tx_conn, pool_of, take_tx_conn};

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql 同款归一化）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 标识符基础校验：非空且不含反引号（反引号为 MySQL 标识符转义符，直接拒绝）
fn is_valid_identifier(name: &str) -> bool {
    !name.is_empty() && !name.contains('`')
}

/// 库名加反引号包裹（调用前已校验）
fn quote_db(name: &str) -> String {
    format!("`{name}`")
}

/// 表名校验并加反引号包裹（防注入）
fn quote_table(name: &str) -> Result<String, AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("表名非法: {name}")));
    }
    Ok(format!("`{name}`"))
}

/// 值 -> SQL 字面量：单引号包裹并转义（先转反斜杠再成对转义单引号，避免 `\'` 歧义）
fn quote_value(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

/// 取连接：优先复用事务内独占连接，否则从池中取
/// 返回 `bool` 表示是否来自事务连接；事务连接用完需 `give_tx_conn` 放回。
async fn take_tx_or_pool(conn_id: &str) -> Result<(Conn, bool), AppError> {
    if let Some(conn) = take_tx_conn(conn_id) {
        return Ok((conn, true));
    }
    let pool = pool_of(conn_id)?;
    let conn = pool.get_conn().await.map_err(mysql_err)?;
    Ok((conn, false))
}

/// 执行 SELECT 并收集全部行（列按位置，None = NULL），同时返回列名列表
async fn collect_rows(
    conn: &mut Conn,
    sql: &str,
) -> Result<(Vec<String>, Vec<Vec<Option<String>>>), AppError> {
    let mut result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let columns: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut rows = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let len = row.columns().len();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(row.take::<Option<String>, _>(i).flatten());
        }
        rows.push(out);
    }
    result.drop_result().await.map_err(mysql_err)?;
    Ok((columns, rows))
}

/// 行数据 -> SQL 值元组列表（NULL -> NULL 字面量）
fn row_to_values(row: &[Option<String>]) -> String {
    let cells: Vec<String> = row
        .iter()
        .map(|c| c.as_deref().map(quote_value).unwrap_or_else(|| "NULL".into()))
        .collect();
    format!("({})", cells.join(", "))
}

/// SHOW CREATE TABLE 取完整 DDL（`db`.`table` 限定，不依赖连接当前库）
async fn show_create(conn: &mut Conn, db: &str, table: &str) -> Result<String, AppError> {
    let sql = format!("SHOW CREATE TABLE {}.{}", quote_db(db), quote_table(table)?);
    let mut result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let cols: Vec<String> = result
        .columns()
        .map(|cols| cols.iter().map(|c| c.name_str().into_owned()).collect())
        .unwrap_or_default();
    let mut sql_text: Option<String> = None;
    if let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let idx = cols
            .iter()
            .position(|c| c.to_ascii_lowercase() == "create table");
        if let Some(i) = idx {
            sql_text = row.take::<Option<String>, _>(i).flatten();
        }
    }
    result.drop_result().await.map_err(mysql_err)?;
    sql_text.ok_or_else(|| AppError::general(format!("无法获取表 {db}.{table} 的建表语句")))
}

/// 建表语句表名重限定：`CREATE TABLE \`old\`` -> `CREATE TABLE \`target_db\`.\`old\``
///
/// 仅替换第一处（CREATE TABLE 的表名位置），表体中的字面量不受影响。
fn replace_create_table_name(ddl: &str, target_db: &str, table: &str) -> String {
    let old = format!("CREATE TABLE {}", quote_db(table));
    let new = format!("CREATE TABLE {}.{}", quote_db(target_db), quote_db(table));
    match ddl.find(&old) {
        Some(pos) => format!("{}{}{}", &ddl[..pos], new, &ddl[pos + old.len()..]),
        None => ddl.to_string(),
    }
}

// ---------------- 数据传输 ----------------

/// data_transfer：跨连接复制表结构与数据
///
/// 逐表：SHOW CREATE TABLE 源 -> 目标端（recreate 时先 DROP）CREATE ->
/// SELECT 源 -> 每 100 行多值 INSERT 目标。全部 SQL 以 `db`.`table` 限定。
pub async fn data_transfer(
    source_conn_id: &str,
    opts: &MySqlTransferOptions,
) -> Result<Vec<MySqlTransferTableResult>, AppError> {
    if opts.tables.is_empty() {
        return Err(AppError::general("未选择要传输的表"));
    }
    if !opts.include_structure && !opts.include_data {
        return Err(AppError::general("请选择传输内容（结构/数据）"));
    }
    let source_db = opts.source_db.trim().to_string();
    let target_db = opts.target_db.trim().to_string();
    if !is_valid_identifier(&source_db) {
        return Err(AppError::general(format!("源库名非法: {source_db}")));
    }
    if !is_valid_identifier(&target_db) {
        return Err(AppError::general(format!("目标库名非法: {target_db}")));
    }
    for t in &opts.tables {
        if !is_valid_identifier(t) {
            return Err(AppError::general(format!("表名非法: {t}")));
        }
    }

    let (mut src, from_tx) = take_tx_or_pool(source_conn_id).await?;
    let mut dst = pool_of(&opts.target_conn_id)?
        .get_conn()
        .await
        .map_err(mysql_err)?;
    let outcome = do_transfer(&mut src, &mut dst, opts, &source_db, &target_db).await;
    if from_tx {
        give_tx_conn(source_conn_id, src);
    }
    outcome
}

/// 传输主体（拆出以便错误早退时仍归还事务连接）
async fn do_transfer(
    src: &mut Conn,
    dst: &mut Conn,
    opts: &MySqlTransferOptions,
    source_db: &str,
    target_db: &str,
) -> Result<Vec<MySqlTransferTableResult>, AppError> {
    let mut results = Vec::new();
    for table in &opts.tables {
        let src_qualified = format!("{}.{}", quote_db(source_db), quote_table(table)?);
        let dst_qualified = format!("{}.{}", quote_db(target_db), quote_table(table)?);

        // 目标表是否已存在（information_schema 计数，不依赖连接当前库）
        let exists_sql = format!(
            "SELECT COUNT(*) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}'",
            target_db.replace('\'', "''"),
            table.replace('\'', "''")
        );
        let exists: u64 = dst
            .query_first(exists_sql.as_str())
            .await
            .map_err(mysql_err)?
            .unwrap_or(0);

        if opts.include_structure {
            if exists > 0 && !opts.recreate {
                // 目标表已存在且未开覆盖重建：跳过结构（数据仍按 include_data 追加）
                let rows = if opts.include_data {
                    copy_rows(src, dst, &src_qualified, &dst_qualified).await?
                } else {
                    0
                };
                results.push(MySqlTransferTableResult {
                    table: table.clone(),
                    rows,
                    skipped: true,
                });
                continue;
            }
            if exists > 0 {
                // 覆盖重建：先 DROP（目标端）
                let drop_sql = format!("DROP TABLE {}.{}", quote_db(target_db), quote_table(table)?);
                dst.query_drop(&drop_sql).await.map_err(mysql_err)?;
            }
            // 源端取建表语句，重限定到目标库名后在目标端执行
            let ddl = show_create(src, source_db, table).await?;
            let qualified_ddl = replace_create_table_name(&ddl, target_db, table);
            dst.query_drop(&qualified_ddl).await.map_err(mysql_err)?;
        } else if exists == 0 {
            return Err(AppError::general(format!(
                "目标表 {target_db}.{table} 不存在且未勾选传输结构"
            )));
        }

        let rows = if opts.include_data {
            copy_rows(src, dst, &src_qualified, &dst_qualified).await?
        } else {
            0
        };
        results.push(MySqlTransferTableResult {
            table: table.clone(),
            rows,
            skipped: false,
        });
    }
    Ok(results)
}

/// 复制单表数据：SELECT 源 -> 每 100 行多值 INSERT 目标
///
/// 列名取自源端结果集（与 SELECT * 顺序一致），NULL 直接写 NULL 字面量。
async fn copy_rows(
    src: &mut Conn,
    dst: &mut Conn,
    src_qualified: &str,
    dst_qualified: &str,
) -> Result<u64, AppError> {
    let (columns, rows) = collect_rows(src, &format!("SELECT * FROM {src_qualified}")).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let col_list = columns
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut total = 0u64;
    for batch in rows.chunks(100) {
        let values = batch
            .iter()
            .map(|row| row_to_values(row))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO {dst_qualified} ({col_list}) VALUES {values}");
        dst.query_drop(&sql).await.map_err(mysql_err)?;
        total += batch.len() as u64;
    }
    Ok(total)
}

// ---------------- 数据生成 ----------------

/// data_generate：单表按列规则批量生成测试数据
///
/// 列规则经 information_schema.COLUMNS 校验（规则列必须真实存在）；
/// 未列入规则的列取表默认值（INSERT 列清单只含规则列）。
pub async fn data_generate(conn_id: &str, opts: &MySqlGenerateOptions) -> Result<u64, AppError> {
    let table = opts.table.trim().to_string();
    quote_table(&table)?;
    if opts.rows == 0 || opts.rows > 100_000 {
        return Err(AppError::general("生成行数需在 1-100000 之间"));
    }
    if opts.columns.is_empty() {
        return Err(AppError::general("未配置任何列的生成规则"));
    }
    for rule in &opts.columns {
        if !is_valid_identifier(rule.name.trim()) {
            return Err(AppError::general(format!("列名非法: {}", rule.name)));
        }
        if !matches!(
            rule.kind.as_str(),
            "int"
                | "decimal"
                | "string"
                | "uuid"
                | "name"
                | "phone"
                | "email"
                | "datetime"
                | "fixed"
        ) {
            return Err(AppError::general(format!("未知生成类型: {}", rule.kind)));
        }
    }

    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = do_generate(&mut conn, opts, &table).await;
    if from_tx {
        give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 数据生成主体（拆出以便错误早退时仍归还事务连接）
async fn do_generate(
    conn: &mut Conn,
    opts: &MySqlGenerateOptions,
    table: &str,
) -> Result<u64, AppError> {
    // 实际列清单（当前连接的当前库上下文；表名已校验无反引号）
    let meta_sql = format!(
        "SELECT COLUMN_NAME FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{}' ORDER BY ORDINAL_POSITION",
        table.replace('\'', "''")
    );
    let (_, rows) = collect_rows(conn, &meta_sql).await?;
    let columns: Vec<String> = rows
        .into_iter()
        .map(|row| row.into_iter().flatten().next().unwrap_or_default())
        .collect();
    for rule in &opts.columns {
        if !columns.contains(&rule.name.trim().to_string()) {
            return Err(AppError::general(format!(
                "列「{}」在表 {table} 中不存在",
                rule.name.trim()
            )));
        }
    }

    if opts.truncate {
        conn.query_drop(format!("TRUNCATE TABLE {}", quote_table(table)?))
            .await
            .map_err(mysql_err)?;
    }

    let table_qualified = quote_table(table)?;
    let col_list = rule_columns(opts);
    // StdRng（Send）：Tauri command 的 future 须跨 await 持有 rng，ThreadRng 非 Send
    let mut rng = rand::rngs::StdRng::from_entropy();
    let mut total = 0u64;

    // 每 100 行一条多值 INSERT
    let mut batch: Vec<String> = Vec::with_capacity(100);
    for _ in 0..opts.rows {
        let cells: Vec<String> = opts
            .columns
            .iter()
            .map(|rule| generate_value(rule, &mut rng))
            // NULL 保留字面量（不加引号），其余值单引号包裹并转义
            .map(|v| if v == "NULL" { v } else { quote_value(&v) })
            .collect();
        batch.push(format!("({})", cells.join(", ")));
        if batch.len() >= 100 {
            let sql = format!(
                "INSERT INTO {} ({}) VALUES {}",
                table_qualified, col_list, batch.join(", ")
            );
            conn.query_drop(&sql).await.map_err(mysql_err)?;
            total += batch.len() as u64;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            table_qualified, col_list, batch.join(", ")
        );
        conn.query_drop(&sql).await.map_err(mysql_err)?;
        total += batch.len() as u64;
    }
    Ok(total)
}

/// 规则列清单（反引号包裹，逗号连接）
fn rule_columns(opts: &MySqlGenerateOptions) -> String {
    opts.columns
        .iter()
        .map(|r| format!("`{}`", r.name.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 按列规则生成单格值（null_ratio 命中输出 NULL）
fn generate_value(rule: &MySqlGenerateColumnRule, rng: &mut rand::rngs::StdRng) -> String {
    if let Some(ratio) = rule.null_ratio {
        if ratio > 0 && rng.gen_range(0..100) < ratio {
            return "NULL".to_string();
        }
    }
    let min = rule.min.unwrap_or(0.0);
    let max = rule.max.unwrap_or(10000.0);
    match rule.kind.as_str() {
        "int" => {
            let lo = min as i64;
            let hi = (max as i64).max(lo);
            rng.gen_range(lo..=hi).to_string()
        }
        "decimal" => format!("{:.2}", rng.gen_range(min..=max)),
        "string" => {
            let len = rule.length.unwrap_or(8).max(1) as usize;
            random_string(rng, len)
        }
        "uuid" => uuid::Uuid::new_v4().to_string(),
        "name" => {
            let surnames = ["张", "王", "李", "赵", "刘", "陈", "杨", "黄", "周", "吴"];
            let givens = [
                "伟", "芳", "娜", "敏", "静", "磊", "军", "洋", "勇", "艳", "杰", "秀英", "文轩",
                "雨婷", "子涵", "浩然",
            ];
            format!(
                "{}{}",
                surnames[rng.gen_range(0..surnames.len())],
                givens[rng.gen_range(0..givens.len())]
            )
        }
        "phone" => {
            let prefixes = ["3", "5", "7", "8", "9"];
            format!(
                "1{}{:09}",
                prefixes[rng.gen_range(0..prefixes.len())],
                rng.gen_range(0..1_000_000_000)
            )
        }
        "email" => {
            let user = random_string(rng, 8);
            let domains = ["example.com", "test.org", "mail.cn", "demo.net"];
            format!("{user}@{}", domains[rng.gen_range(0..domains.len())])
        }
        "datetime" => {
            // 近一年内的随机时刻（秒级 Unix 时间）
            let now = chrono::Utc::now().timestamp();
            let ts = rng.gen_range(now - 365 * 24 * 3600..=now);
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        }
        "fixed" => {
            if rule.values.is_empty() {
                String::new()
            } else {
                rule.values[rng.gen_range(0..rule.values.len())].clone()
            }
        }
        _ => String::new(),
    }
}

/// 随机字母数字字符串
fn random_string(rng: &mut rand::rngs::StdRng, len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

// ---------------- 数据同步 ----------------

/// mysql_data_sync：按主键比对并同步两表数据
///
/// execute=false 仅比对返回差异计数与样例；execute=true 按选项应用
/// （缺失行 INSERT / 多余行 DELETE / 不一致行 REPLACE INTO）。
pub async fn data_sync(
    source_conn_id: &str,
    opts: &MySqlDataSyncOptions,
    execute: bool,
) -> Result<MySqlDataSyncOutcome, AppError> {
    let source_db = opts.source_db.trim().to_string();
    let target_db = opts.target_db.trim().to_string();
    for (label, name) in [
        ("源库名", &source_db),
        ("目标库名", &target_db),
        ("源表名", &opts.source_table),
        ("目标表名", &opts.target_table),
    ] {
        if !is_valid_identifier(name) {
            return Err(AppError::general(format!("{label}非法: {name}")));
        }
    }

    let (mut src, from_tx) = take_tx_or_pool(source_conn_id).await?;
    let mut dst = pool_of(&opts.target_conn_id)?
        .get_conn()
        .await
        .map_err(mysql_err)?;
    let outcome = do_data_sync(&mut src, &mut dst, opts, execute).await;
    if from_tx {
        give_tx_conn(source_conn_id, src);
    }
    outcome
}

/// 同步主体（拆出以便错误早退时仍归还事务连接）
async fn do_data_sync(
    src: &mut Conn,
    dst: &mut Conn,
    opts: &MySqlDataSyncOptions,
    execute: bool,
) -> Result<MySqlDataSyncOutcome, AppError> {
    // 1. 双侧主键（KEY_COLUMN_USAGE 按列序）
    let src_keys = primary_key_columns(src, &opts.source_db, &opts.source_table).await?;
    let dst_keys = primary_key_columns(dst, &opts.target_db, &opts.target_table).await?;
    if src_keys.is_empty() || dst_keys.is_empty() {
        return Err(AppError::general(
            "两表主键缺失，无法按主键同步（请先为两表定义主键）",
        ));
    }
    if src_keys != dst_keys {
        return Err(AppError::general(format!(
            "两表主键结构不一致（源 {src_keys:?} / 目标 {dst_keys:?}），无法按主键同步"
        )));
    }

    // 2. 双侧全量拉取
    let src_sql = format!(
        "SELECT * FROM {}.{}",
        quote_db(&opts.source_db),
        quote_table(&opts.source_table)?
    );
    let dst_sql = format!(
        "SELECT * FROM {}.{}",
        quote_db(&opts.target_db),
        quote_table(&opts.target_table)?
    );
    let (src_cols, src_rows) = collect_rows(src, &src_sql).await?;
    let (dst_cols, dst_rows) = collect_rows(dst, &dst_sql).await?;
    // 列序必须完全一致（同列名同顺序）：行值按源列序写入目标列清单，
    // 顺序不同会写错列，直接拒绝并引导用户先走结构同步
    if src_cols != dst_cols {
        return Err(AppError::general(
            "两表列结构不一致（列名或列序不同），请先执行结构同步",
        ));
    }
    let key_idx_src = key_indices(&src_cols, &src_keys)?;
    let key_idx_dst = key_indices(&dst_cols, &dst_keys)?;

    // 3. PK 元组比对：仅源有 / 仅目标有 / 两端都有但非主键列不一致
    let dst_map: std::collections::HashMap<String, Vec<Option<String>>> = dst_rows
        .iter()
        .map(|row| (tuple_key(row, &key_idx_dst), row.clone()))
        .collect();
    let src_map: std::collections::HashMap<String, Vec<Option<String>>> = src_rows
        .iter()
        .map(|row| (tuple_key(row, &key_idx_src), row.clone()))
        .collect();

    let mut only_source: Vec<Vec<Option<String>>> = Vec::new();
    let mut changed: Vec<Vec<Option<String>>> = Vec::new();
    for row in &src_rows {
        let key = tuple_key(row, &key_idx_src);
        match dst_map.get(&key) {
            None => only_source.push(row.clone()),
            Some(dst_row) => {
                if !same_row(row, dst_row, &key_idx_src) {
                    changed.push(row.clone());
                }
            }
        }
    }
    let mut only_target: Vec<Vec<Option<String>>> = Vec::new();
    for row in &dst_rows {
        if !src_map.contains_key(&tuple_key(row, &key_idx_dst)) {
            only_target.push(row.clone());
        }
    }

    let outcome = MySqlDataSyncOutcome {
        key_columns: src_keys.clone(),
        only_source: only_source.len() as u64,
        only_target: only_target.len() as u64,
        changed: changed.len() as u64,
        sample_source: sample_keys(&only_source, &key_idx_src),
        sample_target: sample_keys(&only_target, &key_idx_dst),
        sample_changed: sample_keys(&changed, &key_idx_src),
    };

    if !execute {
        return Ok(outcome);
    }

    // 4. 应用（目标端）：缺失行 INSERT / 多余行 DELETE / 不一致行 REPLACE
    let dst_qualified = format!("{}.{}", quote_db(&opts.target_db), quote_table(&opts.target_table)?);
    let dst_col_list = dst_cols
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");

    if opts.insert_missing && !only_source.is_empty() {
        for batch in only_source.chunks(100) {
            let values = batch
                .iter()
                .map(|row| row_to_values(row))
                .collect::<Vec<_>>()
                .join(", ");
            dst.query_drop(&format!(
                "INSERT INTO {dst_qualified} ({dst_col_list}) VALUES {values}"
            ))
            .await
            .map_err(mysql_err)?;
        }
    }
    if opts.delete_extra && !only_target.is_empty() {
        for batch in only_target.chunks(100) {
            let conds: Vec<String> = batch
                .iter()
                .map(|row| {
                    // 主键列按 key_idx_dst 顺序对齐 src_keys（主键一致已校验）
                    let parts: Vec<String> = key_idx_dst
                        .iter()
                        .enumerate()
                        .map(|(i, col_idx)| {
                            let value = row
                                .get(*col_idx)
                                .cloned()
                                .flatten()
                                .unwrap_or_default();
                            format!("`{}` = {}", src_keys[i], quote_value(&value))
                        })
                        .collect();
                    parts.join(" AND ")
                })
                .collect();
            dst.query_drop(&format!(
                "DELETE FROM {dst_qualified} WHERE {}",
                conds.join(" OR ")
            ))
            .await
            .map_err(mysql_err)?;
        }
    }
    if opts.update_diff && !changed.is_empty() {
        for batch in changed.chunks(100) {
            let values = batch
                .iter()
                .map(|row| row_to_values(row))
                .collect::<Vec<_>>()
                .join(", ");
            dst.query_drop(&format!(
                "REPLACE INTO {dst_qualified} ({dst_col_list}) VALUES {values}"
            ))
            .await
            .map_err(mysql_err)?;
        }
    }
    Ok(outcome)
}

/// 读表主键列（KEY_COLUMN_USAGE，CONSTRAINT_NAME='PRIMARY'，按列序）
async fn primary_key_columns(
    conn: &mut Conn,
    db: &str,
    table: &str,
) -> Result<Vec<String>, AppError> {
    let db_lit = db.replace('\'', "''");
    let table_lit = table.replace('\'', "''");
    let sql = format!(
        "SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE \
         WHERE TABLE_SCHEMA = '{db_lit}' AND TABLE_NAME = '{table_lit}' \
         AND CONSTRAINT_NAME = 'PRIMARY' ORDER BY ORDINAL_POSITION"
    );
    let (_, rows) = collect_rows(conn, &sql).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.into_iter().flatten().next().unwrap_or_default())
        .collect())
}

/// 列名 -> SELECT * 中的位置（主键在行数据中的索引）
fn key_indices(all: &[String], keys: &[String]) -> Result<Vec<usize>, AppError> {
    keys.iter()
        .map(|k| {
            all.iter()
                .position(|c| c == k)
                .ok_or_else(|| AppError::general(format!("主键列 {k} 不在表列中")))
        })
        .collect()
}

/// 行 -> 主键元组键（多列以 \x00 分隔，避免值内分隔符歧义）
fn tuple_key(row: &[Option<String>], key_idx: &[usize]) -> String {
    key_idx
        .iter()
        .map(|i| row.get(*i).cloned().flatten().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\x00")
}

/// 两端行内容一致性（跳过主键列逐列比对；列数不同视为不一致，引导走结构同步）
fn same_row(src: &[Option<String>], dst: &[Option<String>], key_idx: &[usize]) -> bool {
    if src.len() != dst.len() {
        return false;
    }
    for (i, s) in src.iter().enumerate() {
        if key_idx.contains(&i) {
            continue;
        }
        if s != &dst[i] {
            return false;
        }
    }
    true
}

/// 差异样例：仅保留主键值（按主键列序，限 20 条）
fn sample_keys(rows: &[Vec<Option<String>>], key_idx: &[usize]) -> Vec<Vec<String>> {
    rows.iter()
        .take(20)
        .map(|row| {
            key_idx
                .iter()
                .map(|i| row.get(*i).cloned().flatten().unwrap_or_default())
                .collect()
        })
        .collect()
}

// ---------------- 结构同步 ----------------

/// structure_sync：比对两库表结构并（可选）同步
///
/// execute=false 返回逐表差异计划（缺失表 CREATE + 列差异 ALTER）；
/// execute=true 在目标端逐条执行计划中的 SQL。
pub async fn structure_sync(
    source_conn_id: &str,
    opts: &MySqlStructureSyncOptions,
    execute: bool,
) -> Result<MySqlStructureSyncPlan, AppError> {
    let source_db = opts.source_db.trim().to_string();
    let target_db = opts.target_db.trim().to_string();
    if !is_valid_identifier(&source_db) {
        return Err(AppError::general(format!("源库名非法: {source_db}")));
    }
    if !is_valid_identifier(&target_db) {
        return Err(AppError::general(format!("目标库名非法: {target_db}")));
    }

    let (mut src, from_tx) = take_tx_or_pool(source_conn_id).await?;
    let mut dst = pool_of(&opts.target_conn_id)?
        .get_conn()
        .await
        .map_err(mysql_err)?;
    let outcome =
        do_structure_sync(&mut src, &mut dst, opts, &source_db, &target_db, execute).await;
    if from_tx {
        give_tx_conn(source_conn_id, src);
    }
    outcome
}

/// 结构同步主体（拆出以便错误早退时仍归还事务连接）
async fn do_structure_sync(
    src: &mut Conn,
    dst: &mut Conn,
    opts: &MySqlStructureSyncOptions,
    source_db: &str,
    target_db: &str,
    execute: bool,
) -> Result<MySqlStructureSyncPlan, AppError> {
    let _ = opts; // 库名校验已在调用方完成，此处仅用 source_db/target_db
    // 双侧列定义（TABLE_SCHEMA 过滤，按表/列序分组）
    let src_meta = read_columns(src, source_db).await?;
    let dst_meta = read_columns(dst, target_db).await?;

    let mut items = Vec::new();
    for (table, src_cols) in &src_meta {
        let dst_qualified = format!("{}.{}", quote_db(target_db), quote_table(table)?);
        match dst_meta.get(table) {
            None => {
                // 目标缺失：SHOW CREATE TABLE 源，表名重限定到目标库
                let ddl = show_create(src, source_db, table).await?;
                items.push(MySqlStructureSyncItem {
                    table: table.clone(),
                    kind: "create".to_string(),
                    sql: replace_create_table_name(&ddl, target_db, table),
                });
            }
            Some(dst_cols) => {
                let mut clauses: Vec<String> = Vec::new();
                // 目标缺失的列 -> ADD（源端定义重建）
                for (name, def) in src_cols {
                    if !dst_cols.contains_key(name) {
                        clauses.push(format!("ADD COLUMN {def}"));
                    }
                }
                // 目标多出的列 -> DROP
                for name in dst_cols.keys() {
                    if !src_cols.contains_key(name) {
                        clauses.push(format!("DROP COLUMN `{name}`"));
                    }
                }
                // 两端都有但定义不同 -> MODIFY（以源端定义为准）
                for (name, src_def) in src_cols {
                    if let Some(dst_def) = dst_cols.get(name) {
                        if normalized_def(src_def) != normalized_def(dst_def) {
                            clauses.push(format!("MODIFY COLUMN {src_def}"));
                        }
                    }
                }
                if !clauses.is_empty() {
                    items.push(MySqlStructureSyncItem {
                        table: table.clone(),
                        kind: "alter".to_string(),
                        sql: format!("ALTER TABLE {dst_qualified} {}", clauses.join(", ")),
                    });
                } else {
                    items.push(MySqlStructureSyncItem {
                        table: table.clone(),
                        kind: "same".to_string(),
                        sql: String::new(),
                    });
                }
            }
        }
    }

    // execute：逐条执行（create 单语句 / alter 单语句多子句）
    if execute {
        for item in &items {
            if item.kind == "same" {
                continue;
            }
            dst.query_drop(&item.sql).await.map_err(mysql_err)?;
        }
    }
    Ok(MySqlStructureSyncPlan { items })
}

/// 读取指定库全部表的列定义（information_schema.COLUMNS，按表/列序分组）
///
/// 每列重建为 `` `name` COLUMN_TYPE [NOT NULL] [DEFAULT xxx] [COMMENT 'xxx'] [EXTRA] ``
/// 形式的定义串（供 ADD/MODIFY 子句直接使用）。
async fn read_columns(
    conn: &mut Conn,
    db: &str,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>, AppError>
{
    let db_lit = db.replace('\'', "''");
    let sql = format!(
        "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_COMMENT, EXTRA \
         FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '{db_lit}' ORDER BY TABLE_NAME, ORDINAL_POSITION"
    );
    let mut result = conn.query_iter(sql).await.map_err(mysql_err)?;
    let mut out: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> =
        std::collections::BTreeMap::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let table = row.take::<Option<String>, _>(0).flatten().unwrap_or_default();
        let name = row.take::<Option<String>, _>(1).flatten().unwrap_or_default();
        let ctype = row.take::<Option<String>, _>(2).flatten().unwrap_or_default();
        let nullable = row.take::<Option<String>, _>(3).flatten().unwrap_or_default();
        let default = row.take::<Option<String>, _>(4).flatten();
        let comment = row.take::<Option<String>, _>(5).flatten().unwrap_or_default();
        let extra = row.take::<Option<String>, _>(6).flatten().unwrap_or_default();
        if table.is_empty() || name.is_empty() {
            continue;
        }
        let mut def = format!("`{name}` {ctype}");
        if nullable == "NO" {
            def.push_str(" NOT NULL");
        }
        match &default {
            Some(d) => def.push_str(&format!(" DEFAULT {}", quote_value(d))),
            None => def.push_str(" DEFAULT NULL"),
        }
        if !comment.is_empty() {
            def.push_str(&format!(" COMMENT {}", quote_value(&comment)));
        }
        if !extra.is_empty() {
            def.push_str(&format!(" {extra}"));
        }
        out.entry(table).or_default().insert(name, def);
    }
    Ok(out)
}

/// 定义串归一化（比对用）：去列名前缀、小写、压缩空白。
/// 生成定义与既有定义的 COMMENT/EXTRA 原文大小写一致，此归一化可容忍空格差异。
fn normalized_def(def: &str) -> String {
    let mut s = def.trim().to_lowercase();
    // 去列名前缀：`name` 类型... -> 从第二个反引号后开始
    if s.starts_with('`') {
        if let Some(pos) = s[1..].find('`') {
            s = s[pos + 2..].trim().to_string();
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
