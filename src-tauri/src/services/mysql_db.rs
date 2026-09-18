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
use crate::models::mysql_db::{MySqlDatabaseList, MySqlTableDdl};
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
pub async fn create_database(conn_id: &str, name: &str) -> Result<(), AppError> {
    if !is_valid_identifier(name) {
        return Err(AppError::general(format!("数据库名非法: {name}")));
    }
    let sql = format!("CREATE DATABASE {}", quote_db(name));
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
