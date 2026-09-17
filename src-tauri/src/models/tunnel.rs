//! SSH 隧道数据模型（契约第 5.1 节）
//!
//! 与前端 TS 类型同名同构；`kind` / `status` 为无载荷枚举，
//! 序列化为 PascalCase 变体名（"Local" | "Remote" | "Socks" 等），
//! 与 TransferKind / TransferStatus 的序列化风格一致。

use serde::{Deserialize, Serialize};

/// 隧道类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelKind {
    /// 本地转发：本地监听端口 → SSH → target_host:target_port
    Local,
    /// 远程转发：SSH 侧监听（P1 简化为记录配置，启动时返回明确错误）
    Remote,
    /// SOCKS5 代理：本地最小 SOCKS5 服务端，目标经 SSH direct-tcpip 转发
    Socks,
}

impl TunnelKind {
    /// SQLite 存储 / 日志用字符串（与 serde 序列化结果一致）
    pub fn as_str(&self) -> &'static str {
        match self {
            TunnelKind::Local => "Local",
            TunnelKind::Remote => "Remote",
            TunnelKind::Socks => "Socks",
        }
    }

    /// 从字符串解析（SQLite 读取；未知值容错为 Local）
    pub fn parse(s: &str) -> Self {
        match s {
            "Remote" => TunnelKind::Remote,
            "Socks" => TunnelKind::Socks,
            _ => TunnelKind::Local,
        }
    }
}

/// 隧道状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelStatus {
    /// 已停止
    Stopped,
    /// 监听中
    Listening,
    /// 错误（error 字段携带原因）
    Error,
}

impl TunnelStatus {
    /// SQLite 存储 / 日志用字符串（与 serde 序列化一致）
    pub fn as_str(&self) -> &'static str {
        match self {
            TunnelStatus::Stopped => "Stopped",
            TunnelStatus::Listening => "Listening",
            TunnelStatus::Error => "Error",
        }
    }

    /// 从字符串解析（SQLite 读取；未知值容错为 Stopped）
    pub fn parse(s: &str) -> Self {
        match s {
            "Listening" => TunnelStatus::Listening,
            "Error" => TunnelStatus::Error,
            _ => TunnelStatus::Stopped,
        }
    }
}

/// SSH 隧道规则（契约第 5.1 节）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRule {
    /// uuid v4
    pub id: String,
    /// 所属 SSH 会话 id（= SessionConfig.id）
    pub session_id: String,
    /// 隧道类型：Local | Remote | Socks
    pub kind: TunnelKind,
    /// 本地监听地址，默认 127.0.0.1
    pub listen_host: String,
    /// 本地监听端口
    pub listen_port: u16,
    /// 目标主机：Local/Remote 为转发目标；Socks 为空串
    pub target_host: String,
    /// 目标端口：Local/Remote 为目标端口；Socks 为 0
    pub target_port: u16,
    /// 会话连接时是否自动启动
    pub enabled: bool,
    /// 运行状态：Stopped | Listening | Error
    pub status: TunnelStatus,
    /// 错误信息（status = Error 时非空）
    pub error: Option<String>,
}
