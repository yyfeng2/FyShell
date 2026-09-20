//! 配色方案文件导入导出：薄层，读写用户经系统对话框选择的文件路径。
//!
//! 方案内容为 JSON（前端序列化/校验），Rust 侧只负责文件 IO。

use crate::error::AppError;

/// 读取配色方案文件（JSON 文本）
#[tauri::command]
#[specta::specta]
pub fn scheme_read_file(path: String) -> Result<String, AppError> {
    std::fs::read_to_string(&path).map_err(AppError::Io)
}

/// 写入配色方案文件（JSON 文本，已存在时覆盖）
#[tauri::command]
#[specta::specta]
pub fn scheme_write_file(path: String, content: String) -> Result<(), AppError> {
    std::fs::write(&path, content).map_err(AppError::Io)
}
