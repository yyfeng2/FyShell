//! MySQL 导入导出数据模型（契约 P2 阶段：CSV / JSON / SQL 三种格式）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql_io::MySqlExportOptions` 等。

use serde::{Deserialize, Serialize};

/// 导出格式（内部 tag 表示：序列化/反序列化为 { "format": "csv" } 等对象）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum MySqlExportFormat {
    Csv,
    Json,
    Sql,
}

/// 导入格式（内部 tag 表示：序列化/反序列化为 { "format": "csv" } 等对象）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum MySqlImportFormat {
    Csv,
    Sql,
}

/// 导出选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlExportOptions {
    /// 导出的查询 SQL（SELECT）
    pub sql: String,
    /// 导出格式
    pub format: MySqlExportFormat,
    /// 目标文件路径（前端保存对话框已让用户确认，后端直接覆盖写入）
    pub file_path: String,
    /// SQL 格式时附带 DROP + CREATE TABLE（表结构）
    pub include_create_table: bool,
}

/// 导入选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlImportOptions {
    /// 源文件路径
    pub file_path: String,
    /// 目标表（CSV 格式必填；SQL 格式的语句自带表名，此字段仅作展示）
    pub table: String,
    /// 导入格式
    pub format: MySqlImportFormat,
    /// 批量插入每批行数（默认 500，服务层 clamp 1-1000）
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    /// true = REPLACE INTO 覆盖已有主键
    pub replace: bool,
}

/// batch_size 的 serde 默认值
fn default_batch_size() -> u32 {
    500
}

/// 导入导出结果（rows_total：导出 = 导出行数 / 导入 = 导入行数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlIoResult {
    pub rows_total: u64,
    /// 耗时（毫秒）
    pub duration_ms: u64,
}
