//! MySQL 导入导出命令（契约 P2 阶段）：薄层转发到 services/mysql_io。
//!
//! 危险保护与 mysql_execute 同策略：replace=true（REPLACE INTO 覆盖已有主键）时，
//! 若前端未传 `confirmed: true`，返回带提示的错误，前端弹出二次确认后重新调用。

use crate::error::AppError;
use crate::models::mysql_io::{MySqlExportOptions, MySqlImportOptions, MySqlIoResult};
use crate::services;

/// `mysql_export` (conn_id: String, options: MySqlExportOptions) -> MySqlIoResult
#[tauri::command]
pub async fn mysql_export(
    conn_id: String,
    options: MySqlExportOptions,
) -> Result<MySqlIoResult, AppError> {
    services::mysql_io::export(&conn_id, &options).await
}

/// `mysql_import` (conn_id: String, options: MySqlImportOptions [, confirmed: bool]) -> MySqlIoResult
///
/// `confirmed` 为契约扩展的可选参数：options.replace 为 true（REPLACE INTO 覆盖
/// 已有主键）时，前端首次调用不传，收到错误提示并二次确认，确认后带
/// `confirmed: true` 重新调用。
#[tauri::command]
pub async fn mysql_import(
    conn_id: String,
    options: MySqlImportOptions,
    confirmed: Option<bool>,
) -> Result<MySqlIoResult, AppError> {
    if options.replace && confirmed != Some(true) {
        return Err(AppError::general(
            "导入将使用 REPLACE INTO 覆盖已有主键，请确认后以 confirmed=true 重新执行",
        ));
    }
    services::mysql_io::import(&conn_id, &options).await
}
