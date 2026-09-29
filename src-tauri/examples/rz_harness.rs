//! 真实 lrzsz 对局 harness（发送路径）：经 russh 直连测试服务器运行远端 `rz`，
//! 把自研 Sender 状态机的输出喂回远端接收器，验证「rz 上传」协议互通。
//!
//! 与 sz_harness 互补：sz_harness 验证接收路径（远端 sz → 自研 Receiver），
//! 本文件验证发送路径（远端 rz → 自研 Sender，即用户报告「rz 无法上传」）。
//!
//! 用法: cargo run --example rz_harness -- [local_file]
//! 默认上传文件: <temp>/fyshell-rzsz-upload.txt（自动生成）
//! 远端落盘: /root/rzh/<文件名>

#[path = "../src/services/rzsz_protocol.rs"]
mod rzsz_protocol;

use rzsz_protocol::{Action, Event, Sender};
use russh::client;
use russh::ChannelMsg;
use std::io::{Read, Seek};
use std::sync::Arc;
use std::time::Duration;

const HOST: &str = "192.168.31.100";
const USER: &str = "root";
const PASSWORD: &str = "Ailab@123";

/// 哨兵：ZPAD ZPAD ZDLE 'B'
const SENTINEL: [u8; 4] = [b'*', b'*', 0x18, b'B'];
const IDLE_MAX_POLLS: u32 = 60;

struct AcceptAll;

impl client::Handler for AcceptAll {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// 读一批字节喂入发送器。
/// `started`：是否已越过哨兵（进入协议数据阶段）。哨兵前累积在 `search_buf`
///（处理哨兵跨批拆分，应用 read_loop 的 partial_sentinel 扣留同效）。
async fn pump_one(
    read_half: &mut russh::ChannelReadHalf,
    sender: &mut Sender,
    search_buf: &mut Vec<u8>,
    started: &mut bool,
    leftover: &mut Vec<u8>,
    idle_polls: &mut u32,
) -> Result<bool, Box<dyn std::error::Error>> {
    match tokio::time::timeout(Duration::from_secs(1), read_half.wait()).await {
        Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
            // lrz -vvv 调试文本走 stderr（ExtendedData）：只打印，不喂协议流
            println!("[rzh] [lrz] {}", String::from_utf8_lossy(&data));
            Ok(true)
        }
        Ok(Some(ChannelMsg::Data { data })) => {
            *idle_polls = 0;
            if !*started {
                // 找哨兵：从累积缓冲首部截取完整哨兵起的数据
                search_buf.extend_from_slice(&data);
                let Some(pos) = search_buf
                    .windows(SENTINEL.len())
                    .position(|w| w == SENTINEL)
                else {
                    // 哨兵未到（或跨批拆分未完）：保留等下一批
                    if search_buf.len() > 8192 {
                        search_buf.clear();
                    }
                    return Ok(false);
                };
                let mut rest = search_buf.split_off(pos);
                // 前哨文本仅诊断
                let pre: Vec<String> = search_buf
                    .iter()
                    .take(20)
                    .map(|b| format!("{b:02X}"))
                    .collect();
                println!("[rzh] sentinel found, 前哨 {} 字节 [{:?}]", search_buf.len(), pre);
                search_buf.clear();
                *started = true;
                let head: Vec<String> = rest.iter().take(16).map(|b| format!("{b:02X}")).collect();
                println!("[rzh] << {} bytes [{:?}]", rest.len(), head.join(" "));
                match sender.submit_wire(&rest) {
                    Ok(consumed) if consumed < rest.len() => {
                        *leftover = rest.split_off(consumed);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        println!("[rzh] submit err: {e}（丢弃整批，对端重发恢复）");
                        leftover.clear();
                    }
                }
                Ok(true)
            } else {
                let head: Vec<String> = data.iter().take(16).map(|b| format!("{b:02X}")).collect();
                println!("[rzh] << {} bytes [{:?}]", data.len(), head.join(" "));
                let mut buf = std::mem::take(leftover);
                buf.extend_from_slice(&data);
                match sender.submit_wire(&buf) {
                    Ok(consumed) if consumed < buf.len() => {
                        println!(
                            "[rzh] consumed {}/{} state={} leftover={}",
                            consumed,
                            buf.len(),
                            sender.state_name(),
                            buf.len() - consumed
                        );
                        *leftover = buf.split_off(consumed);
                    }
                    Ok(consumed) => {
                        println!(
                            "[rzh] consumed {consumed}/{} state={}",
                            buf.len(),
                            sender.state_name()
                        );
                    }
                    Err(e) => {
                        println!("[rzh] submit err: {e}（丢弃整批，对端重发恢复）");
                        leftover.clear();
                    }
                }
                Ok(true)
            }
        }
        Ok(Some(_)) => Ok(true),
        Ok(None) => Err("channel closed".into()),
        Err(_) => {
            // 超时：驱动发送器重试（与 run_send 的 PipeWait::Timeout 一致）
            *idle_polls += 1;
            if *idle_polls >= IDLE_MAX_POLLS {
                return Err("timeout（60s 无数据）".into());
            }
            sender.timeout();
            Ok(false)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 本地待上传文件
    let local_file = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let p = std::env::temp_dir().join("fyshell-rzsz-upload.txt");
            std::fs::write(&p, b"hello from rz harness\n".repeat(50)).expect("写临时文件");
            p
        });
    println!("[rzh] uploading: {}", local_file.display());

