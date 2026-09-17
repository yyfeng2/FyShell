//! MySQL 数据库级管理命令（契约第 5.2 节扩展）：薄层转发到 services/mysql_db。
//!
//! DROP DATABASE 的强确认流程与 mysql_execute 一致：命令对未传 `confirmed: true`
//! 的调用返回带提示的错误，前端弹出二次确认后带 confirmed 重新调用。
//! 表级 TRUNCATE 不在此处：复用 mysql_execute 的危险 SQL 确认流程。

use crate::error::AppError;
use crate::models::mysql_db::{MySqlDatabaseList, MySqlTableDdl};
use crate::services;

/// `mysql_db_list` (conn_id: String) -> MySqlDatabaseList（host + current_db + databases）
#[tauri::command]
#[specta::specta]
pub async fn mysql_db_list(conn_id: String) -> Result<MySqlDatabaseList, AppError> {
    services::mysql_db::list_databases(&conn_id).await
}

/// `mysql_db_create` (conn_id: String, name: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_db_create(conn_id: String, name: String) -> Result<(), AppError> {
    services::mysql_db::create_database(&conn_id, &name).await
}

/// `mysql_db_drop` (conn_id: String, name: String [, confirmed: bool]) -> ()
///
/// 强确认：数据库级删除不可恢复，首次调用不传 confirmed 时返回带提示的错误，
/// 前端弹出二次确认（danger）后带 `confirmed: true` 重新调用。
#[tauri::command]
#[specta::specta]
pub async fn mysql_db_drop(
    conn_id: String,
    name: String,
    confirmed: Option<bool>,
) -> Result<(), AppError> {
    if confirmed != Some(true) {
        return Err(AppError::general(
            "DROP DATABASE 不可恢复，请确认后以 confirmed=true 重新执行",
        ));
    }
    services::mysql_db::drop_database(&conn_id, &name).await
}

/// `mysql_db_switch` (conn_id: String, name: String) -> String 新 connection_id
///
/// 切库实现为重建连接池（mysql_async 池归还连接时会 COM_RESET_CONNECTION
/// 重置当前库，USE 不可靠），前端以新 conn_id 替换后刷新对象树与数据网格。
#[tauri::command]
#[specta::specta]
pub async fn mysql_db_switch(conn_id: String, name: String) -> Result<String, AppError> {
    services::mysql_db::switch_database(&conn_id, &name).await
}

/// `mysql_table_show_create` (conn_id: String, table: String) -> MySqlTableDdl
#[tauri::command]
#[specta::specta]
pub async fn mysql_table_show_create(
    conn_id: String,
    table: String,
) -> Result<MySqlTableDdl, AppError> {
    services::mysql_db::show_create_table(&conn_id, &table).await
}

/// `mysql_table_optimize` (conn_id: String, table: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_table_optimize(conn_id: String, table: String) -> Result<(), AppError> {
    services::mysql_db::optimize_table(&conn_id, &table).await
}

/// `mysql_table_rename` (conn_id: String, old_name: String, new_name: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_table_rename(
    conn_id: String,
    old_name: String,
    new_name: String,
) -> Result<(), AppError> {
    services::mysql_db::rename_table(&conn_id, &old_name, &new_name).await
}
