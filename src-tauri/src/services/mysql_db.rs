//! MySQL 数据库级管理服务（对齐 Navicat 的数据库切换/新建/删除与表快捷操作）。
//!
//! - `list_databases`：SHOW DATABASES 列出该连接可见的全部库（含 host 与当前库）
//! - `create_database`：CREATE DATABASE
//! - `drop_database`：DROP DATABASE（强确认，confirmed 兜底在 commands 层）
//! - `switch_database`：切换默认库——重建该连接的连接池并返回新 connection_id
//! - `show_create_table`：SHOW CREATE TABLE 取完整建表语句
//! - `optimize_table`：OPTIMIZE TABLE 回收表空间
//! - `rename_table`：RENAME TABLE 旧表 TO 新表
//!
//! 连接池复用 services::mysql 暴露的 `pool_of` / `take_tx_conn` /
//! `give_tx_conn`，不触碰 AppState / lib.rs。
//!
//! ⚠️ 切库为什么重建连接池：mysql_async 池连接归还时会执行 COM_RESET_CONNECTION
//! （把当前库重置回连接时的默认库），因此执行 `USE db` 会在连接归还后被重置，
//! 池中各连接的当前库上下文无法一致。重建连接池（新 schema + 新 connection_id）
//! 是唯一可靠的方案，代价是切换时会重建一次连接（含可达性探针）。

use mysql_async::prelude::*;
use mysql_async::Conn;

use crate::error::AppError;
use crate::models::mysql_db::{MySqlDatabaseList, MySqlDbFindHit, MySqlTableDdl};
use crate::services::mysql::config_of;

/// mysql_async::Error -> AppError 归入 General 变体（与 services/mysql 同款归一化）
fn mysql_err(e: mysql_async::Error) -> AppError {
    AppError::general(format!("MySQL 错误: {e}"))
}

/// 标识符基础校验：非空且不含反引号（反引号为 MySQL 标识符转义符，直接拒绝）
fn is_valid_identifier(name: &str) -> bool {
    !name.is_empty() && !name.contains('`')
}

/// 数据库名加反引号包裹（调用前已校验）
fn quote_db(name: &str) -> String {
    format!("`{name}`")
}

/// 表名加反引号包裹（非空、无反引号校验，防注入）
fn quote_table(name: &str) -> Result<String, AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("表名非法: {name}")));
    }
    Ok(format!("`{name}`"))
}

/// db_list：SHOW DATABASES 列出该连接可见的全部数据库
///
/// host 取自连接池 Opts（前端「复制 Host」复用）；current_db 为
/// SELECT DATABASE()（事务连接与池连接同源，当前库上下文一致）。
pub async fn list_databases(conn_id: &str) -> Result<MySqlDatabaseList, AppError> {
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    // outcome 捕获模式：即使中途出错也归还事务连接，避免用户显式事务失去句柄
    let outcome = do_list_databases(&mut conn).await;
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 列表主体（拆出以便错误早退时仍归还事务连接）
async fn do_list_databases(conn: &mut Conn) -> Result<MySqlDatabaseList, AppError> {
    // host 从连接 Opts 取（Opts 在建池时由 ip_or_hostname 构造，前端「复制 Host」复用）
    let host = conn.opts().ip_or_hostname().to_string();

    let mut result = conn.query_iter("SHOW DATABASES").await.map_err(mysql_err)?;
    let mut databases = Vec::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let name = row
            .take::<Option<String>, _>(0)
            .flatten()
            .unwrap_or_default();
        if !name.is_empty() {
            databases.push(name);
        }
    }
    result.drop_result().await.map_err(mysql_err)?;

    // 当前默认库（None = 连接时未指定 schema 且未 USE）
    let current_db: Option<String> = conn
        .query_first::<Option<String>, _>("SELECT DATABASE()")
        .await
        .map_err(mysql_err)?
        .flatten();

    databases.sort();
    Ok(MySqlDatabaseList {
        host,
        current_db,
        databases,
    })
}

