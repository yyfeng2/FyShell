//! Telnet 服务：TCP 连接 + 最小 IAC 协商（响应 WILL/WONT/DO/DONT）。
//!
//! 架构与 services/ssh.rs 的"字节流终端"模式对齐，仅传输层不同：
//! - TCP 连接后读写循环在独立线程阻塞执行；
//! - 读侧经 IAC 状态机清洗协商字节后透传给 xterm.js（tauri::ipc::Channel）；
//! - 最小协商策略：拒绝服务器全部 WILL/DO 提议（WONT/DONT 回应），
//!   子协商内容跳过——服务器回退 NVT 默认行为（远端回显），终端仍可正常交互；
//! - 断开时 shutdown 双向关闭 TCP 并同步清理 AppState::telnet_sessions。

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, PoisonError};

use tauri::ipc::Channel;
use tauri::Manager;

use crate::error::AppError;
use crate::state::AppState;

/// 读循环批量发送阈值：单次 read 缓冲 4KB，减少 IPC 次数
const READ_BATCH_BYTES: usize = 4096;

/// session-status 事件的状态字符串（与 ssh.rs 一致）
const STATUS_CONNECTING: &str = "connecting";

/// IAC 协议字节（RFC 854）
const IAC: u8 = 255;
const WILL: u8 = 251;
const WONT: u8 = 252;
const DO: u8 = 253;
const DONT: u8 = 254;
const SB: u8 = 250;
const SE: u8 = 240;

/// std::sync::Mutex 中毒时的统一错误转换
fn lock_err<T>(_e: PoisonError<T>) -> AppError {
    AppError::general("全局状态锁被污染")
}

// ---------------------------------------------------------------------------
// 会话句柄
// ---------------------------------------------------------------------------

/// 单个 Telnet 会话句柄：持有 TCP 流与键盘输入队列，
/// 提供 write / close 能力。存于 `AppState::telnet_sessions`。
#[derive(Clone)]
pub struct TelnetSessionHandle {
    /// 键盘输入队列，由独立写转发线程消费
    write_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// TCP 流句柄：断开时 shutdown 双向关闭（读写线程的阻塞 IO 随之报错退出）
    stream: Arc<TcpStream>,
}

// ---------------------------------------------------------------------------
// IAC 状态机：最小协商实现
// ---------------------------------------------------------------------------

/// IAC 协商状态机状态
#[derive(PartialEq)]
enum IacState {
    /// 普通数据透传
    Normal,
    /// 已收到 IAC，等待命令字节
    AwaitCommand,
    /// WILL/WONT/DO/DONT 后等待选项字节（携带命令字节）
    AwaitOption(u8),
    /// 子协商内容（SB...SE，直接跳过）
    Subnegotiation,
}

/// 处理单个字节：返回需要回应服务器的协商响应（最长 3 字节，无需回应时 None）
fn iac_handle(state: &mut IacState, byte: u8) -> Option<[u8; 3]> {
    match state {
        IacState::Normal => {
            if byte == IAC {
                *state = IacState::AwaitCommand;
            }
            None
        }
        IacState::AwaitCommand => {
            match byte {
                IAC => {
                    // IAC IAC：转义的字面 0xFF 字节（输出侧由调用方处理）
                    *state = IacState::Normal;
                    Some([IAC, 0, 0])
                }
                WILL | WONT | DO | DONT => {
                    *state = IacState::AwaitOption(byte);
                    None
                }
                SB => {
                    // 子协商：跳过内容直到 SE
                    *state = IacState::Subnegotiation;
                    None
                }
                // 其他两字节命令（NOP/AYT 等）直接忽略
                _ => {
                    *state = IacState::Normal;
                    None
                }
            }
        }
        IacState::AwaitOption(cmd) => {
            let cmd = *cmd;
            // 最小实现：拒绝全部选项提议（服务器回退 NVT 默认行为，远端回显）
            let reply = match cmd {
                WILL => Some([IAC, DONT, byte]),
                DO => Some([IAC, WONT, byte]),
                // WONT/DONT 无需响应
                _ => None,
            };
            *state = IacState::Normal;
            reply
        }
        IacState::Subnegotiation => {
            // 子协商内容不透传；IAC 后跟 SE 表示结束，其余按转义跳过
            if byte == SE {
                *state = IacState::Normal;
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 读循环 / 写转发
// ---------------------------------------------------------------------------

/// 读循环：阻塞读取 TCP 字节流 → IAC 状态机解析 → `Channel<Vec<u8>>` send。
/// 单次 read 缓冲 4KB 后批量发送；IAC 序列跨批量边界由状态机正确衔接。
/// EOF（对端关闭）或读错误时推送断开状态。
fn read_loop(
    app: tauri::AppHandle,
    session_id: String,
    mut stream: TcpStream,
    on_output: Channel<Vec<u8>>,
) {
    let mut buf = vec![0u8; READ_BATCH_BYTES];
    let mut pending: Vec<u8> = Vec::new(); // 清洗协商字节后的净数据
    let mut state = IacState::Normal;
    let mut first_chunk = true;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break, // EOF：对端关闭
            Ok(n) => {
                for &byte in &buf[..n] {
                    match iac_handle(&mut state, byte) {
                        // IAC IAC 转义：输出字面 0xFF
                        Some([IAC, 0, 0]) => pending.push(IAC),
                        // 协商回应立即写回服务器（WONT x / DONT x）
                        Some(reply) => {
                            let _ = stream.write_all(&reply);
                        }
                        // 普通数据：追加到净数据缓冲
                        None => {
                            if state == IacState::Normal {
                                pending.push(byte);
                            }
                        }
                    }
                }
                // 冲刷净数据（每个 read 批量后发送一次）
                if !pending.is_empty() {
                    if first_chunk {
                        first_chunk = false;
                        eprintln!(
                            "[telnet] {} first output chunk: {} bytes",
                            session_id,
                            pending.len()
                        );
                    }
                    let chunk = std::mem::take(&mut pending);
                    crate::services::session_log::write_log(&session_id, &chunk);
                    if on_output.send(chunk).is_err() {
                        break;
                    }
                }
            }
            // 信号中断：直接重试
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("[telnet] {} read error: {e}", session_id);
                break;
            }
        }
    }
    // 断开：推送 disconnected 状态并同步清理句柄（对齐 ssh.rs Handler::disconnected）
    crate::services::ssh::emit_status(&app, &session_id, "disconnected");
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut sessions) = state.telnet_sessions.lock() {
            sessions.remove(&session_id);
        }
    }
}

