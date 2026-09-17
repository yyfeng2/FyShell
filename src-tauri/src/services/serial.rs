//! 串口终端服务：基于 serialport crate 的串口枚举、打开与字节流读写。
//!
//! 架构与 services/ssh.rs 的"字节流终端"模式对齐，仅传输层不同：
//! - 枚举本机串口（serial_list）供连接表单下拉选择；
//! - 读循环在独立线程阻塞读取（读超时轮询存活标志），经 tauri::ipc::Channel 透传，
//!   ANSI/文本解析交给 xterm.js；
//! - 串口无终端尺寸概念（resize 无效）；
//! - 断开时置存活标志为 false 并同步清理 AppState::serial_sessions 中的句柄。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::Manager;

use crate::error::AppError;
use crate::state::AppState;

/// 读循环批量发送阈值：单次 read 缓冲 4KB，减少 IPC 次数
const READ_BATCH_BYTES: usize = 4096;

/// 串口读超时（毫秒）：读循环按此节奏轮询存活标志，断开后及时退出
const READ_TIMEOUT_MS: u64 = 100;

/// session-status 事件的状态字符串（与 ssh.rs 一致）
const STATUS_CONNECTING: &str = "connecting";

/// std::sync::Mutex 中毒时的统一错误转换
fn lock_err<T>(_e: PoisonError<T>) -> AppError {
    AppError::general("全局状态锁被污染")
}

// ---------------------------------------------------------------------------
// 会话句柄
// ---------------------------------------------------------------------------

/// 单个串口会话句柄：持有键盘输入队列与存活标志，
/// 提供 write / close 能力。存于 `AppState::serial_sessions`。
#[derive(Clone)]
pub struct SerialSessionHandle {
    /// 键盘输入队列，由独立写转发线程消费
    write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// 存活标志：断开时置 false，读循环每轮检查后退出（串口无 shutdown 语义，
    /// Windows 下 try_clone 克隆的句柄在原句柄 drop 后仍有效，无法依赖 drop 关闭）
    alive: Arc<AtomicBool>,
}

/// 串口信息（连接表单展示用）
#[derive(Clone, serde::Serialize, specta::Type)]
pub struct SerialPortInfo {
    /// 串口名（如 "COM3"、"/dev/ttyUSB0"）
    pub port_name: String,
    /// 串口类型描述（"USB"/"PCI"/"蓝牙"/"未知"）
    pub port_type: String,
}

// ---------------------------------------------------------------------------
// 读循环 / 写转发
// ---------------------------------------------------------------------------

/// 读循环：带超时阻塞读取串口输出 → `Channel<Vec<u8>>` send。
/// 超时视为正常轮询节奏（检查存活标志后继续）；设备拔出等错误时推送断开状态。
fn read_loop(
    app: tauri::AppHandle,
    session_id: String,
    mut port: Box<dyn serialport::SerialPort>,
    on_output: Channel<Vec<u8>>,
    alive: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; READ_BATCH_BYTES];
    let mut first_chunk = true;
    loop {
        // 断开后及时退出（read 超时最多 READ_TIMEOUT_MS，轮询节奏可控）
        if !alive.load(Ordering::Relaxed) {
            break;
        }
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                if first_chunk {
                    first_chunk = false;
                    eprintln!("[serial] {} first output chunk: {} bytes", session_id, n);
                }
                // 会话日志开启时同步追加落盘（未启用时 write_log 立即返回）
                crate::services::session_log::write_log(&session_id, &buf[..n]);
                if on_output.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(e) => {
                // 超时为正常轮询节奏（serialport 的超时错误经 StdError::kind()
                // 归一为 io::ErrorKind::TimedOut，POSIX 后端可能为 WouldBlock）；
                // 其余错误（含设备拔出）退出
                match e.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => continue,
                    _ => {
                        eprintln!("[serial] {} read error: {e}", session_id);
                        break;
                    }
                }
            }
        }
    }
    // 断开：推送 disconnected 状态并同步清理句柄（对齐 ssh.rs Handler::disconnected）
    crate::services::ssh::emit_status(&app, &session_id, "disconnected");
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut sessions) = state.serial_sessions.lock() {
            sessions.remove(&session_id);
        }
    }
}

