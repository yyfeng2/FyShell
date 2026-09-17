//! 登录脚本解析（SSH 选项对话框「连接 → 登录脚本」）
//!
//! 脚本内容按行拆分为待发送命令；`expect => send` 语法保留解析
//! （本期行为上按固定间隔逐行发送，expect 暂忽略，面板提示中说明）。
//! 发送逻辑在 ssh.rs（写路径私有），本模块只负责解析。

/// 单行脚本：send 为待发送命令，expect 保留字段（本期未使用）
pub struct ScriptLine {
    #[allow(dead_code)]
    pub expect: Option<String>,
    pub send: String,
}

/// 解析脚本内容：每行一条命令，`expect => send` 形式拆分；
/// 跳过空行与 `#` 开头的注释行。
pub fn parse(content: &str) -> Vec<ScriptLine> {
    let mut lines = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((expect, send)) = line.split_once("=>") {
            let expect = expect.trim();
            lines.push(ScriptLine {
                expect: (!expect.is_empty()).then(|| expect.to_string()),
                send: send.trim().to_string(),
            });
        } else {
            lines.push(ScriptLine {
                expect: None,
                send: line.to_string(),
            });
        }
    }
    lines
}