/// 写转发线程：消费键盘输入队列，写入 TCP 流
fn write_forward(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    mut stream: TcpStream,
) {
    while let Some(data) = rx.blocking_recv() {
        if stream.write_all(&data).is_err() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 建立 Telnet 连接：TCP 连接 → IAC 协商读循环。
/// 输出走 tauri::ipc::Channel，状态通过 session-status 事件推送。
/// key：连接路由键（Telnet 连接无持久会话，每标签唯一，由前端生成）。
pub async fn connect(
    app: &tauri::AppHandle,
    state: &AppState,
    key: &str,
    host: &str,
    port: u16,
    on_output: tauri::ipc::Channel<Vec<u8>>,
) -> Result<(), AppError> {
    // 防止重复连接：同键已存在时先断开旧会话再完整重建（对齐 ssh.rs）
    {
        let existing = state.telnet_sessions.lock().map_err(lock_err)?.remove(key);
        if existing.is_some() {
            eprintln!("[telnet] {} already connected, rebuilding session", key);
        }
    }
    eprintln!("[telnet] {} connect start: {host}:{port}", key);
    crate::services::ssh::emit_status(app, key, STATUS_CONNECTING);

    // TCP 连接（tokio 连接保持异步语义，再转 std 流交给阻塞线程）
    let std_stream = tokio::net::TcpStream::connect((host, port))
        .await
        .map_err(|e| AppError::general(format!("连接 {host}:{port} 失败: {e}")))?
        .into_std()
        .map_err(|e| AppError::Io(e))?;
    let stream = Arc::new(std_stream);

    // 读线程：阻塞读取 → IAC 解析 → Channel 透传（阻塞 IO 走独立线程，不占异步运行时）
    let read_stream = stream.try_clone()?;
    let thread_app = app.clone();
    let thread_key = key.to_string();
    std::thread::spawn(move || read_loop(thread_app, thread_key, read_stream, on_output));

    // 写转发线程：消费键盘输入队列
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel();
    let write_stream = stream.try_clone()?;
    std::thread::spawn(move || write_forward(write_rx, write_stream));

    // 注册会话句柄（断开时清理）
    state
        .telnet_sessions
        .lock()
        .map_err(lock_err)?
        .insert(key.to_string(), TelnetSessionHandle { write_tx, stream });
    crate::services::ssh::emit_status(&app, &key, "connected");
    Ok(())
}

/// 断开并清理会话：shutdown 双向关闭 TCP（读写线程的阻塞 IO 报错退出）。
pub fn disconnect(state: &AppState, id: &str) {
    let removed = state
        .telnet_sessions
        .lock()
        .map(|mut m| m.remove(id))
        .unwrap_or_default();
    if let Some(handle) = removed {
        let _ = handle.stream.shutdown(Shutdown::Both);
    }
}

/// 键盘输入写入（按连接键路由）
pub fn write(state: &AppState, id: &str, data: &[u8]) -> Result<(), AppError> {
    let sessions = state.telnet_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::general(format!("Telnet 会话 {id} 不存在或已断开")))?;
    handle
        .write_tx
        .send(data.to_vec())
        .map_err(|_| AppError::general("会话已关闭，无法写入"))
}

/// 会话是否存活：Rust 侧是否持有该会话的句柄（对齐 ssh.rs alive）
pub fn alive(state: &AppState, id: &str) -> bool {
    let sessions = match state.telnet_sessions.lock() {
        Ok(s) => s,
        Err(_) => return false,
    };
    sessions.contains_key(id)
}
