//! MySQL 表设计器数据模型（契约第 5.2 节扩展，P2 阶段）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql_design::MySqlTableDesign` 等。
//! 字段一律 snake_case（禁 camelCase，见 serde 命名约定）。

use serde::{Deserialize, Serialize};

/// 列信息（information_schema.COLUMNS 组装）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlColumnInfo {
    pub name: String,
    /// 完整类型定义（如 `int unsigned` / `varchar(255)`，来自 COLUMN_TYPE）
    pub data_type: String,
    pub is_nullable: bool,
    /// 默认值（None = 无默认值；NULL 默认值存字符串 "NULL"）
    pub default_value: Option<String>,
    pub comment: String,
    /// 附加属性（如 `auto_increment` / `VIRTUAL GENERATED`，来自 EXTRA）
    pub extra: String,
    /// 键类型：PRI / UNI / MUL（None = 非键列，来自 COLUMN_KEY，等价 SHOW COLUMNS 的 Key 列）
    pub key_type: Option<String>,
    /// 字符集（二进制 / 数值类型列为 None）
    pub character_set: Option<String>,
    /// 排序规则（二进制 / 数值类型列为 None）
    pub collation: Option<String>,
}

/// 索引信息
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlIndexInfo {
    pub name: String,
    /// 索引覆盖的列（按 SEQ_IN_INDEX 顺序）
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    /// 索引类型（BTREE / FULLTEXT / HASH 等）
    pub index_type: String,
}

/// 外键信息
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlForeignKeyInfo {
    pub name: String,
    /// 本表外键列（按 ORDINAL_POSITION 顺序）
    pub columns: Vec<String>,
    /// 引用的目标表
    pub ref_table: String,
    /// 引用的目标列（与 columns 一一对应）
    pub ref_columns: Vec<String>,
    /// ON DELETE 规则（RESTRICT / CASCADE / SET NULL / NO ACTION，空串 = 未指定）
    pub on_delete: String,
    /// ON UPDATE 规则（同上）
    pub on_update: String,
}

/// 表设计快照（get_design 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlTableDesign {
    pub table: String,
    pub columns: Vec<MySqlColumnInfo>,
    pub indexes: Vec<MySqlIndexInfo>,
    pub foreign_keys: Vec<MySqlForeignKeyInfo>,
    pub engine: String,
    /// 默认字符集（如 utf8mb4）
    pub charset: String,
    pub comment: String,
}

/// 字段级变更动作：新增
pub const ACTION_ADD: &str = "add";
/// 字段级变更动作：修改
pub const ACTION_MODIFY: &str = "modify";
/// 字段级变更动作：删除（此时仅 name 有意义）
pub const ACTION_DROP: &str = "drop";

/// 字段级变更
///
/// drop 时仅 `name` 有意义，其余字段由前端省略（serde default 兜底）。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlColumnChange {
    /// 动作：add / modify / drop（取值见 ACTION_* 常量）
    pub action: String,
    pub name: String,
    /// 完整类型定义（如 `varchar(255)` / `int unsigned`）
    #[serde(default)]
    pub data_type: String,
    #[serde(default)]
    pub is_nullable: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub comment: String,
    /// 附加属性（如 `AUTO_INCREMENT`）
    #[serde(default)]
    pub extra: String,
}

/// 表设计变更（apply_change 入参）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlDesignChange {
    pub table: String,
    /// true = 新建表（生成 CREATE TABLE），false = 修改既有表（生成 ALTER TABLE）
    pub is_new: bool,
    /// 字段级变更列表（is_new 时全部视为新增）
    #[serde(default)]
    pub columns: Vec<MySqlColumnChange>,
    /// 新增的索引列表
    #[serde(default)]
    pub indexes: Vec<MySqlIndexInfo>,
    /// 删除的索引名列表（主键用 "PRIMARY" 标识）
    #[serde(default)]
    pub dropped_indexes: Vec<String>,
    /// 新增的外键列表
    #[serde(default)]
    pub foreign_keys: Vec<MySqlForeignKeyInfo>,
    /// 删除的外键名列表
    #[serde(default)]
    pub dropped_foreign_keys: Vec<String>,
    /// 引擎变更（None = 不变）
    #[serde(default)]
    pub engine: Option<String>,
    /// 字符集变更（None = 不变）
    #[serde(default)]
    pub charset: Option<String>,
    /// 表注释变更（None = 不变）
    #[serde(default)]
    pub comment: Option<String>,
}