/// db_create：`CREATE DATABASE \`name\``（数据库名反引号包裹防注入）
pub async fn create_database(
    conn_id: &str,
    name: &str,
    charset: Option<&str>,
    collation: Option<&str>,
) -> Result<(), AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("数据库名非法: {name}")));
    }
    let mut sql = format!("CREATE DATABASE {}", quote_db(name));
    if let Some(cs) = charset {
        if !is_valid_charset_name(cs) {
            return Err(AppError::general(format!("字符集非法: {cs}")));
        }
        sql.push_str(&format!(" DEFAULT CHARACTER SET = {}", quote_db(cs)));
        if let Some(coll) = collation {
            if !coll.is_empty() {
                if !is_valid_charset_name(coll) {
                    return Err(AppError::general(format!("排序规则非法: {coll}")));
                }
                sql.push_str(&format!(" COLLATE = {}", quote_db(coll)));
            }
        }
    }
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// db_drop：`DROP DATABASE \`name\``（命令层强确认后调用；数据库级不可恢复）
pub async fn drop_database(conn_id: &str, name: &str) -> Result<(), AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("数据库名非法: {name}")));
    }
    let sql = format!("DROP DATABASE {}", quote_db(name));
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// db_switch：切换默认数据库——重建该连接的连接池，返回新 connection_id
///
/// 如模块注释所述：池连接归还时会 COM_RESET_CONNECTION 重置当前库，`USE` 不可靠；
/// 此处取原始连接配置（services/mysql 的 config_of）把 schema 换成新库后断开
/// 旧连接、建立新池（新 conn_id，含可达性验证），前端以新 conn_id 继续。
pub async fn switch_database(conn_id: &str, new_db: &str) -> Result<String, AppError> {
    if !is_valid_identifier(new_db) {
        return Err(AppError::general(format!("数据库名非法: {new_db}")));
    }
    let mut cfg = config_of(conn_id)?;
    cfg.schema = Some(new_db.to_string());
    // 断开旧连接（含事务连接清理）后再建新池；新池建立失败时旧连接已断，
    // 前端以错误提示呈现，用户重新连接即可
    crate::services::mysql::disconnect(conn_id).await?;
    crate::services::mysql::connect(&cfg).await
}

/// show_create_table：`SHOW CREATE TABLE \`table\``，返回完整 DDL
///
/// 表名走反引号拼接（先校验防注入）；DDL 按列名 "Create Table" 定位
/// （对齐 services/mysql_objects 的按列名定位方案，避免固定列错位）。
pub async fn show_create_table(conn_id: &str, table: &str) -> Result<MySqlTableDdl, AppError> {
    let quoted = quote_table(table)?;
    let stmt = format!("SHOW CREATE TABLE {quoted}");

    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = do_show_create(&mut conn, table, &stmt).await;
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// SHOW CREATE TABLE 主体（拆出以便错误后仍归还事务连接）
async fn do_show_create(
    conn: &mut Conn,
    table: &str,
    stmt: &str,
) -> Result<MySqlTableDdl, AppError> {
    // SHOW CREATE 无占位符，直接文本协议查询
    let mut result = conn.query_iter(stmt).await.map_err(mysql_err)?;

    // DDL 列按列名定位（"Create Table"），避免固定第二列的错位
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

    match sql_text {
        Some(sql) => Ok(MySqlTableDdl {
            table: table.to_string(),
            sql,
        }),
        None => Err(AppError::general(format!(
            "无法解析 SHOW CREATE TABLE 结果（表 {table} 不存在或结果列为空）"
        ))),
    }
}

/// table_optimize：`OPTIMIZE TABLE \`table\``（回收碎片空间）
///
/// OPTIMIZE 执行时会附带结果集（Table/Op/Msg_type/Msg_text），统一消费掉。
pub async fn optimize_table(conn_id: &str, table: &str) -> Result<(), AppError> {
    let quoted = quote_table(table)?;
    let sql = format!("OPTIMIZE TABLE {quoted}");
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// table_rename：`RENAME TABLE \`old\` TO \`new\``
///
/// 新旧表名均校验（非空、拒绝反引号）；执行前不预查新表名是否存在
/// （MySQL 自身会拒绝重名 RENAME）。
pub async fn rename_table(conn_id: &str, old_name: &str, new_name: &str) -> Result<(), AppError> {
    let quoted_old = quote_table(old_name)?;
    let quoted_new = quote_table(new_name)?;
    let sql = format!("RENAME TABLE {quoted_old} TO {quoted_new}");
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// db_edit：ALTER DATABASE 指定库默认字符集/排序规则
///
/// 字符集/排序规则名限字母数字与下划线（MySQL 内置命名如 utf8mb4_general_ci 均满足，
/// 防注入）；collation 为 None 时只设字符集。右键的库可与连接默认库不同，
/// ALTER 直接限定库名，不依赖连接当前库上下文。
pub async fn edit_database(
    conn_id: &str,
    name: &str,
    charset: &str,
    collation: Option<&str>,
) -> Result<(), AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("数据库名非法: {name}")));
    }
    if !is_valid_charset_name(charset) {
        return Err(AppError::general(format!("字符集名非法: {charset}")));
    }
    if let Some(coll) = collation {
        if !is_valid_charset_name(coll) {
            return Err(AppError::general(format!("排序规则名非法: {coll}")));
        }
    }
    let mut sql = format!(
        "ALTER DATABASE {} DEFAULT CHARACTER SET = {charset}",
        quote_db(name)
    );
    if let Some(coll) = collation {
        sql.push_str(&format!(" COLLATE = {coll}"));
    }
    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = conn.query_drop(&sql).await.map_err(mysql_err);
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// db_find：全库表数据按关键字 LIKE 搜索（右键菜单「在数据库中查找」）
///
/// 右键的库可与连接默认库不同，全部 SQL 以 `db`.`table` 限定、
/// TABLE_SCHEMA = 'name' 过滤，不依赖连接当前库上下文。
/// 仅搜字符串列（char/varchar/text 系/enum/set）；每表命中行数受 max_per_table
/// 限制（默认 20）；只返回有命中的表，columns 为该表全部列名（SELECT * 顺序）。
pub async fn find_in_database(
    conn_id: &str,
    name: &str,
    keyword: &str,
    max_per_table: Option<u32>,
) -> Result<Vec<MySqlDbFindHit>, AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("数据库名非法: {name}")));
    }
    if keyword.trim().is_empty() {
        return Err(AppError::general("搜索关键字不能为空"));
    }
    let limit = max_per_table.unwrap_or(20).max(1);

    let (mut conn, from_tx) = take_tx_or_pool(conn_id).await?;
    let outcome = do_find_in_database(&mut conn, name, keyword, limit).await;
    if from_tx {
        crate::services::mysql::give_tx_conn(conn_id, conn);
    }
    outcome
}

