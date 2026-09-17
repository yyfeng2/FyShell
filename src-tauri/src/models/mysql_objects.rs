//! MySQL 数据库对象统一数据模型（契约第 5.2 节扩展）。
//!
//! 对齐 Navicat 的「数据库对象」面板：视图 / 函数 / 存储过程 / 触发器 / 事件。
//! TS 侧同名同构，引用方式：`crate::models::mysql_objects::MySqlObjectKind` 等。
//! 字段一律 snake_case（禁 camelCase，见 serde 命名约定）。

use serde::{Deserialize, Serialize};

/// 数据库对象类别（serde tag = "kind"，变体序列化为小写）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum MySqlObjectKind {
    View,
    Function,
    Procedure,
    Trigger,
    Event,
}

impl MySqlObjectKind {
    /// 类别 -> 小写字符串（命令层传字符串、服务层解析；Info/Ddl 的 kind 字段同用此串）
    pub fn as_str(&self) -> &'static str {
        match self {
            MySqlObjectKind::View => "view",
            MySqlObjectKind::Function => "function",
            MySqlObjectKind::Procedure => "procedure",
            MySqlObjectKind::Trigger => "trigger",
            MySqlObjectKind::Event => "event",
        }
    }
}

/// 数据库对象列表项（object_list 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlObjectInfo {
    pub name: String,
    /// 对象类别（小写 view/function/procedure/trigger/event，IPC 传输为字符串）
    pub kind: String,
    /// 注释（仅视图有值，来自 information_schema.VIEWS.TABLE_COMMENT；其余对象留空）
    pub comment: String,
}

/// 数据库对象 DDL（object_ddl 返回）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlObjectDdl {
    pub name: String,
    /// 对象类别（同上，IPC 传输为字符串）
    pub kind: String,
    /// 完整 CREATE 语句（SHOW CREATE 结果，含 DEFINER / ALGORITHM 等子句）
    pub sql: String,
}
