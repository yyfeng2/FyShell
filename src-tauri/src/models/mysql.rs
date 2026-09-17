//! MySQL 基础数据模型（契约第 5.1 节）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql::MySqlConnection` 等。

use serde::{Deserialize, Serialize};

/// MySQL 连接配置（契约 5.1 节）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlConnection {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// 默认数据库（None = 连接时不指定，需在 SQL 中用全限定表名）
    pub schema: Option<String>,
}

/// 表信息（表列表命令返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlTableInfo {
    pub name: String,
    /// 预估行数（InnoDB 下为估算值，来自 information_schema.TABLES.TABLE_ROWS）
    pub rows: u64,
    pub comment: String,
    pub engine: String,
}

/// 查询结果（分页 + 列名 + 行数据）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlQueryResult {
    pub columns: Vec<String>,
    /// None = SQL NULL（值统一序列化为字符串，前端按 NULL 标识渲染）
    pub rows: Vec<Vec<Option<String>>>,
    /// 总行数（COUNT(*) 包装查询得出）
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}
