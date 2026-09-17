//! SQL 控制台数据模型（查询历史 + 执行计划 + 多结果集）。
//!
//! TS 侧同名同构；字段名一律 snake_case（无 camelCase rename）。

use serde::{Deserialize, Serialize};

/// 查询历史条目（SQLite 持久化，history_list / history_search 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlQueryHistoryItem {
    pub id: i64,
    /// 执行该语句的连接标识（表列名 conn_host）
    pub conn_id: String,
    pub sql: String,
    /// 格式 "YYYY-MM-DD HH:MM:SS"（SQLite datetime('now','localtime') 生成）
    pub created_at: String,
    /// 执行耗时毫秒数（None = 未记录）
    pub duration_ms: Option<u64>,
    /// 当前版本仅记录成功执行的语句（表结构无 success 列，恒为 true，预留扩展）
    pub success: bool,
}

/// 已保存查询条目（命名保存，区别于自动记录的查询历史；saved_query_list 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlSavedQueryItem {
    pub id: i64,
    /// 用户命名（表内 UNIQUE 约束，同名保存走覆盖语义）
    pub name: String,
    /// 绑定的连接标识（None = 不绑定连接，全局可见）
    pub conn_id: Option<String>,
    pub sql: String,
    /// 格式 "YYYY-MM-DD HH:MM:SS"（SQLite datetime('now','localtime') 生成）
    pub created_at: String,
}

/// 执行计划结果
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlExplainResult {
    /// "rows" = 传统表格格式；"tree" = 树形文本（EXPLAIN ANALYZE 输出）
    pub format: String,
    /// 列名（format = "rows" 时有效，与 rows 逐行对应）
    pub columns: Vec<String>,
    /// 行数据（None = SQL NULL，值统一序列化为字符串）
    pub rows: Vec<Vec<Option<String>>>,
    /// 树形文本（EXPLAIN ANALYZE / FORMAT=TREE 的单列文本）
    pub tree: Option<String>,
}
