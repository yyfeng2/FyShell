//! SSH 协议跟踪日志开关（SSH 选项对话框「高级 → 跟踪」）
//!
//! 模块级 `AtomicBool` 开关：开启后 ssh.rs 中诊断类 `eprintln!` 输出连接过程
//! 详细日志（kex/认证/PTY 就绪等），关闭时仅保留错误类输出。
//! 开关值持久化在 settings 表（`sshopt_trace_enabled`），连接时刷新（低频操作）。

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::AppError;

static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);

/// 从 settings 表刷新开关（lib.rs setup 与每次 connect 前调用，SQLite 快查可接受）
pub fn refresh_from_settings() -> Result<(), AppError> {
    let enabled = crate::services::settings_store::get("sshopt_trace_enabled")?
        .map(|v| v == "true")
        .unwrap_or(false);
    TRACE_ENABLED.store(enabled, Ordering::Relaxed);
    Ok(())
}

/// 跟踪是否开启（诊断类 eprintln 的输出条件）
pub fn enabled() -> bool {
    TRACE_ENABLED.load(Ordering::Relaxed)
}

/// 跟踪开启时输出诊断日志（封装 `if enabled() { eprintln! }` 模式）
#[macro_export]
macro_rules! ssh_trace {
    ($($arg:tt)*) => {
        if $crate::services::trace::enabled() {
            eprintln!($($arg)*);
        }
    };
}
