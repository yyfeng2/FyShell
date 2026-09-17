//! MySQL 用户管理命令（对齐 Navicat 用户面板）：薄层转发到 services/mysql_user。
//!
//! 权限不足时由服务层返回带提示的错误（错误信息含 MySQL 原始错误，不回显密码）。

use crate::error::AppError;
use crate::models::mysql_user::{MySqlGrantItem, MySqlUserInfo};
use crate::services;

/// `mysql_user_list` (conn_id: String) -> Vec<MySqlUserInfo>
#[tauri::command]
#[specta::specta]
pub async fn mysql_user_list(conn_id: String) -> Result<Vec<MySqlUserInfo>, AppError> {
    services::mysql_user::user_list(&conn_id).await
}

/// `mysql_user_create` (conn_id: String, user: String, host: String, password: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_user_create(
    conn_id: String,
    user: String,
    host: String,
    password: String,
) -> Result<(), AppError> {
    services::mysql_user::user_create(&conn_id, &user, &host, &password).await
}

/// `mysql_user_drop` (conn_id: String, user: String, host: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn mysql_user_drop(
    conn_id: String,
    user: String,
    host: String,
) -> Result<(), AppError> {
    services::mysql_user::user_drop(&conn_id, &user, &host).await
}

/// `mysql_user_grants` (conn_id: String, user: String, host: String) -> Vec<MySqlGrantItem>
#[tauri::command]
#[specta::specta]
pub async fn mysql_user_grants(
    conn_id: String,
    user: String,
    host: String,
) -> Result<Vec<MySqlGrantItem>, AppError> {
    services::mysql_user::user_grants(&conn_id, &user, &host).await
}
