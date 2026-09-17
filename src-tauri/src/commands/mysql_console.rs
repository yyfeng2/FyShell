//! SQL 控制台命令（P2 阶段：查询历史 + 执行计划 + 多结果集）：薄层转发到
//! services/mysql_console。
//!
//! 历史记录由 query_multi 执行成功后自动写入，命令层仅提供列表 / 搜索 / 清空。

use crate::error::AppError;
use crate::models::mysql::MySqlQueryResult;
use crate::models::mysql_console::{MySqlExplainResult, MySqlQueryHistoryItem};
use crate::services;

/// `mysql_history_list` (limit: u32) -> Vec<MySqlQueryHistoryItem>
#[tauri::command]
pub async fn mysql_history_list(limit: u32) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    services::mysql_console::history_list(limit)
}

/// `mysql_history_search` (keyword: String, limit: u32) -> Vec<MySqlQueryHistoryItem>
#[tauri::command]
pub async fn mysql_history_search(
    keyword: String,
    limit: u32,
) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    services::mysql_console::history_search(&keyword, limit)
}

/// `mysql_history_clear` () -> 清空全部查询历史
#[tauri::command]
pub async fn mysql_history_clear() -> Result<(), AppError> {
    services::mysql_console::history_clear()
}

/// `mysql_explain` (conn_id: String, sql: String [, analyze: bool]) -> MySqlExplainResult
///
/// `analyze` 为可选参数（默认 false）：true 时使用 EXPLAIN ANALYZE（真实执行
/// 语句，后端仅允许 SELECT 开头的语句，写语句返回错误提示）。
#[tauri::command]
pub async fn mysql_explain(
    conn_id: String,
    sql: String,
    analyze: Option<bool>,
) -> Result<MySqlExplainResult, AppError> {
    services::mysql_console::explain(&conn_id, &sql, analyze.unwrap_or(false)).await
}

/// `mysql_query_multi` (conn_id: String, sql: String) -> Vec<MySqlQueryResult>
#[tauri::command]
pub async fn mysql_query_multi(conn_id: String, sql: String) -> Result<Vec<MySqlQueryResult>, AppError> {
    services::mysql_console::query_multi(&conn_id, &sql).await
}
