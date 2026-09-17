//! 会话日志命令（契约第 5.2 节）：薄层，参数校验 + 调用 services/session_log。
//!
//! 落盘路径：`app_data_dir/logs/<session_id>/<YYYY-MM-DD>.log`。

use crate::error::AppError;
use crate::services::session_log;

/// 输出落盘开关（某会话）
#[tauri::command]
#[specta::specta]
pub fn session_log_toggle(session_id: String, enabled: bool) -> Result<(), AppError> {
    session_log::toggle(&session_id, enabled)
}

/// 某会话的日志日期列表（YYYY-MM-DD，升序）
#[tauri::command]
#[specta::specta]
pub fn session_log_list(session_id: String) -> Result<Vec<String>, AppError> {
    session_log::list_dates(&session_id)
}

/// 读取某会话某天的日志内容（date 格式必须为 YYYY-MM-DD）
#[tauri::command]
#[specta::specta]
pub fn session_log_read(session_id: String, date: String) -> Result<String, AppError> {
    session_log::read_date(&session_id, &date)
}
