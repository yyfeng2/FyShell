//! 主密码命令：薄层，包装 services/config_store 的主密码接口（P0 预留接口接线）。
//!
//! 哈希为简单实现（FNV-1a 迭代拉伸 + 随机盐），正式版建议替换为 argon2；
//! 凭据仅存 Rust 侧 SQLite meta 表，不进 WebView（架构红线 §1.3）。

use tauri::State;

use crate::error::AppError;
use crate::state::AppState;

/// 是否已设置主密码（前端据此决定首次设置 / 修改模式）
#[tauri::command]
#[specta::specta]
pub fn master_password_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    state.config_store.has_master_password()
}

/// 设置主密码。
///
/// - 首次设置（当前无主密码）：注册保险库 DEK 信封，此后已存凭据转加密存储
/// - 修改（当前已有主密码）：须提供 `old_password` —— 先用它迁移 DEK 信封
///   （旧密码错误时拒绝更新，防止已存凭据变成不可解），成功后再覆盖哈希
#[tauri::command]
#[specta::specta]
pub fn master_password_set(
    state: State<'_, AppState>,
    password: String,
    old_password: Option<String>,
) -> Result<(), AppError> {
    if state.config_store.has_master_password()? {
        let old = old_password
            .ok_or_else(|| AppError::general("修改主密码需提供旧密码"))?;
        crate::services::vault::rekey_migrate(&old, &password)?;
    } else {
        crate::services::vault::rekey(&password)?;
    }
    state.config_store.set_master_password(&password)?;
    // 设密/改密成功后迁移明文凭据存量；失败仅记录不阻塞设密
    if let Err(e) = crate::services::vault::migrate_plaintext() {
        eprintln!("vault 明文凭据迁移失败：{e}");
    }
    Ok(())
}

/// 验证主密码；尚未设置时返回 Ok(false)
#[tauri::command]
#[specta::specta]
pub fn master_password_verify(state: State<'_, AppState>, password: String) -> Result<bool, AppError> {
    state.config_store.verify_master_password(&password)
}