/// 全库搜索主体（拆出以便错误早退时仍归还事务连接）
async fn do_find_in_database(
    conn: &mut Conn,
    name: &str,
    keyword: &str,
    limit: u32,
) -> Result<Vec<MySqlDbFindHit>, AppError> {
    // LIKE 模式转义：反斜杠/百分号/下划线三字符（包 %kw% 后 %/_ 才不当作通配符）
    let mut like = String::from("%");
    for c in keyword.chars() {
        if c == '\\' || c == '%' || c == '_' {
            like.push('\\');
        }
        like.push(c);
    }
    like.push('%');
    // SQL 字符串字面量单引号翻倍转义
    let like_lit = like.replace('\'', "''");
    let schema_lit = name.replace('\'', "''");

    // 1. 全部列元数据（含是否可搜索的字符串列标记），按表/列序分组
    let meta_sql = format!(
        "SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = '{schema_lit}' ORDER BY TABLE_NAME, ORDINAL_POSITION"
    );
    let mut result = conn.query_iter(meta_sql).await.map_err(mysql_err)?;
    // 表名 -> 全列名列表 / 可搜索列名列表；order 保持表出现顺序
    let mut order: Vec<String> = Vec::new();
    let mut all_cols: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut search_cols: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    while let Some(mut row) = result.next().await.map_err(mysql_err)? {
        let table = row.take::<Option<String>, _>(0).flatten().unwrap_or_default();
        let col = row.take::<Option<String>, _>(1).flatten().unwrap_or_default();
        let dtype = row
            .take::<Option<String>, _>(2)
            .flatten()
            .unwrap_or_default();
        if table.is_empty() || col.is_empty() {
            continue;
        }
        if !all_cols.contains_key(&table) {
            order.push(table.clone());
        }
        all_cols.entry(table.clone()).or_default().push(col.clone());
        if is_searchable_type(&dtype) {
            search_cols.entry(table).or_default().push(col);
        }
    }
    result.drop_result().await.map_err(mysql_err)?;

    // 2. 每表 LIKE 搜索（`db`.`table` 限定，不依赖连接当前库）
    let mut hits = Vec::new();
    for table in order {
        let cols = match all_cols.get(&table) {
            Some(cols) => cols.clone(),
            None => continue,
        };
        let searchable = match search_cols.get(&table) {
            Some(cols) if !cols.is_empty() => cols,
            _ => continue,
        };
        let where_clause: Vec<String> = searchable
            .iter()
            .map(|c| format!("`{c}` LIKE '{like_lit}'"))
            .collect();
        let sql = format!(
            "SELECT * FROM {}.{} WHERE {} LIMIT {limit}",
            quote_db(name),
            quote_table(&table)?,
            where_clause.join(" OR ")
        );
        let rows = collect_rows(conn, &sql).await?;
        if !rows.is_empty() {
            hits.push(MySqlDbFindHit { table, columns: cols, rows });
        }
    }
    Ok(hits)
}

/// 字符串系列类型判定（这些列参与 LIKE 搜索）
fn is_searchable_type(dtype: &str) -> bool {
    matches!(
        dtype,
        "char" | "varchar" | "text" | "tinytext" | "mediumtext" | "longtext" | "enum" | "set"
    )
}

/// 字符集/排序规则名校验：仅字母数字与下划线（MySQL 内置命名均满足，防注入）
fn is_valid_charset_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 执行 SELECT 并收集全部行（列按位置，None = NULL）
async fn collect_rows(conn: &mut Conn, sql: &str) -> Result<Vec<Vec<Option<String>>>, AppError> {
    let mut result = conn.query_iter(sql).await.map_err(mysql_err)?;
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
    Ok(rows)
}

/// 取连接：优先复用事务内独占连接，否则从池中取
/// 返回 `bool` 表示是否来自事务连接；事务连接用完需 `give_tx_conn` 放回。
async fn take_tx_or_pool(conn_id: &str) -> Result<(Conn, bool), AppError> {
    if let Some(conn) = crate::services::mysql::take_tx_conn(conn_id) {
        return Ok((conn, true));
    }
    let pool = crate::services::mysql::pool_of(conn_id)?;
    let conn = pool.get_conn().await.map_err(mysql_err)?;
    Ok((conn, false))
}
