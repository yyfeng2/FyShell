//! 终端 rz/sz 传输服务：SSH 终端内 rz/sz 命令的协议拦截与本地文件收发。
//!
//! 架构：
//! - 读循环（services/ssh.rs read_loop）在输出流中检测 rz/sz hex 帧头哨兵
//!   （`**\x18B`）后调用 [`start`]：注册会话条目、spawn_blocking 跑任务；
//! - 任务从哨兵后的帧类型识别方向（ZRQINIT=对端执行了 sz→本地接收；
//!   ZRINIT=对端执行了 rz→本地发送）后 emit `rzsz-start`（含 direction）；
//! - 前端按方向直接弹对应系统选择器（接收=选保存目录 / 发送=选上传文件 /
//!   未知=弹层三选手选），用户选择后命令层调用 `rzsz_respond`，选择经响应
//!   通道回传任务；
//! - 任务以自研 rzsz_protocol 模块的调用方驱动状态机完成协议握手：
//!   `Action::WriteWire` 经既有写转发路径回传，读循环输出经管道喂入
//!   `submit_wire`，文件读写直接 std::fs；
//! - 结束（完成/失败/取消）时移除 AppState 条目（读循环恢复常规转发），
//!   emit `rzsz-end` 事件。
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
use crate::services::rzsz_protocol;
use crate::services::rzsz_protocol::{Action, Event, Receiver, Sender};

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

/// leftover 容量上限：状态机无法消费的控制字节（数据流掺入非预期字节、帧损坏等）
/// 持续累积时的内存上限，超限丢弃最老字节（协议重发机制会重新对齐）
const LEFTOVER_MAX: usize = 64 * 1024;

/// 帧级诊断日志：定位与真实 lrzsz 互通的握手问题。写临时目录 fyshell-rzsz.log
/// （不经终端/管道，避免污染线协议）。仅传输期间产出，体积极小。
fn diag(msg: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("fyshell-rzsz.log"))
    {
        let _ = writeln!(f, "#{n} {ts} {msg}");
    }
}

/// 字节序列 hex 表示（截断到 max 字节，诊断日志用）
fn hex_head(bytes: &[u8], max: usize) -> String {
    let mut s = String::new();
    for (i, b) in bytes.iter().take(max).enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02X}"));
    }
    if bytes.len() > max {
        s.push_str(&format!(" …(+{})", bytes.len() - max));
    }
    s
}

/// 限制 leftover 容量：超出上限丢弃最老字节并告警，防止无界增长导致内存膨胀。
/// 丢弃方向取最老（先入）字节——状态机始终在头部找帧同步，尾部待消费数据保留。
fn cap_leftover(leftover: &mut Vec<u8>) {
    let overflow = leftover.len().saturating_sub(LEFTOVER_MAX);
    if overflow > 0 {
        eprintln!("[rzsz] leftover 超限：丢弃最老 {overflow} 字节（协议重发对齐）");
        leftover.drain(..overflow);
    }
}

/// 前端选择（方向识别失败时的三选手选）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub enum RzszChoice {
    /// 接收文件（对端 sz）：dir 为本地保存目录
    Recv { dir: String },
    /// 发送文件（对端 rz）：path 为本地文件路径
    Send { path: String },
    /// 取消
    Cancel,
}

/// 会话进行中的 rz/sz 状态（存 AppState::rzsz_sessions）
pub struct RzszEntry {
    /// 读循环 → rzsz 任务的数据管道
    pub pipe_tx: std::sync::mpsc::Sender<Vec<u8>>,
    /// 前端选择回传（rzsz_respond 命令调用）
    pub respond_tx: std::sync::mpsc::Sender<RzszChoice>,
}

/// rzsz-start 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct RzszStartEvent {
    pub key: String,
    /// 识别的传输方向："recv"=对端执行了 sz（前端选保存目录）；
    /// "send"=对端执行了 rz（前端选上传文件）；None=无法识别（前端弹层手选）
    pub direction: Option<String>,
}

