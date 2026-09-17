//! MySQL 数据编辑命令（P2 阶段）：薄层转发到 services/mysql_edit。
//!
//! 「预览 -> 确认 -> 执行」管道：前端先调 mysql_edit_preview 获取将执行的
//! SQL 与受影响行数估算；确认后再调 mysql_update_row / mysql_update_rows /
//! mysql_delete_row 执行。写命令的 `confirmed` 参数与 mysql_execute 一致：
//! 命中危险操作且未确认时返回带提示错误，前端二次确认后重新调用。

use crate::error::AppError;
use crate::models::mysql_edit::{MySqlEditPreview, MySqlRowUpdate, MySqlRowUpdateBatch};
use crate::services;

/// `mysql_edit_preview` (conn_id: String, update: MySqlRowUpdate) -> MySqlEditPreview
#[tauri::command]
pub async fn mysql_edit_preview(
    conn_id: String,
    update: MySqlRowUpdate,
) -> Result<MySqlEditPreview, AppError> {
    services::mysql_edit::preview_update(&conn_id, &update).await
}

/// `mysql_update_row` (conn_id: String, update: MySqlRowUpdate [, confirmed: bool]) -> u64 受影响行数
///
/// 执行前先走一次预览（同时完成标识符校验与 danger 判定），命中危险操作且
/// 未确认时返回带提示错误；确认后带 `confirmed: true` 重新调用（复用 P0 的
/// confirm 流程）。
#[tauri::command]
pub async fn mysql_update_row(
    conn_id: String,
    update: MySqlRowUpdate,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    let preview = services::mysql_edit::preview_update(&conn_id, &update).await?;
    if preview.danger && confirmed != Some(true) {
        return Err(AppError::general(format!(
            "危险写操作（{}），请确认后以 confirmed=true 重新执行",
            preview.danger_reason.unwrap_or_default()
        )));
    }
    services::mysql_edit::update_row(&conn_id, &update).await
}

/// `mysql_update_rows` (conn_id: String, batch: MySqlRowUpdateBatch [, confirmed: bool]) -> u64 总受影响行数
///
/// 批量条目全部按主键定位（生成的 UPDATE 恒含 WHERE，无全表风险），
/// `confirmed` 参数为管道一致性保留；批量默认以隐式事务包裹，失败整体回滚。
#[tauri::command]
pub async fn mysql_update_rows(
    conn_id: String,
    batch: MySqlRowUpdateBatch,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    let _ = confirmed;
    services::mysql_edit::update_rows(&conn_id, &batch).await
}

/// `mysql_delete_row` (conn_id: String, table: String, pk_column: String, pk_value: Option<String> [, confirmed: bool]) -> u64 受影响行数
///
/// 按主键定位删除，SQL 恒含 WHERE（非无 WHERE 的危险 DELETE），
/// `confirmed` 参数为管道一致性保留。
#[tauri::command]
pub async fn mysql_delete_row(
    conn_id: String,
    table: String,
    pk_column: String,
    pk_value: Option<String>,
    confirmed: Option<bool>,
) -> Result<u64, AppError> {
    let _ = confirmed;
    services::mysql_edit::delete_row(&conn_id, &table, &pk_column, pk_value).await
}
