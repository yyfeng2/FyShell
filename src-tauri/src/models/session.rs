//! 会话管理数据模型（契约第 1 节）
//!
//! 与前端 TS 类型同名同构；敏感字段（密码、口令短语）的 `Debug` 输出已手动脱敏，
//! 避免在日志中泄漏凭据。序列化（Serialize）结果仅用于 IPC 传输，禁止写入日志。

use serde::{Deserialize, Serialize};

/// 认证方式（5 种，契约第 1 节）
///
/// TS 侧为可辨识联合（discriminated union），tag 字段为 `type`，
/// 值为 `"password" | "publicKey" | "interactive" | "noAuth" | "jump"`。
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthType {
    /// 密码认证
    Password {
        password: String,
    },
    /// 私钥认证：私钥文件路径 + 可选口令
    PublicKey {
        private_key_path: String,
        passphrase: Option<String>,
    },
    /// 交互式（keyboard-interactive）
    Interactive {
        password: String,
    },
    /// 不验证
    NoAuth,
    /// 跳板机：引用另一个已存会话
    Jump {
        jump_session_id: String,
    },
}

// 手动实现 Debug：密码、口令短语等敏感字段一律脱敏，防止日志泄漏
impl std::fmt::Debug for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { .. } => f.write_str("Password {{ password: *** }}"),
            Self::PublicKey { private_key_path, .. } => {
                // 私钥文件路径仅是位置引用，不算凭据；口令短语脱敏
                write!(f, "PublicKey {{ private_key_path: {private_key_path:?}, passphrase: *** }}")
            }
            Self::Interactive { .. } => f.write_str("Interactive {{ password: *** }}"),
            Self::NoAuth => f.write_str("NoAuth"),
            Self::Jump { jump_session_id } => {
                write!(f, "Jump {{ jump_session_id: {jump_session_id:?} }}")
            }
        }
    }
}

/// 会话配置（存 Rust 侧 SQLite，契约第 1 节）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SessionConfig {
    /// uuid v4
    pub id: String,
    /// 显示名
    pub name: String,
    /// 所属文件夹（None = 根级）
    pub folder_id: Option<String>,
    /// 主机地址
    pub host: String,
    /// 端口，默认 22
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式（5 种）
    pub auth_type: AuthType,
    /// 编码，默认 "UTF-8"
    pub encoding: String,
    /// Tab 着色（hex）
    pub color: Option<String>,
    /// 保活间隔（秒），默认 30
    pub keepalive_interval: u32,
    /// 引用的认证配置文件 id（可选，None = 使用会话自身认证配置）。
    /// serde default：老数据缺该字段时反序列化为 None，向后兼容 P0 流程。
    #[serde(default)]
    pub profile_id: Option<String>,
    /// 会话类型：Some("mysql") = 数据库会话，Some("telnet"/"rlogin") = Telnet 兼容，
    /// Some("serial") = 串口，None 或 Some("ssh") = SSH 会话。
    /// serde default：老数据缺该字段时反序列化为 None（视为 SSH），向后兼容。
    #[serde(default)]
    pub session_type: Option<String>,
    /// 串口会话：端口名（session_type == "serial" 时生效）
    #[serde(default)]
    pub serial_port: Option<String>,
    /// 串口会话：波特率（默认 115200，由前端给缺省值）
    #[serde(default)]
    pub baud_rate: Option<u32>,
    /// 备注/说明（可选，serde default：老数据缺该字段时反序列化为 None，向后兼容）
    #[serde(default)]
    pub description: Option<String>,
    /// 最后修改时间（Unix 秒；serde default：老数据缺该字段时反序列化为 0，向后兼容）
    #[serde(default)]
    pub updated_at: i64,
}

/// 会话树文件夹
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct SessionFolder {
    /// uuid v4
    pub id: String,
    /// 文件夹名
    pub name: String,
    /// 父文件夹（None = 根级）
    pub parent_id: Option<String>,
}

/// 会话树节点：文件夹或会话（契约第 1 节）
///
/// serde tag 字段为 `kind`，值为 `"folder" | "session"`；
/// 层级由 `folder_id` / `parent_id` 表达，`session_list` 返回扁平全量列表。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionNode {
    Folder(SessionFolder),
    Session(SessionConfig),
}
