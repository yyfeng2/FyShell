//! 终端 ZMODEM 传输服务：SSH 终端内 rz/sz 命令的协议拦截与本地文件收发。
//!
//! 架构：
//! - 读循环（services/ssh.rs read_loop）在输出流中检测 ZRQINIT 哨兵
//!   （`**\x18B`）后调用 [`start`]：注册会话条目、spawn_blocking 跑任务、
//!   emit `zmodem-start` 事件；
//! - 前端弹对话框（接收文件 / 发送文件 / 取消），用户选择后命令层调用
//!   `zmodem_respond`，选择经响应通道回传任务；
//! - 任务以 zmodem2 crate 的调用方驱动状态机完成协议握手：
//!   `Action::WriteWire` 经既有写转发路径回传，读循环输出经管道喂入
//!   `submit_wire`，文件读写直接 std::fs；
//! - 结束（完成/失败/取消）时移除 AppState 条目（读循环恢复常规转发），
//!   emit `zmodem-end` 事件。
//!
//! 状态机 crate 为纯同步实现，任务整体跑在 blocking 线程：
//! 管道 `recv_timeout` 阻塞等待、std::fs 文件 I/O、tokio UnboundedSender::send
//! 本身同步，无 async 原语阻塞风险。

use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use zmodem2::{Action, Event, Receiver, Sender};

use crate::state::AppState;

/// 等待前端选择（对话框）的兜底超时：超时自动取消，避免终端悬空
const CHOICE_TIMEOUT: Duration = Duration::from_secs(60);

/// 管道等待超时：超时驱动状态机 timeout()（内置协议重试）
const PIPE_POLL_TIMEOUT: Duration = Duration::from_secs(1);

/// 连续无数据的管道等待次数上限：超过判定链路失效（会话已断），
/// 中止任务防止悬挂
const MAX_IDLE_POLLS: u32 = 30;

/// 进度推送节流间隔
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// 中止序列：连续 8 个 CAN（0x18），lrzsz 视为放弃传输
const CAN_ABORT: [u8; 8] = [0x18; 8];

/// 前端选择（对话框三选一）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub enum ZmodemChoice {
    /// 接收文件（对端 sz）：dir 为本地保存目录
    Recv { dir: String },
    /// 发送文件（对端 rz）：path 为本地文件路径
    Send { path: String },
    /// 取消
    Cancel,
}

/// 会话进行中的 ZMODEM 状态（存 AppState::zmodem_sessions）
pub struct ZmodemEntry {
    /// 读循环 → zmodem 任务的数据管道
    pub pipe_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// 前端选择回传（zmodem_respond 命令调用）
    pub respond_tx: std::sync::mpsc::Sender<ZmodemChoice>,
}

/// zmodem-start 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct ZmodemStartEvent {
    pub key: String,
}

/// zmodem-progress 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct ZmodemProgressEvent {
    pub key: String,
    pub file_name: String,
    pub transferred: u64,
    pub total: u64,
}

/// zmodem-end 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct ZmodemEndEvent {
    pub key: String,
    pub ok: bool,
    pub message: String,
}

/// 启动 ZMODEM 传输任务（read_loop 检测到 ZRQINIT 哨兵时调用）。
/// `initial`：哨兵起的字节（含 ZRQINIT 帧），经管道作为任务的首批输入。
/// 同 key 已有任务在跑时覆盖旧条目（正常不会发生：改道后读循环不再检测哨兵）。
pub fn start(
    app: &AppHandle,
    state: &AppState,
    key: &str,
    initial: Vec<u8>,
    write_tx: tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
) {
    let (pipe_tx, pipe_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    // 哨兵起的字节（含 ZRQINIT 帧）作为任务的首批输入
    if !initial.is_empty() {
        let _ = pipe_tx.send(initial);
    }
    let (respond_tx, respond_rx) = std::sync::mpsc::channel::<ZmodemChoice>();

    state
        .zmodem_sessions
        .lock()
        .expect("zmodem_sessions 锁被污染")
        .insert(
            key.to_string(),
            ZmodemEntry {
                pipe_tx,
                respond_tx,
            },
        );

    let task_app = app.clone();
    let task_key = key.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        run(task_app, task_key, pipe_rx, respond_rx, write_tx);
    });

    let _ = app.emit("zmodem-start", ZmodemStartEvent { key: key.to_string() });
}

