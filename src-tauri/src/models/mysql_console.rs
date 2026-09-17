//! SQL 控制台数据模型（查询历史 + 执行计划 + 多结果集）。
//!
//! TS 侧同名同构；字段名一律 snake_case（无 camelCase rename）。

use serde::{Deserialize, Serialize};

/// 查询历史条目（SQLite 持久化，history_list / history_search 返回）
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// 执行计划单行节点（扁平行列表，前端按 id/parent_id 自行组树）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlExplainNode {
    /// EXPLAIN 的 select_type+table 组合或树节点标识
    pub id: String,
    /// 父节点标识（None = 根节点）
    pub parent_id: Option<String>,
    /// 键值对形式，兼容不同 EXPLAIN 格式的列（键为列名、值为字符串化的值）
    pub values: Vec<(String, String)>,
}

/// 执行计划结果
#[derive(Debug, Clone, Serialize, Deserialize)]
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
