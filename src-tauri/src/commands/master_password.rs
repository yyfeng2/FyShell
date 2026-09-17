//! 主密码命令：薄层，包装 services/config_store 的主密码接口（P0 预留接口接线）。
//!
//! 哈希为简单实现（FNV-1a 迭代拉伸 + 随机盐），正式版建议替换为 argon2；
//! 凭据仅存 Rust 侧 SQLite meta 表，不进 WebView（架构红线 §1.3）。

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;

/// 是否已设置主密码（前端据此决定首次设置 / 修改模式）
#[tauri::command]
pub fn master_password_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    state.config_store.has_master_password()
}

/// 设置主密码（Rust 侧幂等覆盖；修改前应由前端先经 verify 校验旧密码）
#[tauri::command]
pub fn master_password_set(state: State<'_, AppState>, password: String) -> Result<(), AppError> {
    state.config_store.set_master_password(&password)
}

/// 验证主密码；尚未设置时返回 Ok(false)
#[tauri::command]
pub fn master_password_verify(state: State<'_, AppState>, password: String) -> Result<bool, AppError> {
    state.config_store.verify_master_password(&password)
}
