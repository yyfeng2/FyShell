//! 本地终端服务：基于 portable-pty（wezterm 系）的本地 shell PTY 封装。
//!
//! Windows 使用 ConPTY（伪控制台），macOS/Linux 使用原生 PTY，仅传输层不同：
//! - spawn 本地 shell（Windows 用 PowerShell，类 Unix 优先 $SHELL 回退 /bin/sh）；
//! - PTY 读循环在独立线程阻塞读取（单次 ≤4KB 批量），经 tauri::ipc::Channel 透传，
//!   ANSI 解析交给 xterm.js（与 services/ssh.rs 的读循环模式对齐）；
//! - 连接状态通过 `session-status` 事件推送；
//! - 断开时同步清理 AppState::local_sessions 中的句柄并 kill 子进程。

use std::io::{Read, Write};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::error::AppError;
use crate::state::AppState;

/// 默认终端尺寸（前端 resize 后更新）
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

/// 读循环批量发送阈值：单次 read 缓冲 4KB，减少 IPC 次数
const READ_BATCH_BYTES: usize = 4096;

/// session-status 事件的状态字符串（与 ssh.rs 一致）
const STATUS_DISCONNECTED: &str = "disconnected";

/// std::sync::Mutex 中毒时的统一错误转换
fn lock_err<T>(_e: PoisonError<T>) -> AppError {
    AppError::general("全局状态锁被污染")
}

// ---------------------------------------------------------------------------
// 会话句柄
// ---------------------------------------------------------------------------

/// 写转发任务的消息：键盘输入或终端 resize
pub enum StreamWriteMsg {
    /// 键盘输入字节流
    Data(Vec<u8>),
    /// 终端尺寸变更（pix 尺寸固定 0）
    Resize { cols: u16, rows: u16 },
}

/// 单个本地终端会话句柄：持有 shell 子进程句柄与写入队列，
/// 提供 write / resize / close 能力。存于 `AppState::local_sessions`。
/// 可 Clone（内部字段用 Arc 包裹），供传输任务跨线程共享。
#[derive(Clone)]
pub struct LocalShellHandle {
    /// 键盘输入 / resize 消息队列，由独立写转发线程消费
    write_tx: tokio::sync::mpsc::UnboundedSender<StreamWriteMsg>,
    /// shell 子进程句柄：断开时显式 kill（master drop 关闭 ConPTY 后子进程同样终止，双保险）
    child: Arc<std::sync::Mutex<Box<dyn portable_pty::Child + Send>>>,
}

impl LocalShellHandle {
    /// 发送写转发消息（内部使用）
    fn send_msg(&self, msg: StreamWriteMsg) -> Result<(), AppError> {
        self.write_tx
            .send(msg)
            .map_err(|_| AppError::general("会话已关闭，无法写入"))
    }
}

// ---------------------------------------------------------------------------
// 读循环 / 写转发（独立线程：portable-pty 的读写端为阻塞 IO，无法安全跨入异步运行时）
// ---------------------------------------------------------------------------

/// 读循环：阻塞读取 PTY 输出 → `Channel<Vec<u8>>` send。
/// 单次 read 缓冲 4KB 后立即发送（阻塞读无法实现限时冲刷，
/// 但单次批量上限与 ssh.rs 的 4KB 批量一致，交互延迟更低）。
/// ANSI 解析交给 xterm.js；EOF（shell 退出）或读错误时推送断开状态。
fn read_loop(
    app: tauri::AppHandle,
    session_id: String,
    mut reader: Box<dyn Read + Send>,
    on_output: Channel<Vec<u8>>,
) {
    let mut buf = vec![0u8; READ_BATCH_BYTES];
    let mut first_chunk = true;
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break, // EOF：shell 退出或 PTY 关闭
            Ok(n) => {
                if first_chunk {
                    first_chunk = false;
                    eprintln!("[local] {} first output chunk: {} bytes", session_id, n);
                }
                // 会话日志开启时同步追加落盘（未启用时 write_log 立即返回）
                crate::services::session_log::write_log(&session_id, &buf[..n]);
                if on_output.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            // 信号中断：直接重试
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[local] {} read error: {e}", session_id);
                break;
            }
        }
    }
    // 断开：推送 disconnected 状态并同步清理句柄（对齐 ssh.rs Handler::disconnected）
    crate::services::ssh::emit_status(&app, &session_id, STATUS_DISCONNECTED);
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut sessions) = state.local_sessions.lock() {
            sessions.remove(&session_id);
        }
    }
}

