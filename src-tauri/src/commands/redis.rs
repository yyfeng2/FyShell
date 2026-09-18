//! Redis 基础命令：薄层转发到 services/redis。
//!
//! 命令名即前端 invoke 名，必须与前端保持一致。

use crate::error::AppError;
use crate::models::redis::{RedisConnection, RedisExecResult};
use crate::services;

/// `redis_connect` (config: RedisConnection) -> String connection_id
#[tauri::command]
#[specta::specta]
pub async fn redis_connect(config: RedisConnection) -> Result<String, AppError> {
    services::redis::connect(&config).await
}

/// `redis_disconnect` (conn_id: String) -> ()
#[tauri::command]
#[specta::specta]
pub async fn redis_disconnect(conn_id: String) -> Result<(), AppError> {
    services::redis::disconnect(&conn_id).await
}

/// `redis_test` (config: RedisConnection) -> String 服务器版本摘要
#[tauri::command]
#[specta::specta]
pub async fn redis_test(config: RedisConnection) -> Result<String, AppError> {
    services::redis::test(&config).await
}

/// `redis_info` (conn_id: String) -> Vec<(String, String)> 键值对
#[tauri::command]
#[specta::specta]
pub async fn redis_info(conn_id: String) -> Result<Vec<(String, String)>, AppError> {
    services::redis::info(&conn_id).await
}

/// `redis_select_db` (conn_id: String, db: u8) -> 切换库
#[tauri::command]
#[specta::specta]
pub async fn redis_select_db(conn_id: String, db: u8) -> Result<(), AppError> {
    services::redis::select_db(&conn_id, db).await
}

/// `redis_keys` (conn_id: String, pattern: String) -> Vec<String>
#[tauri::command]
#[specta::specta]
pub async fn redis_keys(conn_id: String, pattern: String) -> Result<Vec<String>, AppError> {
    services::redis::keys(&conn_id, &pattern).await
}

/// `redis_exec` (conn_id: String, args: Vec<String>) -> RedisExecResult
#[tauri::command]
#[specta::specta]
pub async fn redis_exec(conn_id: String, args: Vec<String>) -> Result<RedisExecResult, AppError> {
    services::redis::exec(&conn_id, &args).await
}