/// 任务主流程：等前端选择 → 状态机收发文件 → 结束清理
fn run(
    app: AppHandle,
    key: String,
    pipe_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    respond_rx: std::sync::mpsc::Receiver<ZmodemChoice>,
    write_tx: tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
) {
    eprintln!("[zmodem] {key} started, waiting for user choice");
    let choice = match respond_rx.recv_timeout(CHOICE_TIMEOUT) {
        Ok(choice) => choice,
        // 超时/通道关闭（会话断开清理）→ 视作取消
        Err(_) => ZmodemChoice::Cancel,
    };
    eprintln!("[zmodem] {key} choice: {choice:?}");

    let result = match choice {
        ZmodemChoice::Recv { dir } => run_recv(&app, &key, &pipe_rx, &write_tx, &dir),
        ZmodemChoice::Send { path } => run_send(&app, &key, &pipe_rx, &write_tx, &path),
        ZmodemChoice::Cancel => {
            // 用户取消：发中止序列，避免远端 sz/rz 继续重试刷出乱码
            write_to_wire(&write_tx, &CAN_ABORT);
            Ok(())
        }
    };
    // 失败路径补发中止序列，避免远端 sz/rz 继续重试刷出乱码
    if result.is_err() {
        write_to_wire(&write_tx, &CAN_ABORT);
    }

    // 终态：移除条目（读循环恢复常规转发）+ emit zmodem-end
    app.state::<AppState>()
        .zmodem_sessions
        .lock()
        .expect("zmodem_sessions 锁被污染")
        .remove(&key);
    let (ok, message) = match &result {
        Ok(()) => (true, String::new()),
        Err(e) => (false, e.to_string()),
    };
    eprintln!("[zmodem] {key} ended: ok={ok} {message}");
    let _ = app.emit("zmodem-end", ZmodemEndEvent { key, ok, message });
}

/// 接收文件（对端 sz）：Receiver 状态机收 ZFILE/ZDATA 落盘到 dir/<服务端文件名>
fn run_recv(
    app: &AppHandle,
    key: &str,
    pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    write_tx: &tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
    dir: &str,
) -> Result<(), String> {
    // buffer_len=0 + CANOVIO：向远端声明非停等 I/O，SSH 可靠流上连续流式传输
    let mut receiver = Receiver::with_flow_control(0, true)
        .map_err(|e| format!("初始化 ZMODEM 接收器失败: {e}"))?;
    // 自动接受模式（默认）：ZFILE 元数据解析后立即从偏移 0 接收，
    // 目录已由前端对话框选定；sz 多文件时逐个接收

    let dir_path = PathBuf::from(dir);
    let mut file: Option<std::fs::File> = None;
    let mut file_name = String::new();
    let mut total: u64 = 0;
    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    let mut idle_polls: u32 = 0;

    loop {
        // 状态机动作处理到 Idle 为止
        loop {
            match receiver.poll() {
                Action::WriteWire(bytes) => {
                    let n = bytes.len();
                    write_to_wire(write_tx, bytes);
                    receiver.wire_written(n);
                }
                Action::Event(Event::FileStarted(info)) => {
                    let name = String::from_utf8_lossy(info.name).into_owned();
                    file_name = sanitized_file_name(&name)?;
                    total = info.size.map(|s| u64::from(s.get())).unwrap_or(0);
                    transferred = 0;
                    let path = dir_path.join(&file_name);
                    file = Some(
                        std::fs::File::create(&path)
                            .map_err(|e| format!("创建本地文件失败: {e}"))?,
                    );
                    eprintln!("[zmodem] {key} recv file: {file_name} ({total} bytes)");
                    let _ = app.emit(
                        "zmodem-progress",
                        ZmodemProgressEvent {
                            key: key.to_string(),
                            file_name: file_name.clone(),
                            transferred,
                            total,
                        },
                    );
                    last_emit = Instant::now();
                }
                Action::WriteFile(data) => {
                    let n = data.len();
                    if let Some(f) = file.as_mut() {
                        if let Err(e) = f.write_all(data) {
                            return Err(format!("写入本地文件失败: {e}"));
                        }
                    } else {
                        return Err("尚未开始接收文件".into());
                    }
                    transferred += n as u64;
                    emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                    receiver
                        .file_written(n)
                        .map_err(|e| format!("ZMODEM 协议错误: {e}"))?;
                }
                Action::Event(event) => match event {
                    Event::FileCompleted => {
                        // 单文件完成：终值进度兜底推送
                        emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                    }
                    Event::SessionCompleted => return Ok(()),
                    Event::Aborted => return Err("ZMODEM 传输已中止".into()),
                    // 其余协议事件无需处理
                    _ => {}
                },
                Action::ReadFile { .. } => {}
                Action::Idle => break,
                // Action 为 non_exhaustive 枚举，预留通配
                _ => {}
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                if !data.is_empty() {
                    receiver
                        .submit_wire(&data)
                        .map_err(|e| format!("ZMODEM 协议错误: {e}"))?;
                }
            }
            PipeWait::Timeout => {
                idle_polls += 1;
                if idle_polls >= MAX_IDLE_POLLS {
                    return Err("ZMODEM 传输超时（链路无响应）".into());
                }
                let _ = receiver.timeout();
            }
            PipeWait::Disconnected => return Err("会话已断开".into()),
        }
    }
}

