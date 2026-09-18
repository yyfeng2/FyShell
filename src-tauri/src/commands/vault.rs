//! 凭据保险库命令：薄层，转发到 services/vault。
//!
//! 已存连接的密文明文加解密完全在后端完成（DEK 从不过桥到前端）；
//! 前端仅提供主密码串与收发密文/明文，明文在内存中短暂存在。

use crate::error::AppError;
use crate::models::vault::VaultStatus;
use crate::services;

/// `vault_status` () -> VaultStatus（has_master_password / unlocked）
#[tauri::command]
#[specta::specta]
pub fn vault_status() -> Result<VaultStatus, AppError> {
    Ok(VaultStatus {
        has_master_password: services::vault::has_master_password(),
        unlocked: services::vault::is_unlocked(),
    })
}

/// `vault_unlock` (password: String) -> 以主密码解锁保险库（解包 DEK 入内存）
///
/// 主密码错误时返回带提示错误（AEAD 认证失败），不泄露哈希信息。
#[tauri::command]
#[specta::specta]
pub fn vault_unlock(password: String) -> Result<(), AppError> {
    services::vault::unlock(&password)
}

/// `vault_lock` () -> 锁定保险库（丢弃内存 DEK），后续凭据读写需重新解锁
#[tauri::command]
#[specta::specta]
pub fn vault_lock() -> Result<(), AppError> {
    services::vault::lock();
    Ok(())
}

/// `vault_encrypt` (plain: String) -> String（`v1$nonce$cipher`，hex）
///
/// 供保存路径在后端加密明文凭据后落 settings；未解锁时返回带提示错误。
#[tauri::command]
#[specta::specta]
pub fn vault_encrypt(plain: String) -> Result<String, AppError> {
    services::vault::encrypt(&plain)
}

/// `vault_decrypt` (cipher: String) -> String（读取路径，密文还原明文）
#[tauri::command]
#[specta::specta]
pub fn vault_decrypt(cipher: String) -> Result<String, AppError> {
    services::vault::decrypt(&cipher)
}
