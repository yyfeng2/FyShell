//! 终端 ZMODEM 传输服务：SSH 终端内 rz/sz 命令的协议拦截与本地文件收发。
//!
//! 架构：
//! - 读循环（services/ssh.rs read_loop）在输出流中检测 ZMODEM hex 帧头哨兵
//!   （`**\x18B`）后调用 [`start`]：注册会话条目、spawn_blocking 跑任务；
//! - 任务从哨兵后的帧类型识别方向（ZRQINIT=对端执行了 sz→本地接收；
//!   ZRINIT=对端执行了 rz→本地发送）后 emit `zmodem-start`（含 direction）；
//! - 前端按方向直接弹对应系统选择器（接收=选保存目录 / 发送=选上传文件 /
//!   未知=弹层三选手选），用户选择后命令层调用 `zmodem_respond`，选择经响应
//!   通道回传任务；
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

/// 前端选择（方向识别失败时的三选手选）
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
    /// 识别的传输方向："recv"=对端执行了 sz（前端选保存目录）；
    /// "send"=对端执行了 rz（前端选上传文件）；None=无法识别（前端弹层手选）
    pub direction: Option<String>,
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

/// 启动 ZMODEM 传输任务（read_loop 检测到 ZMODEM hex 帧头哨兵时调用）。
/// `initial`：哨兵起的字节（含首帧头），经管道作为任务的首批输入，任务据此
/// 识别传输方向。同 key 已有任务在跑时覆盖旧条目（正常不会发生：改道后读循环
/// 不再检测哨兵）。
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
    // zmodem-start 由任务在方向识别后 emit（start 立即 emit 时方向尚不可知）
}

/// 传输方向（对端角色）：由哨兵后的首帧类型识别
enum Direction {
    /// 对端执行了 sz（发送方）→ 本地接收
    Recv,
    /// 对端执行了 rz（接收方）→ 本地发送
    Send,
    /// 非传输开始帧（ZFIN/ZFILE 等）——如上一轮取消后对端 lrz 退出时发的
    /// ZFIN 序列会再次触发哨兵，这类任务应静默忽略
    Ignore,
}

