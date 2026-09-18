//! SQL 控制台命令（P2 阶段：查询历史 + 执行计划 + 已保存查询）：
//! 薄层转发到 services/mysql_console。
//!
//! 历史记录由 services/mysql.rs 的 query/execute 执行成功后自动写入（页首只记一次）；
//! 命令层仅提供列表 / 搜索 / 清空；已保存查询为用户命名的持久化查询，
//! 提供列表 / 保存（同名覆盖）/ 重命名 / 删除。

use crate::error::AppError;
use crate::models::mysql_console::{MySqlExplainResult, MySqlQueryHistoryItem, MySqlSavedQueryItem};
use crate::services;

/// `mysql_history_list` (limit: u32) -> Vec<MySqlQueryHistoryItem>
#[tauri::command]
#[specta::specta]
pub async fn mysql_history_list(limit: u32) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    services::mysql_console::history_list(limit)
}

/// `mysql_history_search` (keyword: String, limit: u32) -> Vec<MySqlQueryHistoryItem>
#[tauri::command]
#[specta::specta]
pub async fn mysql_history_search(
    keyword: String,
    limit: u32,
) -> Result<Vec<MySqlQueryHistoryItem>, AppError> {
    services::mysql_console::history_search(&keyword, limit)
}

/// `mysql_history_clear` () -> 清空全部查询历史
#[tauri::command]
#[specta::specta]
pub async fn mysql_history_clear() -> Result<(), AppError> {
    services::mysql_console::history_clear()
}

/// `mysql_explain` (conn_id: String, sql: String [, analyze: bool]) -> MySqlExplainResult
///
/// `analyze` 为可选参数（默认 false）：true 时使用 EXPLAIN ANALYZE（真实执行
/// 语句，后端仅允许 SELECT 开头的语句，写语句返回错误提示）。
#[tauri::command]
#[specta::specta]
pub async fn mysql_explain(
    conn_id: String,
    sql: String,
    analyze: Option<bool>,
) -> Result<MySqlExplainResult, AppError> {
    services::mysql_console::explain(&conn_id, &sql, analyze.unwrap_or(false)).await
}

// ---------------------------------------------------------------------------
// 已保存查询（命名保存，区别于自动记录的查询历史）
// ---------------------------------------------------------------------------

/// `mysql_saved_query_list` () -> Vec<MySqlSavedQueryItem>
#[tauri::command]
#[specta::specta]
pub async fn mysql_saved_query_list() -> Result<Vec<MySqlSavedQueryItem>, AppError> {
    services::mysql_console::saved_query_list()
}

/// `mysql_saved_query_save` (name: String, conn_id: Option<String>, sql: String [, overwrite: bool]) -> bool（true = 覆盖了同名）
///
/// 新增或覆盖：同名且未带 `overwrite=true` 时返回带提示错误，前端覆盖确认后
/// 重新调用（与 mysql_update_row 的 confirmed 流程一致）。
/// `conn_id` 可空：None 表示不绑定连接（全局可见）。
#[tauri::command]
#[specta::specta]
pub async fn mysql_saved_query_save(
    name: String,
    conn_id: Option<String>,
    sql: String,
    overwrite: Option<bool>,
) -> Result<bool, AppError> {
    services::mysql_console::saved_query_save(
        &name,
        conn_id.as_deref(),
        &sql,
        overwrite.unwrap_or(false),
    )
}

/// `mysql_saved_query_rename` (id: i64, name: String) -> 重命名（按 id 定位）
///
/// 新名称与其它条目冲突（UNIQUE）时返回带提示错误。
#[tauri::command]
#[specta::specta]
pub async fn mysql_saved_query_rename(id: i64, name: String) -> Result<(), AppError> {
    services::mysql_console::saved_query_rename(id, &name)
}

/// `mysql_saved_query_delete` (id: i64) -> 删除（按 id 定位）
#[tauri::command]
#[specta::specta]
pub async fn mysql_saved_query_delete(id: i64) -> Result<(), AppError> {
    services::mysql_console::saved_query_delete(id)
}
