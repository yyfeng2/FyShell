//! 前端诊断日志命令（临时调试用）：把前端关键路径日志写到 Rust stdout，
//! dev 模式下随进程输出到 dev.log，便于不打开 DevTools 时定位链路问题。

#[tauri::command]
#[specta::specta]
pub fn debug_log(message: String) {
    eprintln!("[web] {message}");
}
