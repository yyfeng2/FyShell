//! 键位映射数据模型
//!
//! 与前端 TS 类型同名同构；键位经 ui store 的 shortcutOf 归一化为
//! "ctrl+shift+x" 形式，拦截点在 xterm attachCustomKeyEventHandler。

use serde::{Deserialize, Serialize};

/// 键位映射动作类型
///
/// - `send_string`：向会话发送 payload 字符串
/// - `menu_command`：执行 payload 对应的应用菜单命令（对齐 MenuBar action 集）
pub const ACTION_SEND_STRING: &str = "send_string";
pub const ACTION_MENU_COMMAND: &str = "menu_command";

/// 键位映射（键盘操作由菜单功能、发送字符串等自定义）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct KeyMapping {
    /// uuid v4
    pub id: String,
    /// 归一化键位组合（如 "ctrl+shift+x" / "alt+f1"）
    pub key_combo: String,
    /// 动作类型（send_string | menu_command）
    pub action_type: String,
    /// 动作载荷：发送的字符串 或 菜单命令 action 名
    pub payload: String,
}
