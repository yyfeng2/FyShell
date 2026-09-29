//! 真实 lrzsz 对局 harness：经 russh 直连测试服务器运行远端 sz，
//! 把真实字节流喂给自研接收器，验证协议互通并定位传输挂起根因。
//!
//! 用法: cargo run --example sz_harness -- [remote_path] [local_dir]
//! 不依赖 UI、不经终端管道；协议层诊断走 temp/fyshell-rzsz.log。

#[path = "../src/services/rzsz_protocol.rs"]
mod rzsz_protocol;

use rzsz_protocol::{Action, Event, Receiver};
use russh::client;
use russh::ChannelMsg;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

const HOST: &str = "192.168.31.100";
const USER: &str = "root";
const PASSWORD: &str = "Ailab@123";
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let remote = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/root/rzsz-test.txt".into());
    let local_dir = std::env::args().nth(2).unwrap_or_else(|| {
        std::env::temp_dir()
            .to_string_lossy()
            .into_owned()
    });

    let mut config = client::Config::default();
    config.keepalive_interval = Some(Duration::from_secs(15));
    let mut handle = client::connect(Arc::new(config), (HOST, 22), AcceptAll).await?;
    handle.authenticate_password(USER, PASSWORD).await?;
    println!("[harness] auth ok");

    let channel = handle.channel_open_session().await?;
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await?;
    channel.request_shell(true).await?;
    println!("[harness] shell ready, running: {remote}");
    // 发送 sz 命令（含 "sz " 前缀——漏发会让 bash 把路径当命令执行）
    channel
        .data(format!("sz {remote}\r").into_bytes().as_slice())
        .await?;
    // 等远端 sz 启动并设 raw 模式：构造函数即排队的 ZRINIT 若在 shell
    // canonical 模式下发出，会被 bash 当命令执行/回显，污染字节流。
    // 应用内无此竞态（哨兵检测到时 sz 已设 raw）；harness 盲等 1.5s。
    tokio::time::sleep(Duration::from_millis(1500)).await;
    println!("[harness] grace period over, starting receiver");

    let (mut read_half, mut write_half) = channel.split();
    // 与应用 run_recv 完全一致的配置：buffer_len=0 + CANOVIO
    let mut receiver = Receiver::with_flow_control(0, true)
        .map_err(|e| format!("初始化接收器失败: {e}"))?;

    // 冲出构造函数排队的 ZRINIT
    loop {
        match receiver.poll() {
            Action::WriteWire(bytes) => {
                let n = bytes.len();
                println!("[harness] >> wire {n} bytes (ZRINIT)");
                write_half.data(bytes).await?;
                receiver.wire_written(n);
            }
            _ => break,
        }
    }

    let dir_path = std::path::PathBuf::from(local_dir);
    let mut file: Option<std::fs::File> = None;
    let mut leftover: Vec<u8> = Vec::new();
    let mut idle_polls: u32 = 0;
    let mut result_msg = String::new();

    loop {
        // 状态机动作处理到 Idle 为止（与 run_recv 一致）
        let mut done = false;
        loop {
            match receiver.poll() {
                Action::WriteWire(bytes) => {
                    let n = bytes.len();
                    println!("[harness] >> wire {n} bytes");
                    write_half.data(bytes).await?;
                    receiver.wire_written(n);
                }
                Action::Event(Event::FileStarted(info)) => {
                    let name = String::from_utf8_lossy(info.name).into_owned();
                    println!("[harness] file started: {name}");
                    file = Some(std::fs::File::create(dir_path.join(&name))?);
                }
                Action::WriteFile(data) => {
                    let n = data.len();
                    if let Some(f) = file.as_mut() {
                        f.write_all(data)?;
                    }
                    receiver.file_written(n)?;
                }
                Action::Event(Event::FileCompleted) => {
                    println!("[harness] file complete");
                }
                Action::Event(Event::SessionCompleted) => {
                    // 与修复后的 run_recv 一致：先冲出状态机排队的 ZFIN 应答
                    //（事件先于 WriteWire 返回，直接退出会让对端等应答悬挂、
                    // 远端 tty 停留 raw 模式）
                    loop {
                        match receiver.poll() {
                            Action::WriteWire(bytes) => {
                                let n = bytes.len();
                                println!("[harness] >> wire {n} bytes (ZFIN flush)");
                                write_half.data(bytes).await?;
                                receiver.wire_written(n);
                            }
                            _ => break,
                        }
                    }
                    result_msg = "session complete".into();
                    done = true;
                    break;
                }
                Action::Event(Event::Aborted) => {
                    result_msg = "aborted".into();
                    done = true;
                    break;
                }
                Action::ReadFile { .. } => {}
                Action::Idle => break,
            }
        }
        if done {
            break;
        }

        // 未消费残余优先喂入（与 run_recv 一致：drain 非 clear）
        if !leftover.is_empty() {
            match receiver.submit_wire(&leftover) {
                Ok(consumed) if consumed > 0 => {
                    leftover.drain(..consumed);
                    continue;
                }
                Ok(_) => {}
                Err(e) => {
                    println!("[harness] leftover submit err: {e}（保留对齐）");
                }
            }
        }

        // 通道读取 + 喂入；超时驱动重试
        match tokio::time::timeout(Duration::from_secs(1), read_half.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => {
                idle_polls = 0;
                let head: Vec<String> =
                    data.iter().take(24).map(|b| format!("{b:02X}")).collect();
                println!("[harness] << {} bytes [{:?}]", data.len(), head.join(" "));
                let mut buf = std::mem::take(&mut leftover);
                buf.extend_from_slice(&data);
                match receiver.submit_wire(&buf) {
                    Ok(consumed) if consumed < buf.len() => {
                        println!(
                            "[harness] consumed {}/{} state={} leftover={}",
                            consumed,
                            buf.len(),
                            receiver.state_name(),
                            buf.len() - consumed
                        );
                        leftover = buf.split_off(consumed);
                    }
                    Ok(consumed) => {
                        println!(
                            "[harness] consumed {consumed}/{} state={}",
                            buf.len(),
                            receiver.state_name()
                        );
                    }
                    Err(e) => {
                        println!("[harness] submit err: {e}（保留 {} 字节对齐）", buf.len());
                        leftover = buf;
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                result_msg = "channel closed".into();
                break;
            }
            Err(_) => {
                // 超时：驱动接收器重试（与 run_recv 的 PipeWait::Timeout 一致）
                idle_polls += 1;
                if idle_polls >= IDLE_MAX_POLLS {
                    result_msg = "timeout（60s 无数据）".into();
                    break;
                }
                receiver.timeout();
            }
        }
    }
    println!("[harness] done: {result_msg}");

    // 终端响应性验证（复刻用户报告的「下载完成后终端卡死」）：传输结束后
    // 等对端 sz 退出（收到 ZFIN 应答后毫秒级），再发送 echo 命令，
    // shell 回显出现 = 远端 tty 已退出 raw 模式、终端可用
    println!("[harness] terminal responsiveness check...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    write_half
        .data(Vec::from("echo TERMINAL_OK\r").as_slice())
        .await?;
    let mut responsive = false;
    let mut out_all: Vec<u8> = Vec::new();
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_secs(1), read_half.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => {
                out_all.extend_from_slice(&data);
                if out_all.windows(11).any(|w| w == b"TERMINAL_OK") {
                    responsive = true;
                    break;
                }
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {}
        }
    }
    let echo_head: Vec<String> = out_all
        .iter()
        .take(80)
        .map(|b| format!("{b:02X}"))
        .collect();
    println!(
        "[harness] terminal {} raw=[{:?}] hex64=[{}]",
        if responsive { "responsive ✓" } else { "FROZEN ✗" },
        String::from_utf8_lossy(&out_all),
        echo_head.join(" ")
    );
    if !responsive {
        return Err("传输完成后终端无响应（远端 tty 未恢复）".into());
    }
    Ok(())
}
