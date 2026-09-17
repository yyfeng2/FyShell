//! 认证配置文件数据模型（契约第 5.1 节）
//!
//! 认证变量集独立管理，一处改全局生效；`SessionConfig.profile_id` 可选引用。
//! 凭据（密码、口令短语）的 `Debug` 输出由 `AuthType` 手动脱敏实现，此处派生即可。

use serde::{Deserialize, Serialize};

use crate::models::session::AuthType;

/// 认证配置文件（契约第 5.1 节）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AuthProfile {
    /// uuid v4
    pub id: String,
    /// 显示名
    pub name: String,
    /// 认证方式（复用 5 种认证方式，含凭据）
    pub auth_type: AuthType,
}