/// 方向识别：哨兵（ZPAD ZPAD ZDLE 'B'）后紧跟首帧类型 hex 两位——
/// "00"=ZRQINIT（对端 sz）、"01"=ZRINIT（对端 rz，lrz 启动即发 ZRINIT）。
/// 其他帧类型（ZFIN="08" 等）不是传输开始，返回 Ignore 静默忽略。
/// prelude 以哨兵起始；帧类型字节未随首批到达时等待后续批次（对端会重发
/// 首帧），兜底 5s 仍无法识别返回 None。识别期间截留的字节保留在 prelude
/// 中，稍后须喂入状态机。
fn detect_direction(
    pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    prelude: &mut Vec<u8>,
) -> Option<Direction> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pos) = crate::services::ssh::find_zmodem_sentinel(prelude) {
            let after = &prelude[pos + crate::services::ssh::ZMODEM_SENTINEL.len()..];
            if after.len() >= 2 {
                return match &after[..2] {
                    b"00" => Some(Direction::Recv),
                    b"01" => Some(Direction::Send),
                    _ => Some(Direction::Ignore),
                };
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        match pipe_rx.recv_timeout(PIPE_POLL_TIMEOUT) {
            Ok(data) => prelude.extend_from_slice(&data),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// 任务主流程：识别方向 → 等前端选择 → 状态机收发文件 → 结束清理
fn run(
    app: AppHandle,
    key: String,
    pipe_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    respond_rx: std::sync::mpsc::Receiver<ZmodemChoice>,
    write_tx: tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
) {
    // 方向识别：哨兵后首帧类型区分对端 rz/sz；识别期间截留的字节稍后喂状态机
    let mut prelude = Vec::new();
    let direction = detect_direction(&pipe_rx, &mut prelude);

    // 非传输开始帧（ZFIN/ZFILE 等，如上一轮取消后对端 lrz 的退出序列）：
    // 静默清理条目（读循环恢复常规转发），不弹选择器不 emit 事件
    if matches!(direction, Some(Direction::Ignore)) {
        app.state::<AppState>()
            .zmodem_sessions
            .lock()
            .expect("zmodem_sessions 锁被污染")
            .remove(&key);
        return;
    }

    let direction_str = match &direction {
        Some(Direction::Recv) => Some("recv".to_string()),
        Some(Direction::Send) => Some("send".to_string()),
        Some(Direction::Ignore) | None => None,
    };
    let _ = app.emit(
        "zmodem-start",
        ZmodemStartEvent {
            key: key.clone(),
            direction: direction_str,
        },
    );

    let choice = match respond_rx.recv_timeout(CHOICE_TIMEOUT) {
        Ok(choice) => choice,
        // 超时/通道关闭（会话断开清理）→ 视作取消
        Err(_) => ZmodemChoice::Cancel,
    };

    let result = match choice {
        ZmodemChoice::Recv { dir } => run_recv(&app, &key, &pipe_rx, &write_tx, &dir, prelude),
        ZmodemChoice::Send { path } => run_send(&app, &key, &pipe_rx, &write_tx, &path, prelude),
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
    let _ = app.emit("zmodem-end", ZmodemEndEvent { key, ok, message });
}

/// 接收文件（对端 sz）：Receiver 状态机收 ZFILE/ZDATA 落盘到 dir/<服务端文件名>
fn run_recv(
    app: &AppHandle,
    key: &str,
    pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    write_tx: &tokio::sync::mpsc::UnboundedSender<crate::services::ssh::SshWriteMsg>,
    dir: &str,
    prelude: Vec<u8>,
) -> Result<(), String> {
    // buffer_len=0 + CANOVIO：向远端声明非停等 I/O，SSH 可靠流上连续流式传输
    let mut receiver = Receiver::with_flow_control(0, true)
        .map_err(|e| format!("初始化 ZMODEM 接收器失败: {e}"))?;
    // 自动接受模式（默认）：ZFILE 元数据解析后立即从偏移 0 接收，
    // 目录已由前端对话框选定；sz 多文件时逐个接收

    // 先冲出构造函数排队的 ZRINIT：submit_wire 在 outgoing 非空时被 blocked
    // 挡住直接 break、消耗 0 字节——构造即排队的 ZRINIT 会让 prelude（含对端
    // ZRQINIT）被静默丢弃
    loop {
        match receiver.poll() {
            Action::WriteWire(bytes) => {
                let n = bytes.len();
                write_to_wire(write_tx, bytes);
                receiver.wire_written(n);
            }
            _ => break,
        }
    }
    // 方向识别期间截留的首帧字节（含哨兵帧）喂入状态机；未消费的残余保留
    // 待后续管道数据合并（submit_wire 提前 break 会丢弃同批输入中完整帧
    // 之后的字节）
    let mut prelude = prelude;
    // prelude 帧损坏（CRC 错误）可恢复：对端重试机制会重发握手帧，丢弃本批
    // 继续等待重发即可；其余错误终止传输
    let consumed = match receiver.submit_wire(&prelude) {
        Ok(n) => n,
        Err(e) if is_crc_error(&e) => prelude.len(),
        Err(e) => return Err(format!("ZMODEM 协议错误: {e}")),
    };
    let mut leftover = prelude.split_off(consumed);
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

        // 未消费残余优先喂入（outgoing 已排空；1 字节进展即继续）——完整帧
        // 滞留残余会让对端等待响应自旋
        if !leftover.is_empty() {
            let consumed = receiver
                .submit_wire(&leftover)
                .map_err(|e| format!("ZMODEM 协议错误: {e}"))?;
            if consumed > 0 {
                leftover.drain(..consumed);
                continue;
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                let mut buf = std::mem::take(&mut leftover);
                buf.extend_from_slice(&data);
                match receiver.submit_wire(&buf) {
                    Ok(consumed) => {
                        if consumed < buf.len() {
                            leftover = buf.split_off(consumed);
                        }
                    }
                    // CRC 损坏可恢复：状态机已排队 NAK（或对端重试机制会重发
                    // 损坏帧），本批输入已耗尽，继续等待重发即可
                    Err(e) if is_crc_error(&e) => {}
                    Err(e) => return Err(format!("ZMODEM 协议错误: {e}")),
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
    prelude: Vec<u8>,
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

    // 注册文件元数据：状态机收到 ZRINIT 后自动发 ZFILE（zmodem2 要求
    // 调用方在 WaitReceiverInit/ReadyForFile 阶段显式 start_file，
    // 否则状态机干等、ReadFile 动作永不出现 → 传输饿死超时）
    sender
        .start_file(zmodem2::FileInfo::new(
            file_name.as_bytes(),
            Some(zmodem2::Position::new(u32::try_from(total).unwrap_or(u32::MAX))),
        ))
        .map_err(|e| format!("注册 ZMODEM 文件失败: {e}"))?;
    // 单文件传输：当前文件完成后发 ZFIN 结束会话
    sender
        .finish()
        .map_err(|e| format!("注册 ZMODEM 结束请求失败: {e}"))?;

    // 先冲出构造函数排队的 ZRQINIT：submit_wire 在 outgoing 非空时于循环开头
    // 直接 break、消耗 0 字节——构造即排队的 ZRQINIT 会让 prelude 的 ZRINIT
    // 被静默丢弃（数据丢失），握手也多一轮往返
    loop {
        match sender.poll() {
            Action::WriteWire(bytes) => {
                let n = bytes.len();
                write_to_wire(write_tx, bytes);
                sender.wire_written(n);
            }
            _ => break,
        }
    }
    // 方向识别期间截留的首帧字节（对端 rz 启动即发的 ZRINIT）喂入状态机；
    // 未消费的残余保留待后续管道数据合并（submit_wire 提前 break 会丢弃
    // 同批输入中完整帧之后的字节）
    let mut prelude = prelude;
    // prelude 帧损坏（CRC 错误）可恢复：对端重试机制会重发握手帧，丢弃本批
    // 继续等待重发即可；其余错误终止传输
    let consumed = match sender.submit_wire(&prelude) {
        Ok(n) => n,
        Err(e) if is_crc_error(&e) => prelude.len(),
        Err(e) => return Err(format!("ZMODEM 协议错误: {e}")),
    };
    let mut leftover = prelude.split_off(consumed);

    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    let mut idle_polls: u32 = 0;
    // 对端 ZSKIP 跳过文件标记（文件非空但 transferred==0 时置位）
    let mut skipped = false;
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
                        // 文件非空但 transferred==0：对端 ZSKIP 跳过了该文件
                        //（远端同名文件已存在且 rz 默认不覆盖）。finish() 在
                        // ReadyForFile 状态立即排队 ZFIN 干净结束会话，否则
                        // 状态机停在 ReadyForFile、后续 ZRINIT 全部被忽略 →
                        // 0 B 卡死直至取消
                        if transferred == 0 && total > 0 {
                            skipped = true;
                            let _ = sender.finish();
                        }
                    }
                    Event::SessionCompleted => {
                        if skipped {
                            return Err(
                                "对端跳过了该文件：同名文件已存在（远端 rz 默认不覆盖），可删除远端文件或让对端以 rz -y 运行后重试"
                                    .into(),
                            );
                        }
                        return Ok(());
                    }
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

        // 未消费残余优先喂入（outgoing 已排空；1 字节进展即继续）——完整帧
        // 滞留残余会让对端等待响应自旋
        if !leftover.is_empty() {
            let consumed = sender
                .submit_wire(&leftover)
                .map_err(|e| format!("ZMODEM 协议错误: {e}"))?;
            if consumed > 0 {
                leftover.drain(..consumed);
                continue;
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                let mut buf = std::mem::take(&mut leftover);
                buf.extend_from_slice(&data);
                match sender.submit_wire(&buf) {
                    Ok(consumed) => {
                        if consumed < buf.len() {
                            leftover = buf.split_off(consumed);
                        }
                    }
                    // CRC 损坏可恢复：状态机已排队 NAK（或对端重试机制会重发
                    // 损坏帧），本批输入已耗尽，继续等待重发即可
                    Err(e) if is_crc_error(&e) => {}
                    Err(e) => return Err(format!("ZMODEM 协议错误: {e}")),
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

/// CRC 校验错误可恢复：NAK 或对端重试机制会重发损坏帧，无需终止传输
fn is_crc_error(e: &zmodem2::Error) -> bool {
    matches!(
        e,
        zmodem2::Error::UnexpectedCrc16 | zmodem2::Error::UnexpectedCrc32
    )
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
