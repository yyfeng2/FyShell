//! MySQL 用户管理模型（对齐 Navicat 用户面板：用户列表 / 建删用户 / 授权查看）。
//!
//! TS 侧同名同构，引用方式：`crate::models::mysql_user::MySqlUserInfo` 等。
//! 字段一律 snake_case（契约红线：禁用 rename_all="camelCase"）。

use serde::{Deserialize, Serialize};

/// mysql.user 表中的一行用户信息（User, Host 联合为主键）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlUserInfo {
    pub user: String,
    pub host: String,
    /// 备注（可空，空串表示无备注；mysql.user 表无 Comment 列，字段为契约预留）
    pub comment: String,
}

/// SHOW GRANTS FOR 的每条授权语句（单个用户可能有多条，含同义行）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MySqlGrantItem {
    /// 完整的 GRANT 语句原文（如 `GRANT SELECT ON *.* TO 'u'@'%'`）
    pub grant_sql: String,
}
