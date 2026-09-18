//! Redis 数据模型
//!
//! TS 侧同名同构，引用方式：`crate::models::redis::RedisConnection` 等。
//! 字段一律 snake_case（bindings 无 camelCase 转换）。

use serde::{Deserialize, Serialize};
use specta_typescript::Any;

/// Redis 连接配置
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RedisConnection {
    pub host: String,
    pub port: u16,
    /// ACL 用户名（可空）
    pub username: Option<String>,
    /// 密码（可空=无密码 Redis）
    pub password: Option<String>,
    /// 默认库 index
    pub db: u8,
}

/// 任意命令执行结果（前端按 kind 渲染）
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RedisExecResult {
    /// "nil" | "int" | "string" | "blob" | "array" | "map" | "status" | "error"
    pub kind: String,
    /// 任意 JSON（递归的 serde_json::Value 无法被 specta-typescript 内联，映射为 TS any）
    #[specta(type = Any)]
    pub value: serde_json::Value,
}
