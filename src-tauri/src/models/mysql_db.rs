//! MySQL 数据库级管理与表结构快捷操作的数据模型（契约第 5.2 节扩展）。
//!
//! 字段一律 snake_case（契约红线：禁用 rename_all="camelCase"）。

use serde::{Deserialize, Serialize};

/// 数据库清单（mysql_db_list 返回）
///
/// host 与 current_db 一并回传：前端「复制 Host」按钮与数据库切换下拉共用，
/// 避免再起一条命令。host 取自连接池 Opts（ip_or_hostname）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlDatabaseList {
    pub host: String,
    /// 当前默认数据库（None = 连接时未指定且未 USE，需先选择库）
    pub current_db: Option<String>,
    /// 该连接可见的全部数据库名（SHOW DATABASES 结果，已排序）
    pub databases: Vec<String>,
}

/// 表结构 DDL（mysql_table_show_create 返回，SHOW CREATE TABLE 结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlTableDdl {
    pub table: String,
    pub sql: String,
}