/// rzsz-progress 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct RzszProgressEvent {
    pub key: String,
    pub file_name: String,
    pub transferred: u64,
    pub total: u64,
}

/// rzsz-end 事件 payload
#[derive(Clone, Serialize, specta::Type)]
pub struct RzszEndEvent {
    pub key: String,
    pub ok: bool,
    pub message: String,
}

/// 启动 rz/sz 传输任务（read_loop 检测到 rz/sz hex 帧头哨兵时调用）。
/// `initial`：哨兵起的字节（含首帧头），经管道作为任务的首批输入，任务据此
/// 识别传输方向。同 key 已有任务在跑时覆盖旧条目（正常不会发生：改道后读循环
/// 不再检测哨兵）。
pub fn start(
    app: &AppHandle,
    state: &AppState,
    key: &str,
    initial: Vec<u8>,
    write_tx: tokio::sync::mpsc::Sender<crate::services::ssh::SshWriteMsg>,
) {
    let (pipe_tx, pipe_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    // 哨兵起的字节（含 ZRQINIT 帧）作为任务的首批输入
    if !initial.is_empty() {
        let _ = pipe_tx.send(initial);
    }
    let (respond_tx, respond_rx) = std::sync::mpsc::channel::<RzszChoice>();

    state
        .rzsz_sessions
        .lock()
        .expect("rzsz_sessions 锁被污染")
        .insert(
            key.to_string(),
            RzszEntry {
                pipe_tx,
                respond_tx,
            },
        );

    let task_app = app.clone();
    let task_key = key.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        run(task_app, task_key, pipe_rx, respond_rx, write_tx);
    });
    // rzsz-start 由任务在方向识别后 emit（start 立即 emit 时方向尚不可知）
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
        if let Some(pos) = crate::services::ssh::find_rzsz_sentinel(prelude) {
            let after = &prelude[pos + crate::services::ssh::RZSZ_SENTINEL.len()..];
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
    respond_rx: std::sync::mpsc::Receiver<RzszChoice>,
    write_tx: tokio::sync::mpsc::Sender<crate::services::ssh::SshWriteMsg>,
) {
    // 方向识别：哨兵后首帧类型区分对端 rz/sz；识别期间截留的字节稍后喂状态机
    let mut prelude = Vec::new();
    let direction = detect_direction(&pipe_rx, &mut prelude);

    // 非传输开始帧（ZFIN/ZFILE 等，如上一轮取消后对端 lrz 的退出序列）：
    // 静默清理条目（读循环恢复常规转发），不弹选择器不 emit 事件
    if matches!(direction, Some(Direction::Ignore)) {
        app.state::<AppState>()
            .rzsz_sessions
            .lock()
            .expect("rzsz_sessions 锁被污染")
            .remove(&key);
        return;
    }

    let direction_str = match &direction {
        Some(Direction::Recv) => Some("recv".to_string()),
        Some(Direction::Send) => Some("send".to_string()),
        Some(Direction::Ignore) | None => None,
    };
    diag(&format!(
        "run: key={key} direction={direction_str:?} prelude={} [{}]",
        prelude.len(),
        hex_head(&prelude, 24)
    ));
    let _ = app.emit(
        "rzsz-start",
        RzszStartEvent {
            key: key.clone(),
            direction: direction_str,
        },
    );

    let choice = match respond_rx.recv_timeout(CHOICE_TIMEOUT) {
        Ok(choice) => choice,
        // 超时/通道关闭（会话断开清理）→ 视作取消
        Err(_) => RzszChoice::Cancel,
    };

    let result = match choice {
        RzszChoice::Recv { dir } => {
            run_recv(&app, &key, &pipe_rx, &write_tx, &dir, prelude, &respond_rx)
        }
        RzszChoice::Send { path } => {
            run_send(&app, &key, &pipe_rx, &write_tx, &path, prelude, &respond_rx)
        }
        RzszChoice::Cancel => {
            // 用户取消：发中止序列，避免远端 sz/rz 继续重试刷出乱码
            write_to_wire(&write_tx, &CAN_ABORT);
            Ok(())
        }
    };
    // 失败路径补发中止序列，避免远端 sz/rz 继续重试刷出乱码
    if result.is_err() {
        write_to_wire(&write_tx, &CAN_ABORT);
    }

    // 终态：移除条目（读循环恢复常规转发）+ emit rzsz-end
    app.state::<AppState>()
        .rzsz_sessions
        .lock()
        .expect("rzsz_sessions 锁被污染")
        .remove(&key);
    let (ok, message) = match &result {
        Ok(()) => (true, String::new()),
        Err(e) => (false, e.to_string()),
    };
    let _ = app.emit("rzsz-end", RzszEndEvent { key, ok, message });
}

