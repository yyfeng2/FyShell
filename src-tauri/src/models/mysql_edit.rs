//! MySQL 数据编辑模型（P2 阶段：按主键行编辑 / 批量编辑 / 预览确认管道）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql_edit::MySqlRowUpdate` 等。
//! 字段一律 snake_case（契约红线：禁用 rename_all="camelCase"）。

use serde::{Deserialize, Serialize};

/// 单行单列更新（按主键定位行，一次只改一列）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlRowUpdate {
    pub table: String,
    pub pk_column: String,
    /// 主键定位值（None = 主键本身为 NULL，按 `pk IS NULL` 定位）
    pub pk_value: Option<String>,
    pub column: String,
    /// 新值（与 MySqlQueryResult.rows 的 Option<String> 对齐：None = NULL）
    pub value: Option<String>,
    /// 显式 NULL 标识：true 时值按 NULL 写入（区分空串与 NULL，建立信任）
    pub is_null: bool,
}

/// 批量更新（每条按主键定位，逐条执行）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlRowUpdateBatch {
    pub table: String,
    pub updates: Vec<MySqlRowUpdate>,
}

/// 更新预览（预览 -> 确认 -> 执行管道的第一步产物）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlEditPreview {
    /// 将要执行的完整 UPDATE 语句（值已转义内联）
    pub sql: String,
    /// COUNT(*) 估算的受影响行数
    pub affected_estimate: u64,
    /// 是否危险（复用 services/mysql.rs 的 is_dangerous_sql 判定）
    pub danger: bool,
    pub danger_reason: Option<String>,
}