    let mut config = client::Config::default();
    config.keepalive_interval = Some(Duration::from_secs(15));
    let mut handle = client::connect(Arc::new(config), (HOST, 22), AcceptAll).await?;
    handle.authenticate_password(USER, PASSWORD).await?;
    println!("[rzh] auth ok");

    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;
    channel
        .data(b"rm -rf /root/rzh && mkdir -p /root/rzh && cd /root/rzh && rz -vvv\r".as_slice())
        .await?;
    // 等远端 rz 启动并设 raw 模式：构造即排队的 ZRINIT 若在 shell canonical
    // 模式下发出，会被 bash 当命令执行/回显，污染字节流（与 sz_harness 同竞态）
    tokio::time::sleep(Duration::from_millis(1500)).await;
    println!("[rzh] grace period over, starting sender");

    let (mut read_half, mut write_half) = channel.split();

    // 与应用 run_send 完全一致的初始化：全非停等流式 + 单文件 + 结束
    let mut sender = Sender::new().map_err(|e| format!("初始化发送器失败: {e}"))?;
    sender.set_streaming_window(usize::MAX);
    let mut source = std::fs::File::open(&local_file)?;
    let file_name = local_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| local_file.to_string_lossy().into_owned());
    let total = source.metadata().map(|m| m.len()).unwrap_or(0);
    println!("[rzh] file: {file_name} total={total}");
    sender
        .start_file(rzsz_protocol::FileInfo::new(
            file_name.as_bytes(),
            Some(rzsz_protocol::Position::new(u32::try_from(total).unwrap_or(u32::MAX))),
        ))
        .expect("start_file");
    sender.finish().expect("finish");

    // 冲出构造函数排队的 ZRQINIT
    loop {
        match sender.poll() {
            Action::WriteWire(bytes) => {
                let n = bytes.len();
                println!("[rzh] >> wire {n} bytes (ZRQINIT)");
                write_half.data(bytes).await?;
                sender.wire_written(n);
            }
            _ => break,
        }
    }

    let mut search_buf: Vec<u8> = Vec::new();
    let mut started = false;
    let mut leftover: Vec<u8> = Vec::new();
    let mut idle_polls: u32 = 0;
    let mut result_msg = String::new();
    let mut transferred: u64 = 0;
    let mut skipped = false;

    'outer: loop {
        // 状态机动作处理到 Idle 为止
        'poll: loop {
            match sender.poll() {
                Action::WriteWire(bytes) => {
                    let n = bytes.len();
                    let hex: Vec<String> = bytes.iter().take(24).map(|b| format!("{b:02X}")).collect();
                    println!("[rzh] >> wire {n} bytes [{:?}]", hex.join(" "));
                    write_half.data(bytes).await?;
                    sender.wire_written(n);
                }
                Action::ReadFile { offset, max_len } => {
                    let mut buf = vec![0u8; max_len.min(16 * 1024)];
                    source.seek(std::io::SeekFrom::Start(u64::from(offset.get())))?;
                    let n = source.read(&mut buf)?;
                    if n == 0 {
                        return Err("文件读取提前结束".into());
                    }
                    transferred = u64::from(offset.get()) + n as u64;
                    println!(
                        "[rzh] ReadFile offset={} 读 {n} 字节 → submit_file",
                        offset.get()
                    );
                    sender.submit_file(&buf[..n])?;
                }
                Action::Event(event) => match event {
                    Event::FileCompleted => {
                        println!(
                            "[rzh] Event::FileCompleted transferred={transferred}/{total}"
                        );
                        // 复刻 app run_send 的跳过处理：文件非空但 transferred==0
                        // = 对端 ZSKIP（同名文件已存在），发 ZFIN 干净结束会话
                        if transferred == 0 && total > 0 {
                            skipped = true;
                            let _ = sender.finish();
                        }
                    }
                    Event::SessionCompleted => {
                        // 先冲出状态机排队的收尾字节（OO 序列）再退出，
                        // 否则对端等 OO 悬挂、远端 tty 停留 raw 模式
                        loop {
                            match sender.poll() {
                                Action::WriteWire(bytes) => {
                                    let n = bytes.len();
                                    println!("[rzh] >> wire {n} bytes (收尾)");
                                    write_half.data(bytes).await?;
                                    sender.wire_written(n);
                                }
                                _ => break,
                            }
                        }
                        result_msg = if skipped {
                            "skipped (file exists)".into()
                        } else {
                            "session complete".into()
                        };
                        break 'outer;
                    }
                    Event::Aborted => {
                        result_msg = "aborted".into();
                        break 'outer;
                    }
                    Event::FileStarted(_) => {}
                },
                Action::WriteFile(_) => {}
                Action::Idle => break 'poll,
            }
        }

        // 未消费残余优先喂入
        if !leftover.is_empty() {
            match sender.submit_wire(&leftover) {
                Ok(consumed) if consumed > 0 => {
                    leftover.drain(..consumed);
                    continue;
                }
                Ok(_) => {}
                Err(_) => {
                    leftover.clear();
                }
            }
        }

        // 等对端应答
        pump_one(
            &mut read_half,
            &mut sender,
            &mut search_buf,
            &mut started,
            &mut leftover,
            &mut idle_polls,
        )
        .await?;
    }
    println!("[rzh] done: {result_msg}");

    // 远端验证：等 rz 退出后执行 ls + wc 校验
    tokio::time::sleep(Duration::from_secs(2)).await;
    // 先排空残留输出（提示符重建等），再埋哨兵读校验结果。
    // 哨兵字面量用引号拼接（__RZH_'END'__）：回显的命令行本身不含 __RZH_END__，
    // 否则捕获循环在回显批次就提前 break，ls/md5sum 输出永远读不回。
    for _ in 0..3 {
        let _ = tokio::time::timeout(Duration::from_millis(300), read_half.wait()).await;
    }
    write_half
        .data(
            Vec::from("echo __RZH_'DONE'__; ls -l /root/rzh; md5sum /root/rzh/*; echo __RZH_'END'__\r")
                .as_slice(),
        )
        .await?;
    let mut out_all: Vec<u8> = Vec::new();
    for _ in 0..20 {
        match tokio::time::timeout(Duration::from_secs(1), read_half.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) | Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                out_all.extend_from_slice(&data);
                if out_all.windows(11).any(|w| w == b"__RZH_END__") {
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {}
        }
    }
    let text = String::from_utf8_lossy(&out_all);
    println!("[rzh] remote verify:\n{}", text.trim());
    // 本地 md5 对照
    let _ = std::process::Command::new("md5sum")
        .arg(&local_file)
        .status();
    if text.contains("fyshell-rzsz-upload.txt") && !text.contains("No such file") {
        println!("[rzh] ✓ 上传文件已落盘（可核对上方 md5 与本地文件是否一致）");
    } else {
        println!("[rzh] ✗ 未在远端 /root/rzh 看到上传文件");
    }
    Ok(())
}
