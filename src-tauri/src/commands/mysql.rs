//! MySQL 基础命令（契约第 5.2 节）：薄层转发到 services/mysql。
//!
//! 危险 SQL 检测在 commands 层做：mysql_execute 命中对 DROP/TRUNCATE/ALTER 开头、
//! 或 DELETE/UPDATE 无 WHERE 的语句，若前端未传 `confirmed: true`，返回带提示的
//! 错误，前端弹出二次确认后重新调用（复用 P0 的 confirm 流程）。

use crate::error::AppError;
use crate::models::mysql::{MySqlConnection, MySqlQueryResult, MySqlTableInfo};
use crate::services;

/// `mysql_connect` (config: MySqlConnection) -> String connection_id
#[tauri::command]
#[specta::specta]
pub async fn mysql_connect(config: MySqlConnection) -> Result<String, AppError> {
    services::mysql::connect(&config).await
}

/// `mysql_disconnect` (conn_id: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_disconnect(conn_id: String) -> Result<(), AppError> {
    services::mysql::disconnect(&conn_id).await
}

/// `mysql_list_tables` (conn_id: String) -> Vec<MySqlTableInfo>
#[tauri::command]
#[specta::specta]
pub async fn mysql_list_tables(conn_id: String) -> Result<Vec<MySqlTableInfo>, AppError> {
    services::mysql::list_tables(&conn_id).await
}

/// `mysql_query` (conn_id: String, sql: String, page: u32, page_size: u32) -> MySqlQueryResult
#[tauri::command]
#[specta::specta]
pub async fn mysql_query(
    conn_id: String,
    sql: String,
    page: u32,
    page_size: u32,
) -> Result<MySqlQueryResult, AppError> {
    services::mysql::query(&conn_id, &sql, page, page_size).await
}

/// `mysql_execute` (conn_id: String, sql: String [, confirmed: bool]) -> u64 受影响行数
///
/// `confirmed` 为契约扩展的可选参数：前端首次调用不传，命中危险 SQL 时收到
/// 错误提示并二次确认，确认后带 `confirmed: true` 重新调用。
#[tauri::command]
#[specta::specta]
pub async fn mysql_execute(
    conn_id: String,
    sql: String,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    if services::mysql::is_dangerous_sql(&sql) && confirmed != Some(true) {
        return Err(AppError::general(
            "危险 SQL（DROP/TRUNCATE/ALTER 或无 WHERE 的 DELETE/UPDATE），请确认后以 confirmed=true 重新执行",
        ));
    }
    services::mysql::execute(&conn_id, &sql).await
}

/// `mysql_begin` (conn_id: String) -> 开启事务
#[tauri::command]
#[specta::specta]
pub async fn mysql_begin(conn_id: String) -> Result<(), AppError> {
    services::mysql::begin(&conn_id).await
}

/// `mysql_commit` (conn_id: String) -> 提交事务
#[tauri::command]
#[specta::specta]
pub async fn mysql_commit(conn_id: String) -> Result<(), AppError> {
    services::mysql::commit(&conn_id).await
}

/// `mysql_rollback` (conn_id: String) -> 回滚事务
#[tauri::command]
#[specta::specta]
pub async fn mysql_rollback(conn_id: String) -> Result<(), AppError> {
    services::mysql::rollback(&conn_id).await
}
