//! 统一错误类型：所有 commands 层命令返回 `Result<T, AppError>`
//!
//! 序列化为字符串（前端收到的是可读文本，而非结构体），
//! 契约红线：error.rs 定义 AppError（thiserror + Serialize，serialize 为字符串）。

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 文件系统 / 网络 IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// SSH 连接 / 认证 / PTY 错误（russh 错误统一转字符串归入此变体）
    #[error("SSH 错误: {0}")]
    Ssh(String),

    /// SQLite 持久化错误（会话配置存储）
    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// 序列化 / 反序列化错误
    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    /// 通用错误（参数校验、业务规则等）
    #[error("{0}")]
    General(String),
}

impl AppError {
    /// 便捷构造：任意字符串转 General 变体
    pub fn general(msg: impl Into<String>) -> Self {
        AppError::General(msg.into())
    }
}

// russh 错误归入 Ssh 变体：转字符串，避免依赖 russh 内部 Error trait 的实现细节
impl From<russh::Error> for AppError {
    fn from(err: russh::Error) -> Self {
        AppError::Ssh(err.to_string())
    }
}

// Tauri IPC 要求错误类型实现 Serialize；此处序列化为可读字符串
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// tauri-specta 要求命令错误类型实现 specta::Type；AppError 的 Serialize 行为
// 是字符串，故类型形状按 string 导出（不生成命名的错误结构体）
impl specta::Type for AppError {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta::datatype::Primitive::str.into()
    }
}