/// 写转发线程：消费键盘输入 / resize 消息队列，写入 PTY master。
/// 全部 Sender drop（会话关闭）后线程退出，master drop 关闭 PTY。
fn write_forward(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<StreamWriteMsg>,
    mut writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
) {
    while let Some(msg) = rx.blocking_recv() {
        match msg {
            StreamWriteMsg::Data(data) => {
                if writer.write_all(&data).is_err() {
                    break;
                }
            }
            StreamWriteMsg::Resize { cols, rows } => {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 按平台构建本地 shell 启动命令：Windows 用 PowerShell（内置 ConPTY 支持），
/// 类 Unix 优先取 $SHELL 回退 /bin/sh。
fn build_shell_command() -> CommandBuilder {
    if cfg!(windows) {
        CommandBuilder::new("powershell.exe")
    } else {
        CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()))
    }
}

/// 建立本地终端连接：打开 PTY → spawn 本地 shell → 读循环。
/// 输出走 tauri::ipc::Channel（4KB 批量），状态通过 session-status 事件推送。
/// key：连接路由键（本地终端无持久会话，每标签唯一，由前端生成）。
pub async fn connect(
    app: &tauri::AppHandle,
    state: &AppState,
    key: &str,
    on_output: tauri::ipc::Channel<Vec<u8>>,
) -> Result<(), AppError> {
    // 防止重复连接：同键已存在时先断开旧会话再完整重建
    // （场景：HMR/页面重载后重连时，旧输出 Channel 已随旧 JS 上下文失效，对齐 ssh.rs）
    {
        let existing = state.local_sessions.lock().map_err(lock_err)?.remove(key);
        if existing.is_some() {
            eprintln!("[local] {} already connected, rebuilding session", key);
        }
    }
    eprintln!("[local] {} connect start", key);

    // 打开 PTY（默认 80x24，前端 resize 后更新）
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| AppError::general(format!("打开本地 PTY 失败: {e}")))?;

    // spawn 本地 shell
    let child = pair
        .slave
        .spawn_command(build_shell_command())
        .map_err(|e| AppError::general(format!("启动本地 shell 失败: {e}")))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::general(format!("获取 PTY 读端失败: {e}")))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::general(format!("获取 PTY 写端失败: {e}")))?;

    // 读线程：阻塞读取 → Channel 透传（阻塞 IO 走独立线程，不占异步运行时）
    let thread_app = app.clone();
    let thread_key = key.to_string();
    std::thread::spawn(move || read_loop(thread_app, thread_key, reader, on_output));

    // 写转发线程：消费键盘输入 / resize 消息（master 一并由写线程持有，resize 用）
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || write_forward(write_rx, writer, pair.master));

    // 注册会话句柄（断开时清理）
    state.local_sessions.lock().map_err(lock_err)?.insert(
        key.to_string(),
        LocalShellHandle {
            write_tx,
            child: Arc::new(std::sync::Mutex::new(child)),
        },
    );
    crate::services::ssh::emit_status(&app, &key, "connected");
    Ok(())
}

/// 断开并清理会话：从 local_sessions 移除句柄（write_tx drop → 写转发线程退出
/// → master drop 关闭 PTY），并显式 kill 子进程避免残留后台 shell。
pub fn disconnect(state: &AppState, id: &str) {
    let removed = state
        .local_sessions
        .lock()
        .map(|mut m| m.remove(id))
        .unwrap_or_default();
    // 确保子进程一定终止：kill 后轮询 try_wait（短眠重试），否则 kill 可能因
    // 竞态未生效（进程恰好已退出但状态未回收、或 ConPTY 下 kill 未命中真实进程）。
    // 轮询之余 write_tx drop → 写转发线程退出 → master drop 关闭 ConPTY，
    // Pty 内带进来的子进程同样终止，双保险下 5 次内必然退出
    if let Some(handle) = removed {
        if let Ok(mut child) = handle.child.lock() {
            for _ in 0..5 {
                match child.try_wait() {
                    // 已退出（或 wait 出错）：终止完成
                    Ok(Some(_)) | Err(_) => break,
                    Ok(None) => {
                        let _ = child.kill();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            let _ = child.kill();
        }
    }
}

/// 键盘输入写入（按连接键路由）
pub fn write(state: &AppState, id: &str, data: &[u8]) -> Result<(), AppError> {
    let sessions = state.local_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::general(format!("本地终端会话 {id} 不存在或已断开")))?;
    handle.send_msg(StreamWriteMsg::Data(data.to_vec()))
}

/// 终端尺寸变更（按连接键路由 resize）
pub fn resize(state: &AppState, id: &str, cols: u16, rows: u16) -> Result<(), AppError> {
    let sessions = state.local_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::general(format!("本地终端会话 {id} 不存在或已断开")))?;
    handle.send_msg(StreamWriteMsg::Resize { cols, rows })
}

/// 会话是否存活：Rust 侧是否持有该会话的句柄（对齐 ssh.rs alive）
pub fn alive(state: &AppState, id: &str) -> bool {
    let sessions = match state.local_sessions.lock() {
        Ok(s) => s,
        Err(_) => return false, // 锁被污染/占用时按不存活处理，前端会走重连
    };
    sessions.contains_key(id)
}
