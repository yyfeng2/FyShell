//! 凭据保险库模型（commands/vault.rs ↔ api/vault.ts）
//!
//! 仅一个状态模型：前端据此决定 解锁表单 / 明文直读 / 加密保存 三种路径。

use serde::Serialize;

/// 保险库状态（vault_status 返回）
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct VaultStatus {
    /// 是否已设置主密码（未设置时凭据沿用明文存储，向后兼容）
    pub has_master_password: bool,
    /// 当前是否已解锁（解锁后 vault_encrypt/vault_decrypt 可用）
    pub unlocked: bool,
}