/// 接收文件（对端 sz）：Receiver 状态机收 ZFILE/ZDATA 落盘到 dir/<服务端文件名>
fn run_recv(
    app: &AppHandle,
    key: &str,
    pipe_rx: &std::sync::mpsc::Receiver<Vec<u8>>,
    write_tx: &tokio::sync::mpsc::Sender<crate::services::ssh::SshWriteMsg>,
    dir: &str,
    prelude: Vec<u8>,
    respond_rx: &std::sync::mpsc::Receiver<RzszChoice>,
) -> Result<(), String> {
    // buffer_len=0 + CANOVIO：向远端声明非停等 I/O，SSH 可靠流上连续流式传输
    let mut receiver = Receiver::with_flow_control(0, true)
        .map_err(|e| format!("初始化 rz/sz 接收器失败: {e}"))?;
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
    // prelude 帧损坏（CRC 错误）可恢复：对端重试机制会重发握手帧。不整批丢弃，
    // 原样保留为 leftover（consumed=0）让重发内容与之对齐——整批丢弃会因对端已
    // 进入重传而缺帧；其余错误终止传输
    let consumed = match receiver.submit_wire(&prelude) {
        Ok(n) => n,
        Err(e) if is_crc_error(&e) => 0,
        Err(e) => return Err(format!("rz/sz 协议错误: {e}")),
    };
    let mut leftover = prelude.split_off(consumed);
    cap_leftover(&mut leftover);
    let dir_path = PathBuf::from(dir);
    let mut file: Option<std::fs::File> = None;
    let mut file_name = String::new();
    let mut total: u64 = 0;
    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    let mut idle_polls: u32 = 0;

    loop {
        // 取消检查：前端「取消传输」经 respond_tx 到达。传输循环主线是
        // pipe_rx 等待与状态机驱动，取消必须旁路非阻塞感知，否则 persistent
        // 对话框挡住终端、取消按钮无效，用户鼠标完全无法操作
        match respond_rx.try_recv() {
            Ok(RzszChoice::Cancel) => return Err("rz/sz 传输已取消".into()),
            Ok(_) => {}
            Err(_) => {}
        }
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
                        "rzsz-progress",
                        RzszProgressEvent {
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
                        .map_err(|e| format!("rz/sz 协议错误: {e}"))?;
                }
                Action::Event(event) => match event {
                    Event::FileCompleted => {
                        // 单文件完成：终值进度兜底推送
                        emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                    }
                    Event::SessionCompleted => {
                        // 事件先于 WriteWire 返回（poll 事件优先）：会话完成时
                        // 状态机已排队 ZFIN 应答，直接 return 会让对端等应答
                        // 悬挂、远端 tty 停留 raw 模式（终端卡死至对端超时）——
                        // 必须先冲出收尾字节再返回
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
                        return Ok(());
                    }
                    Event::Aborted => return Err("rz/sz 传输已中止".into()),
                    // 接收方向不会收到 FileStarted（接收方专属事件）
                    Event::FileStarted(_) => {}
                },
                Action::ReadFile { .. } => {}
                Action::Idle => break,
            }
        }

        // 未消费残余优先喂入（outgoing 已排空；1 字节进展即继续）——完整帧
        // 滞留残余会让对端等待响应自旋
        if !leftover.is_empty() {
            match receiver.submit_wire(&leftover) {
                Ok(consumed) if consumed > 0 => {
                    leftover.drain(..consumed);
                    cap_leftover(&mut leftover);
                    continue;
                }
                Ok(_) => {}
                // CRC 损坏可恢复：保留 leftover 等对端重发对齐（状态机已排队 NAK/ZRPOS）
                Err(e) if is_crc_error(&e) => {}
                Err(e) => return Err(format!("rz/sz 协议错误: {e}")),
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                diag(&format!(
                    "recv: pipe 数据 {} 字节 [{}]",
                    data.len(),
                    hex_head(&data, 16)
                ));
                let mut buf = std::mem::take(&mut leftover);
                buf.extend_from_slice(&data);
                match receiver.submit_wire(&buf) {
                    Ok(consumed) if consumed < buf.len() => {
                        diag(&format!(
                            "recv: 喂入 {}/{} 状态={} 残余={}",
                            consumed,
                            buf.len(),
                            receiver.state_name(),
                            buf.len() - consumed
                        ));
                        leftover = buf.split_off(consumed);
                        cap_leftover(&mut leftover);
                    }
                    Ok(_) => {}
                    // CRC 损坏可恢复：状态机已排队 NAK/ZRPOS（对端从最后好偏移重传）。
                    // 保留本批数据供重发对齐，不整批丢弃——丢弃会把对端重传的帧头
                    // 一起丢掉导致缺帧；容量由 cap_leftover 兜底
                    Err(e) if is_crc_error(&e) => {
                        leftover = buf;
                        cap_leftover(&mut leftover);
                    }
                    Err(e) => return Err(format!("rz/sz 协议错误: {e}")),
                }
            }
            PipeWait::Timeout => {
                idle_polls += 1;
                if idle_polls >= MAX_IDLE_POLLS {
                    return Err("rz/sz 传输超时（链路无响应）".into());
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
    write_tx: &tokio::sync::mpsc::Sender<crate::services::ssh::SshWriteMsg>,
    path: &str,
    prelude: Vec<u8>,
    respond_rx: &std::sync::mpsc::Receiver<RzszChoice>,
) -> Result<(), String> {
    let mut sender = Sender::new().map_err(|e| format!("初始化 rz/sz 发送器失败: {e}"))?;
    // SSH 可靠流：非停等流式传输，免每包 ACK 往返
    sender.set_streaming_window(usize::MAX);

    let mut source = std::fs::File::open(path).map_err(|e| format!("读取本地文件失败: {e}"))?;
    let file_name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let total = source.metadata().map(|m| m.len()).unwrap_or(0);

    // 注册文件元数据：状态机收到 ZRINIT 后自动发 ZFILE（协议状态机要求
    // 调用方在 WaitReceiverInit/ReadyForFile 阶段显式 start_file，
    // 否则状态机干等、ReadFile 动作永不出现 → 传输饿死超时）
    sender
        .start_file(rzsz_protocol::FileInfo::new(
            file_name.as_bytes(),
            Some(rzsz_protocol::Position::new(u32::try_from(total).unwrap_or(u32::MAX))),
        ))
        .map_err(|e| format!("注册 rz/sz 文件失败: {e}"))?;
    // 单文件传输：当前文件完成后发 ZFIN 结束会话
    sender
        .finish()
        .map_err(|e| format!("注册 rz/sz 结束请求失败: {e}"))?;

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
    // 发送方向命中坏帧（CRC 错误）整批丢弃、不入 leftover 滞留：协议状态机发送侧
    // submit_wire 对坏帧只 queue_nak + 返回 Err（无 enter_resync，接收侧才有），
    // 若把坏帧留在 leftover 前部，对端随后重发的合法帧永远排在坏帧之后、每次
    // submit 先命中坏帧 Err 提前返回 → 合法重传帧长期不可达。丢弃后由本状态机
    // 已排队的 NAK / 对端超时重发恢复对齐（发送侧帧多为对端短响应控制帧，整批
    // 丢弃无损于本地待发文件数据）；接收侧才需保留数据供对齐重传。其余错误终止传输
    let consumed = match sender.submit_wire(&prelude) {
        Ok(n) => n,
        // 丢弃整批：split_off(prelude.len()) 后 leftover 为空（不滞留坏帧）
        Err(e) if is_crc_error(&e) => prelude.len(),
        Err(e) => {
            diag(&format!("send: submit_wire(prelude) Err: {e}"));
            return Err(format!("rz/sz 协议错误: {e}"));
        }
    };
    diag(&format!(
        "send: submit_wire(prelude) consumed={consumed}/{} 状态={}",
        prelude.len(),
        sender.state_name()
    ));
    let mut leftover = prelude.split_off(consumed);
    cap_leftover(&mut leftover);

    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    let mut idle_polls: u32 = 0;
    // 对端 ZSKIP 跳过文件标记（文件非空但 transferred==0 时置位）
    let mut skipped = false;
    let _ = app.emit(
        "rzsz-progress",
        RzszProgressEvent {
            key: key.to_string(),
            file_name: file_name.clone(),
            transferred,
            total,
        },
    );

    loop {
        // 取消检查：同 run_recv——persistent 对话框挡住终端时取消必须旁路感知
        match respond_rx.try_recv() {
            Ok(RzszChoice::Cancel) => return Err("rz/sz 传输已取消".into()),
            Ok(_) => {}
            Err(_) => {}
        }
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
                    diag(&format!(
                        "send: ReadFile offset={} 读 {n} 字节 → submit_file",
                        offset.get()
                    ));
                    transferred = u64::from(offset.get()) + n as u64;
                    if let Err(e) = sender.submit_file(&buf[..n]) {
                        return Err(format!("提交文件数据失败: {e}"));
                    }
                    emit_progress(app, key, &file_name, transferred, total, &mut last_emit);
                }
                Action::Event(event) => match event {
                    Event::FileCompleted => {
                        diag(&format!(
                            "send: FileCompleted transferred={transferred}/{total}"
                        ));
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
                        // 事件先于 WriteWire 返回（poll 事件优先）：会话完成时
                        // 状态机已排队 OO 结束序列，直接 return 会让对端等 OO
                        // 悬挂（远端 tty 停留 raw 模式致终端卡死）——先冲出
                        // 收尾字节再返回
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
                        if skipped {
                            return Err(
                                "对端跳过了该文件：同名文件已存在（远端 rz 默认不覆盖），可删除远端文件或让对端以 rz -y 运行后重试"
                                    .into(),
                            );
                        }
                        return Ok(());
                    }
                    Event::Aborted => return Err("rz/sz 传输已中止".into()),
                    // 发送方向不会收到 FileStarted（接收方专属事件）
                    Event::FileStarted(_) => {}
                },
                Action::WriteFile(_) => {}
                Action::Idle => break,
            }
        }

        // 未消费残余优先喂入（outgoing 已排空；1 字节进展即继续）——完整帧
        // 滞留残余会让对端等待响应自旋
        if !leftover.is_empty() {
            match sender.submit_wire(&leftover) {
                Ok(consumed) if consumed > 0 => {
                    leftover.drain(..consumed);
                    cap_leftover(&mut leftover);
                    continue;
                }
                Ok(_) => {}
                // 发送方向坏帧整批丢弃、不入 leftover 滞留（原因见下方管道喂入注释）：
                // 状态机已排队 NAK，对端重传恢复对齐
                Err(e) if is_crc_error(&e) => leftover.clear(),
                Err(e) => return Err(format!("rz/sz 协议错误: {e}")),
            }
        }

        // 管道等待 + 喂入状态机；超时驱动重试，连续无数据超限判定链路失效
        match wait_pipe(pipe_rx) {
            PipeWait::Data(data) => {
                idle_polls = 0;
                diag(&format!(
                    "send: pipe 数据 {} 字节 [{}] 状态={}",
                    data.len(),
                    hex_head(&data, 16),
                    sender.state_name()
                ));
                let mut buf = std::mem::take(&mut leftover);
                buf.extend_from_slice(&data);
                match sender.submit_wire(&buf) {
                    Ok(consumed) if consumed < buf.len() => {
                        leftover = buf.split_off(consumed);
                        cap_leftover(&mut leftover);
                    }
                    Ok(_) => {}
                    // 发送方向坏帧整批丢弃、不入 leftover 滞留：协议状态机发送侧
                    // submit_wire 对坏帧只 queue_nak + 返回 Err（无 enter_resync），
                    // 若把坏帧保留在 leftover 前部，对端随后重发的合法帧永远排在
                    // 坏帧之后、每次 submit 先命中坏帧 Err 提前返回 → 合法重传帧
                    // 长期不可达。丢弃后由状态机已排队的 NAK / 对端超时重发恢复
                    // （buf 已 take 清空 leftover，此处不赋回即整批丢弃，不滞留）；
                    // 与接收侧「保留数据供对齐重传」的处理相区分
                    Err(e) if is_crc_error(&e) => {}
                    Err(e) => return Err(format!("rz/sz 协议错误: {e}")),
                }
            }
            PipeWait::Timeout => {
                idle_polls += 1;
                diag(&format!(
                    "send: idle_poll={idle_polls}/{MAX_IDLE_POLLS} 状态={}",
                    sender.state_name()
                ));
                if idle_polls >= MAX_IDLE_POLLS {
                    return Err("rz/sz 传输超时（链路无响应）".into());
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
fn is_crc_error(e: &rzsz_protocol::ProtocolError) -> bool {
    matches!(
        e,
        rzsz_protocol::ProtocolError::UnexpectedCrc16
            | rzsz_protocol::ProtocolError::UnexpectedCrc32
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

/// 经既有写转发路径回传协议字节（与键盘输入同路）。
/// 有界通道满（下游写入积压）时丢弃本批并告警——阻塞线程无法 async 等待，
/// 丢弃后由 rz/sz 重传/超时机制重发对齐，避免无界积压导致内存膨胀。
fn write_to_wire(
    write_tx: &tokio::sync::mpsc::Sender<crate::services::ssh::SshWriteMsg>,
    bytes: &[u8],
) {
    diag(&format!(">> wire: {} [{}]", bytes.len(), hex_head(bytes, 16)));
    match write_tx.try_send(crate::services::ssh::SshWriteMsg::Data(bytes.to_vec())) {
        Ok(()) => {}
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            eprintln!(
                "[rzsz] 写缓冲已满：{} 字节协议数据被丢弃（等待对端重传）",
                bytes.len()
            );
        }
        Err(_) => {}
    }
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
            "rzsz-progress",
            RzszProgressEvent {
                key: key.to_string(),
                file_name: file_name.to_string(),
                transferred,
                total,
            },
        );
    }
}

/// 服务端文件名合法性：拒绝路径分隔符与相对路径片段（防路径穿越）。
/// Windows 保留名（CON/PRN/AUX/NUL/COM1-9/LPT1-9 及带扩展名形式）在落盘时会被
/// 系统拦截导致写文件失败/静默，加下划线前缀改名规避（仅 Windows 生效）。
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
    let mut safe = trimmed.to_string();
    if cfg!(windows) && is_windows_reserved_name(&safe) {
        safe = format!("_{safe}");
    }
    Ok(safe)
}

/// Windows 保留设备名检测：基名（首个 '.' 前的部分，含带扩展名形式如 "CON.txt"、
/// "COM1.log"）命中保留名即视为保留。纯字符串逻辑，非 Windows 平台不调用。
fn is_windows_reserved_name(name: &str) -> bool {
    use std::sync::OnceLock;
    static RESERVED: OnceLock<Vec<String>> = OnceLock::new();
    let reserved = RESERVED.get_or_init(|| {
        let mut v: Vec<String> = ["CON", "PRN", "AUX", "NUL"]
            .into_iter()
            .map(String::from)
            .collect();
        for i in 1..=9 {
            v.push(format!("COM{i}"));
            v.push(format!("LPT{i}"));
        }
        v
    });
    let stem = name.split('.').next().unwrap_or(name).trim().to_ascii_uppercase();
    reserved.iter().any(|r| r == &stem)
}