/// 写转发线程：消费键盘输入队列，写入串口
fn write_forward(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    mut port: Box<dyn serialport::SerialPort>,
) {
    while let Some(data) = rx.blocking_recv() {
        if port.write_all(&data).is_err() {
            break;
        }
    }
    // 全部 Sender drop（会话关闭）后线程退出，port drop 关闭串口句柄
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 枚举本机串口列表（供连接表单下拉选择）
pub fn list() -> Result<Vec<SerialPortInfo>, AppError> {
    let ports = serialport::available_ports()
        .map_err(|e| AppError::general(format!("枚举串口失败: {e}")))?;
    Ok(ports
        .into_iter()
        .map(|p| SerialPortInfo {
            port_name: p.port_name,
            port_type: port_type_label(&p.port_type),
        })
        .collect())
}

/// 串口类型转可读描述
fn port_type_label(port_type: &serialport::SerialPortType) -> String {
    match port_type {
        serialport::SerialPortType::UsbPort { .. } => "USB".into(),
        serialport::SerialPortType::PciPort => "PCI".into(),
        serialport::SerialPortType::BluetoothPort => "蓝牙".into(),
        serialport::SerialPortType::Unknown => "未知".into(),
    }
}

/// 打开串口并启动读写循环。
/// 输出走 tauri::ipc::Channel，状态通过 session-status 事件推送。
/// key：连接路由键（串口连接无持久会话，每标签唯一，由前端生成）。
pub async fn connect(
    app: &tauri::AppHandle,
    state: &AppState,
    key: &str,
    port_name: &str,
    baud_rate: u32,
    on_output: tauri::ipc::Channel<Vec<u8>>,
) -> Result<(), AppError> {
    // 防止重复连接：同键已存在时先断开旧会话再完整重建（对齐 ssh.rs）
    {
        let existing = state.serial_sessions.lock().map_err(lock_err)?.remove(key);
        if existing.is_some() {
            eprintln!("[serial] {} already connected, rebuilding session", key);
        }
    }
    eprintln!("[serial] {} connect start: {port_name}@{baud_rate}", key);
    crate::services::ssh::emit_status(app, key, STATUS_CONNECTING);

    // 打开串口（8N1 默认，读超时 100ms）
    let port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(READ_TIMEOUT_MS))
        .open()
        .map_err(|e| AppError::general(format!("打开串口 {port_name} 失败: {e}")))?;

    // 读线程持有 try_clone 的句柄，写线程持有原句柄
    let read_port = port
        .try_clone()
        .map_err(|e| AppError::general(format!("克隆串口句柄失败: {e}")))?;

    let alive = Arc::new(AtomicBool::new(true));

    // 读线程：阻塞读取（带超时轮询）→ Channel 透传（阻塞 IO 走独立线程，不占异步运行时）
    let read_alive = alive.clone();
    let thread_app = app.clone();
    let thread_key = key.to_string();
    std::thread::spawn(move || read_loop(thread_app, thread_key, read_port, on_output, read_alive));

    // 写转发线程：消费键盘输入队列
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || write_forward(write_rx, port));

    // 注册会话句柄（断开时清理）
    state.serial_sessions.lock().map_err(lock_err)?.insert(
        key.to_string(),
        SerialSessionHandle { write_tx, alive },
    );
    crate::services::ssh::emit_status(&app, &key, "connected");
    Ok(())
}

/// 断开并清理会话：移除句柄（write_tx drop → 写转发线程退出 → port drop），
/// 并置存活标志为 false，读循环超时轮询命中后随之退出。
pub fn disconnect(state: &AppState, id: &str) {
    let removed = state
        .serial_sessions
        .lock()
        .map(|mut m| m.remove(id))
        .unwrap_or_default();
    if let Some(handle) = removed {
        handle.alive.store(false, Ordering::Relaxed);
    }
}

/// 键盘输入写入（按连接键路由）
pub fn write(state: &AppState, id: &str, data: &[u8]) -> Result<(), AppError> {
    let sessions = state.serial_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::general(format!("串口会话 {id} 不存在或已断开")))?;
    handle
        .write_tx
        .send(data.to_vec())
        .map_err(|_| AppError::general("会话已关闭，无法写入"))
}

/// 会话是否存活：Rust 侧是否持有该会话的句柄（对齐 ssh.rs alive）
pub fn alive(state: &AppState, id: &str) -> bool {
    let sessions = match state.serial_sessions.lock() {
        Ok(s) => s,
        Err(_) => return false,
    };
    sessions.contains_key(id)
}
