//! 会话输出日志：模块级注册表管理 enabled 状态与写入路径
//!
//! - 落盘路径：`app_data_dir/logs/<session_id>/<YYYY-MM-DD>.log`
//! - `write_log` 由 rust-ssh 在读循环中高频调用，必须无锁竞争小开销：
//!   未启用时立即返回（一次哈希表查询）；启用时追加写入，日期字符串按天缓存，
//!   避免每次写盘重复格式化。写失败静默忽略（日志落盘不影响终端输出）
//! - 不挂在 `AppState` 上，lib.rs setup 中调用一次 `session_log::init(&app_data_dir)`

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{Datelike, Local};

use crate::error::AppError;

/// 模块级注册表：init 后全局可用
static LOG_STATE: OnceLock<LogState> = OnceLock::new();

/// 会话日志状态：enabled 集合 + 写入路径 + 日期缓存
struct LogState {
    /// 日志根目录：app_data_dir/logs
    logs_root: PathBuf,
    /// 已开启落盘的会话 id 集合
    enabled: Mutex<HashSet<String>>,
    /// 本地日期缓存：(epoch 天数, "YYYY-MM-DD")，跨天时才重新格式化
    today_cache: Mutex<(i64, String)>,
}

/// 初始化（lib.rs setup 中调用；日志根目录不存在则自动创建）。
///
/// 幂等：重复调用时保留首次初始化的实例。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    let logs_root = app_data_dir.join("logs");
    std::fs::create_dir_all(&logs_root)?;
    let _ = LOG_STATE.set(LogState {
        logs_root,
        enabled: Mutex::new(HashSet::new()),
        today_cache: Mutex::new((0, String::new())),
    });
    Ok(())
}

/// 开关某会话的输出落盘；开启时确保该会话的日志目录存在
pub fn toggle(session_id: &str, enabled: bool) -> Result<(), AppError> {
    validate_session_id(session_id)?;
    let Some(state) = LOG_STATE.get() else {
        return Ok(());
    };
    let mut set = state.enabled.lock().expect("session_log enabled 锁中毒");
    if enabled {
        if set.insert(session_id.to_string()) {
            // 首次开启：建好 logs/<session_id>/ 目录，写入路径不再每次建目录
            let _ = std::fs::create_dir_all(state.logs_root.join(session_id));
        }
    } else {
        set.remove(session_id);
    }
    Ok(())
}

/// 追加写入会话输出日志（由 rust-ssh 读循环调用，热路径）。
///
/// 未启用时立即返回；IO 失败静默丢弃，绝不向上传播影响终端数据流。
pub fn write_log(session_id: &str, data: &[u8]) {
    let Some(state) = LOG_STATE.get() else {
        return;
    };
    // 未启用时快速返回（仅一次锁 + 一次哈希查询）
    {
        let set = state.enabled.lock().expect("session_log enabled 锁中毒");
        if !set.contains(session_id) {
            return;
        }
    }
    let date = today_str(state);
    let path = state.logs_root.join(session_id).join(format!("{date}.log"));
    // 追加写入；错误静默忽略（终端输出不能因日志失败而中断）
    let _ = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, data));
}

/// 当前本地日期（YYYY-MM-DD），按天缓存（跨天时才重新格式化）
fn today_str(state: &LogState) -> String {
    let now = Local::now();
    let days = i64::from(now.num_days_from_ce());
    let mut cache = state.today_cache.lock().expect("session_log 日期缓存锁中毒");
    if cache.0 != days {
        *cache = (days, now.format("%Y-%m-%d").to_string());
    }
    cache.1.clone()
}

/// 列出某会话的全部日志日期（YYYY-MM-DD，升序）
pub fn list_dates(session_id: &str) -> Result<Vec<String>, AppError> {
    validate_session_id(session_id)?;
    let Some(state) = LOG_STATE.get() else {
        return Err(AppError::general("会话日志模块未初始化"));
    };
    let dir = state.logs_root.join(session_id);
    let mut dates = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // 仅识别 <YYYY-MM-DD>.log 命名的日志文件
        if is_valid_date(&name) {
            if let Some(date) = name.strip_suffix(".log") {
                dates.push(date.to_string());
            }
        }
    }
    dates.sort();
    Ok(dates)
}

/// 读取某会话某天的日志内容；date 格式必须为 YYYY-MM-DD，当次尚无日志时返回空串
pub fn read_date(session_id: &str, date: &str) -> Result<String, AppError> {
    validate_session_id(session_id)?;
    if !is_valid_date(date) {
        return Err(AppError::general("日期格式必须为 YYYY-MM-DD"));
    }
    let Some(state) = LOG_STATE.get() else {
        return Err(AppError::general("会话日志模块未初始化"));
    };
    let path = state.logs_root.join(session_id).join(format!("{date}.log"));
    // 不存在返回空串（当次尚无日志）
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}

/// session_id 校验：非空且不含路径分隔符 / 上级引用（防路径穿越）
fn validate_session_id(session_id: &str) -> Result<(), AppError> {
    if session_id.trim().is_empty() {
        return Err(AppError::general("session_id 不能为空"));
    }
    if session_id.contains('/') || session_id.contains('\\') || session_id.contains("..") {
        return Err(AppError::general("session_id 含非法字符"));
    }
    Ok(())
}

/// 日期格式校验：`YYYY-MM-DD`（数字 + 连字符，防路径穿越）
fn is_valid_date(date: &str) -> bool {
    let b = date.as_bytes();
    b.len() == 10
        && b.iter()
            .enumerate()
            .all(|(i, &c)| if i == 4 || i == 7 { c == b'-' } else { c.is_ascii_digit() })
}