/// 发送文件（对端 rz）：Sender 状态机读本地文件上传
fn run_send(
    app: &AppHandle,
    key: &str,
    pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    write_tx: &tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
    path: &str,
) -> Result<(), String> {
    let mut sender = Sender::new().map_err(|e| format!("初始化 ZMODEM 发送器失败: {e}"))?;
    // SSH 可靠流：非停等流式传输，免每包 ACK 往返
    sender.set_streaming_window(usize::MAX);

    let mut source = std::fs::File::open(path).map_err(|e| format!("读取本地文件失败: {e}"))?;
    let file_name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let total = source.metadata().map(|m| m.len()).unwrap_or(0);
    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    let mut idle_polls: u32 = 0;
    eprintln!("[zmodem] {key} send file: {file_name} ({total} bytes)");
    let _ = app.emit(
        "zmodem-progress",
        ZmodemProgressEvent {
            key: key.to_string(),
            file_name: file_name.clone(),
            transferred,
            total,
        },
    );

    loop {
        // 状态机动作处理到 Idle 为止
        loop {
            match sender.poll() {
                Action::WriteWire(bytes) => {
                    let n = bytes.len();
                    write_to_wire(write_tx, bytes);
                    sender.wire_written(n);
                }
                Action::ReadFile { offset, max_len } => {
                    let mut buf = vec![0u8; max_len.min(16 * 1024)];
                    if let Err(e) = source.seek(std::io::SeekFrom::Start(u64::from(offset.get()))) {
                        return Err(format!("文件 seek 失败: {e}"));
                    }
                    let n = source
                        .read(&mut buf)
                        .map_err(|e| format!("读取文件失败: {e}"))?;
                    if n == 0 {
                        return Err("文件读取提前结束".into());
                    }
                    transferred = u64::from(offset.get()) + n as u64;
                    if let Err(e) = sender.submit_file(&buf[..n]) {
                        return Err(format!("提交文件数据失败: {e}"));
                    }
                    emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                }
                Action::Event(event) => match event {
                    Event::FileCompleted => {
                        emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                    }
                    Event::SessionCompleted => return Ok(()),
                    Event::Aborted => return Err("ZMODEM 传输已中止".into()),
                    // 其余协议事件无需处理
                    _ => {}
                },
                Action::WriteFile(_) => {}
                Action::Idle => break,
                // Action 为 non_exhaustive 枚举，预留通配
                _ => {}
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                if !data.is_empty() {
                    sender
                        .submit_wire(&data)
                        .map_err(|e| format!("ZMODEM 协议错误: {e}"))?;
                }
            }
            PipeWait::Timeout => {
                idle_polls += 1;
                if idle_polls >= MAX_IDLE_POLLS {
                    return Err("ZMODEM 传输超时（链路无响应）".into());
                }
                let _ = sender.timeout();
            }
            PipeWait::Disconnected => return Err("会话已断开".into()),
        }
    }
}

/// 管道等待结果
enum PipeWait {
    Data(Vec<u8>),
    Timeout,
    Disconnected,
}

/// 等待读循环送来的一批输出（超时即 Timeout，管道关闭即 Disconnected）
fn wait_pipe(pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>) -> PipeWait {
    match pipe_rx.recv_timeout(PIPE_POLL_TIMEOUT) {
        Ok(data) => PipeWait::Data(data),
        Err(RecvTimeoutError::Timeout) => PipeWait::Timeout,
        Err(RecvTimeoutError::Disconnected) => PipeWait::Disconnected,
    }
}

/// 经既有写转发路径回传协议字节（与键盘输入同路）
fn write_to_wire(
    write_tx: &tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
    bytes: &[u8],
) {
    let _ = write_tx.send(crate::services::ssh::SshWriteMsg::Data(bytes.to_vec()));
}

/// 进度推送（节流）
fn emit_progress(
    app: &AppHandle,
    key: &str,
    file_name: &str,
    transferred: u64,
    total: u64,
    last_emit: &mut Instant,
) {
    if last_emit.elapsed() >= PROGRESS_INTERVAL {
        *last_emit = Instant::now();
        let _ = app.emit(
            "zmodem-progress",
            ZmodemProgressEvent {
                key: key.to_string(),
                file_name: file_name.to_string(),
                transferred,
                total,
            },
        );
    }
}

/// 服务端文件名合法性：拒绝路径分隔符与相对路径片段（防路径穿越）
fn sanitized_file_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.starts_with('.')
    {
        return Err(format!("文件名不合法: {name}"));
    }
    Ok(trimmed.to_string())
}
