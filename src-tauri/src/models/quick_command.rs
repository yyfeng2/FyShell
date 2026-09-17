//! 快捷命令数据模型（契约第 5.1 节）
//!
//! 与前端 TS 类型同名同构；树状分类参考 Xshell 8 模式。
//! 层级由 `group_id` / `parent_id` 表达，`qc_list` 返回扁平全量列表，前端组装。

use serde::{Deserialize, Serialize};

/// 快捷命令（契约第 5.1 节）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCommand {
    /// uuid v4
    pub id: String,
    /// 显示名
    pub name: String,
    /// 命令正文（可多行，发送到会话时按行回车）
    pub command_text: String,
    /// 所属分组（None = 根级）
    pub group_id: Option<String>,
}

/// 快捷命令树文件夹（契约第 5.1 节）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickCommandFolder {
    /// uuid v4
    pub id: String,
    /// 文件夹名
    pub name: String,
    /// 父文件夹（None = 根级）
    pub parent_id: Option<String>,
}

/// 快捷命令树节点：文件夹或命令（契约第 5.1 节）
///
/// serde tag 字段为 `kind`，值为 `"folder" | "command"`（internally tagged），
/// 与 `SessionNode` 同模式；层级由 `group_id` / `parent_id` 表达。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QuickCommandNode {
    /// 文件夹分组
    Folder(QuickCommandFolder),
    /// 快捷命令
    Command(QuickCommand),
}
