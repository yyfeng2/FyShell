//! SSH 服务封装：基于 russh 0.63 的客户端连接、5 种认证、HostKey 验证、
//! PTY/shell 请求、读循环与 keepalive。
//!
//! 架构红线：
//! - 终端输出 Rust 侧 4KB 批量读取再经 tauri::ipc::Channel 透传，ANSI 解析交给 xterm.js；
//! - 连接状态通过 `session-status` 事件推送（仅低频状态走 event）；
//! - 断开时同步清理 AppState::ssh_sessions 中的句柄。

use std::sync::{Arc, PoisonError};
use std::time::Duration;

use russh::client::{AuthResult, DisconnectReason};
use russh::keys::known_hosts;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, load_secret_key};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, Disconnect};
use tauri::ipc::Channel;
use tauri::{Emitter, Manager};

use crate::error::AppError;
use crate::models::session::{AuthType, SessionConfig};
use crate::state::AppState;

use super::login_script;
use super::ssh_proxy::ProxyConfig;
use crate::ssh_trace;

/// 默认终端尺寸（前端 resize 后更新）
const DEFAULT_COLS: u32 = 80;
const DEFAULT_ROWS: u32 = 24;

/// 读循环批量发送阈值：累计 >= 4KB 才向 Channel 发送一次，减少 IPC 次数
const READ_BATCH_BYTES: usize = 4096;

/// 读循环冲刷延迟：小批量输出最多等待该毫秒数即发送，保证交互低延迟显示
const FLUSH_DELAY_MS: u64 = 20;

/// session-status 事件的状态字符串（契约第 3 节）
const STATUS_CONNECTING: &str = "connecting";
const STATUS_CONNECTED: &str = "connected";
const STATUS_DISCONNECTED: &str = "disconnected";
const STATUS_HOSTKEY_VERIFY: &str = "hostkey-verify";

/// std::sync::Mutex 中毒时的统一错误转换
fn lock_err<T>(_e: PoisonError<T>) -> AppError {
    AppError::general("全局状态锁被污染")
}

// ---------------------------------------------------------------------------
// 会话句柄
// ---------------------------------------------------------------------------

/// 单个 SSH 会话句柄：持有 russh 连接句柄与 shell 写入通道，
/// 提供 write / resize / close 能力。存于 `AppState::ssh_sessions`。
/// 可 Clone（内部字段用 Arc 包裹），供传输任务跨 await 共享。
#[derive(Clone)]
pub struct SshSessionHandle {
    /// russh 连接句柄（发送全局消息：disconnect、打开新 channel 等）
    handle: Arc<russh::client::Handle<SshClientHandler>>,

    /// 键盘输入 / resize 消息队列，由独立写转发任务消费（同步接口下避免阻塞）
    write_tx: tokio::sync::mpsc::UnboundedSender<SshWriteMsg>,
}

/// 写转发任务的消息：键盘输入或终端 resize
enum SshWriteMsg {
    /// 键盘输入字节流
    Data(Vec<u8>),
    /// 终端尺寸变更（pix 尺寸固定 0）
    Resize { cols: u32, rows: u32 },
}

impl SshSessionHandle {
    /// 打开 SFTP 子系统 channel 并返回 russh_sftp::client::SftpSession
    /// （供 services/sftp.rs 使用；每次调用打开独立 channel，互不影响终端）
    pub async fn open_sftp(&self) -> Result<russh_sftp::client::SftpSession, AppError> {
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::Ssh(format!("打开 SFTP channel 失败: {e}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| AppError::Ssh(format!("请求 SFTP 子系统失败: {e}")))?;
        russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| AppError::Ssh(format!("初始化 SFTP 会话失败: {e}")))
    }

    /// 连接是否仍然存活
    pub fn is_alive(&self) -> bool {
        !self.handle.is_closed()
    }

    /// 在已连接会话上执行一次性命令并返回全部输出（P1：供监控/Docker 采集使用）。
    /// 独立 session channel，与终端互不影响；stderr 一并收集。
    pub async fn exec(&self, command: &str) -> Result<String, AppError> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| AppError::Ssh(format!("打开 exec channel 失败: {e}")))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| AppError::Ssh(format!("执行命令失败: {e}")))?;
        // 读取全部输出直到流结束（Eof/Close/对端断开）
        let mut out: Vec<u8> = Vec::new();
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    out.extend_from_slice(&data);
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                // 窗口调整等其他消息直接忽略
                Some(_) => {}
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// 开通 direct-tcpip 通道（P1：供 SSH 隧道本地/SOCKS5 转发使用）。
    /// 返回的 Channel 经 `into_stream()` 转为流后双向转发。
    pub async fn open_direct_tcpip(
        &self,
        target_host: &str,
        target_port: u16,
        originator_host: &str,
        originator_port: u16,
    ) -> Result<russh::Channel<russh::client::Msg>, AppError> {
        self.handle
            .channel_open_direct_tcpip(
                target_host,
                u32::from(target_port),
                originator_host,
                u32::from(originator_port),
            )
            .await
            .map_err(|e| AppError::Ssh(format!("开通 direct-tcpip 通道失败: {e}")))
    }

    /// 发送写转发消息（内部使用）
    fn send_msg(&self, msg: SshWriteMsg) -> Result<(), AppError> {
        self.write_tx
            .send(msg)
            .map_err(|_| AppError::Ssh("会话已关闭，无法写入".into()))
    }
}

// ---------------------------------------------------------------------------
// client::Handler 实现：HostKey 验证回调 + 断开通知
// ---------------------------------------------------------------------------

/// russh client::Handler 实现：0.63 的 Handler 为内建 async trait（无需 async-trait crate）。
struct SshClientHandler {
    /// 应用句柄：emit hostkey-prompt / session-status 事件
    app: tauri::AppHandle,
    /// 会话 id（= SessionConfig.id）
    session_id: String,
    /// 目标主机（emit payload 用）
    host: String,
    /// 目标端口（known_hosts 匹配用）
    port: u16,
    /// 主机 key 严格模式（SSH 选项「SSH → 安全性」）：开启时未信任主机直接拒绝，不弹确认窗
    hostkey_strict: bool,
}

/// hostkey-prompt 事件 payload（契约第 3 节）
#[derive(Clone, serde::Serialize)]
struct HostkeyPromptPayload {
    id: String,
    host: String,
    fingerprint: String,
}

/// session-status 事件 payload（契约第 3 节）
#[derive(Clone, serde::Serialize)]
struct SessionStatusPayload {
    id: String,
    status: String,
}

/// 推送连接状态变更（低频状态走 event）。
/// pub(crate)：本地终端 / Telnet / 串口服务复用同一 session-status 事件流。
pub(crate) fn emit_status(app: &tauri::AppHandle, id: &str, status: &str) {
    let _ = app.emit(
        "session-status",
        SessionStatusPayload {
            id: id.to_string(),
            status: status.to_string(),
        },
    );
}

impl russh::client::Handler for SshClientHandler {
    type Error = AppError;

    /// HostKey 验证回调：
    /// - 已信任（known_hosts 命中）→ 直接通过；
    /// - 未信任 → emit `hostkey-prompt` 事件给前端，把 oneshot::Sender 存入
    ///   AppState::pending_hostkey 挂起，等待前端调用 ssh_hostkey_accept 后继续/中止。
    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // P0 简化：OpenSSH 证书直接信任（证书/跳板校验在后续版本完善）
        if matches!(server_public_key, PublicKeyOrCertificate::Certificate(_)) {
            return Ok(true);
        }

        let key = server_public_key.public_key();
        // SHA-256 指纹（"SHA256:..." 格式）
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();

        // known_hosts 已记录则无需再次确认；
        // 主机 key 变更（Err(KeyChanged)）同样弹窗交由用户决定
        let trusted = known_hosts::check_known_hosts(&self.host, self.port, &key).unwrap_or(false);
        if trusted {
            return Ok(true);
        }

        // 主机 key 严格模式（SSH 选项）：未信任主机一律拒绝，不弹确认窗
        if self.hostkey_strict {
            eprintln!("[ssh] {} hostkey strict: rejecting untrusted key", self.session_id);
            return Ok(false);
        }

        // 推送 hostkey-prompt 事件（payload 按契约第 3 节）
        let _ = self.app.emit(
            "hostkey-prompt",
            HostkeyPromptPayload {
                id: self.session_id.clone(),
                host: self.host.clone(),
                fingerprint,
            },
        );
        emit_status(&self.app, &self.session_id, STATUS_HOSTKEY_VERIFY);

        // oneshot 挂起：Sender 存入 AppState::pending_hostkey，
        // ssh_hostkey_accept 取出后 send(bool) 继续/中止
        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        {
            let state = self.app.state::<AppState>();
            state
                .pending_hostkey
                .lock()
                .map_err(lock_err)?
                .insert(self.session_id.clone(), tx);
        }
        // 前端确认前一直挂起；Sender 被清理（如断开）视为拒绝
        let accepted = rx.await.unwrap_or(false);

        if accepted {
            // 用户信任该主机：写入 known_hosts，下次连接不再确认
            let _ = known_hosts::learn_known_hosts(&self.host, self.port, &key);
        }
        Ok(accepted)
    }

    /// 连接断开（远端断开或网络错误）时推送 disconnected 状态，
    /// 并同步清理 AppState 中的残留句柄与挂起的 HostKey 确认。
    async fn disconnected(
        &mut self,
        _reason: DisconnectReason<Self::Error>,
    ) -> Result<(), AppError> {
        emit_status(&self.app, &self.session_id, STATUS_DISCONNECTED);
        if let Some(state) = self.app.try_state::<AppState>() {
            if let Ok(mut sessions) = state.ssh_sessions.lock() {
                sessions.remove(&self.session_id);
            }
            if let Ok(mut pending) = state.pending_hostkey.lock() {
                pending.remove(&self.session_id);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 认证：契约 AuthType 的 5 种方式
// ---------------------------------------------------------------------------

/// 按认证方式完成 SSH 认证
async fn authenticate(
    handle: &mut russh::client::Handle<SshClientHandler>,
    cfg: &SessionConfig,
) -> Result<(), AppError> {
    match &cfg.auth_type {
        AuthType::Password { password } => {
            match handle.authenticate_password(&cfg.username, password).await? {
                AuthResult::Success => Ok(()),
                AuthResult::Failure { .. } => {
                    Err(AppError::Ssh("密码认证被拒绝（认证失败）".into()))
                }
            }
        }
        AuthType::PublicKey {
            private_key_path,
            passphrase,
        } => {
            // 读私钥文件 + 可选 passphrase
            let key = load_secret_key(private_key_path, passphrase.as_deref())
                .map_err(|e| AppError::Ssh(format!("读取私钥 {private_key_path} 失败: {e}")))?;
            // RSA 时向服务器查询最佳 hash 算法（非 RSA 自动忽略）
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            match handle
                .authenticate_publickey(&cfg.username, key_with_hash)
                .await?
            {
                AuthResult::Success => Ok(()),
                AuthResult::Failure { .. } => {
                    Err(AppError::Ssh("公钥认证被拒绝（认证失败）".into()))
                }
            }
        }
        AuthType::Interactive { password } => {
            // keyboard-interactive：启动后循环响应服务端的 InfoRequest，
            // 用配置中的密码回答所有非回显（echo=false）提示
            let mut response = handle
                .authenticate_keyboard_interactive_start(&cfg.username, None)
                .await?;
            loop {
                match response {
                    russh::client::KeyboardInteractiveAuthResponse::Success => return Ok(()),
                    russh::client::KeyboardInteractiveAuthResponse::Failure { .. } => {
                        return Err(AppError::Ssh("keyboard-interactive 认证被拒绝".into()));
                    }
                    russh::client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                        let replies = prompts
                            .iter()
                            .map(|p| if p.echo { String::new() } else { password.clone() })
                            .collect();
                        response = handle
                            .authenticate_keyboard_interactive_respond(replies)
                            .await?;
                    }
                }
            }
        }
        AuthType::NoAuth => {
            match handle.authenticate_none(&cfg.username).await? {
                AuthResult::Success => Ok(()),
                AuthResult::Failure { .. } => {
                    Err(AppError::Ssh("服务器要求认证（NoAuth 被拒绝）".into()))
                }
            }
        }
        AuthType::Jump { .. } => {
            // 跳板机：P0 简化为记录配置但走直连（嵌套转发在 P1 实现）
            Err(AppError::Ssh(
                "跳板机（Jump）模式暂未实现，请使用直连方式".into(),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// 读循环 / 写转发
// ---------------------------------------------------------------------------

/// 读循环：批量读取 channel 输出 → `Channel<Vec<u8>>` send。
/// 凑满 4KB 立即发送；不足 4KB 时最多等 FLUSH_DELAY_MS 即冲刷
/// （否则登录 banner + 提示符远小于 4KB，输出会一直滞留导致终端黑屏）。
/// ANSI 解析交给 xterm.js；流结束（Eof/Close/断开）时由 Handler::disconnected 推送断开状态。
/// 会话日志开启时同步追加落盘（P1，write_log 未启用时立即返回）。
async fn read_loop(
    session_id: String,
    mut read_half: russh::ChannelReadHalf,
    on_output: Channel<Vec<u8>>,
) {
    let mut buf: Vec<u8> = Vec::with_capacity(READ_BATCH_BYTES);
    let mut first_chunk = true;
    loop {
        // 有积压数据时限时等待（超时即冲刷）；空缓冲时无限等待
        let msg = if buf.is_empty() {
            read_half.wait().await
        } else {
            match tokio::time::timeout(Duration::from_millis(FLUSH_DELAY_MS), read_half.wait())
                .await
            {
                Ok(msg) => msg,
                // 超时：视为无消息，走下方冲刷分支
                Err(_) => None,
            }
        };
        match msg {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                if first_chunk {
                    first_chunk = false;
                    eprintln!("[ssh] {} first output chunk: {} bytes", session_id, data.len());
                }
                buf.extend_from_slice(&data);
                if buf.len() >= READ_BATCH_BYTES {
                    // take() 取走已积累内容并复用缓冲区容量
                    let chunk = std::mem::take(&mut buf);
                    crate::services::session_log::write_log(&session_id, &chunk);
                    if on_output.send(chunk).is_err() {
                        break;
                    }
                }
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
            // 窗口调整等其他消息直接忽略
            Some(_) => {}
            None => {
                // 空缓冲：流结束（对端断开）；非空缓冲：超时 → 冲刷积压输出
                if buf.is_empty() {
                    break;
                }
                let chunk = std::mem::take(&mut buf);
                crate::services::session_log::write_log(&session_id, &chunk);
                if on_output.send(chunk).is_err() {
                    eprintln!("[ssh] {} channel send failed", session_id);
                    break;
                }
            }
        }
    }
    // 冲刷残留输出
    if !buf.is_empty() {
        crate::services::session_log::write_log(&session_id, &buf);
        let _ = on_output.send(buf);
    }
}

/// 写转发任务：消费键盘输入 / resize 消息队列，转发到 russh channel
async fn write_forward(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<SshWriteMsg>,
    half: russh::ChannelWriteHalf<russh::client::Msg>,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            SshWriteMsg::Data(data) => {
                // &[u8] 实现 AsyncRead；russh 内部按窗口/包大小分片
                if half.data(&data[..]).await.is_err() {
                    break;
                }
            }
            SshWriteMsg::Resize { cols, rows } => {
                let _ = half.window_change(cols, rows, 0, 0).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 建立 SSH 连接：TCP 连接 → HostKey 验证 → 认证 → PTY/shell → 读循环。
/// 输出流走 tauri::ipc::Channel（4KB 批量），连接状态通过 session-status 事件推送。
/// key：连接路由键（多标签同会话独立连接时为每标签唯一，未传时等于会话 id）。
pub async fn connect(
    app: &tauri::AppHandle,
    state: &AppState,
    cfg: &SessionConfig,
    key: &str,
    on_output: tauri::ipc::Channel<Vec<u8>>,
) -> Result<(), AppError> {
    // 防止重复连接：同键已存在时先断开旧会话再完整重建。
    // 场景：前端状态丢失（HMR/页面重载）后重连时，旧输出 Channel 已随旧 JS
    // 上下文失效，仅返回"已连接"会让前端永远无法恢复输出（终端黑屏），
    // 故此处选择重建而非报错。
    {
        let existing = state
            .ssh_sessions
            .lock()
            .map_err(lock_err)?
            .remove(key);
        if let Some(handle) = existing {
            eprintln!("[ssh] {} already connected, rebuilding session", key);
            if let Ok(mut pending) = state.pending_hostkey.lock() {
                pending.remove(key);
            }
            let _ = handle
                .handle
                .disconnect(Disconnect::ByApplication, "", "")
                .await;
        }
    }
    eprintln!("[ssh] {} connect start: {}:{}", key, cfg.host, cfg.port);
    emit_status(app, key, STATUS_CONNECTING);

    // 跟踪开关：连接是低频操作，每次连接前刷新一次（SSH 选项「高级 → 跟踪」）
    let _ = super::trace::refresh_from_settings();
    ssh_trace!("[ssh] {key} connect start: {}:{}", cfg.host, cfg.port);
    emit_status(app, key, STATUS_CONNECTING);

    // 连接配置：keepalive 按 SessionConfig.keepalive_interval 秒（russh 内建 keepalive 定时器）；
    // sshopt_keepalive_enabled 关闭时全局禁用 keepalive（SSH 选项「连接 → 保持活动状态」）
    let keepalive_enabled = settings_flag("sshopt_keepalive_enabled", true);
    let mut config = russh::client::Config::default();
    if keepalive_enabled {
        config.keepalive_interval =
            Some(Duration::from_secs(u64::from(cfg.keepalive_interval.max(1))));
    }
    // 压缩：SSH 选项「连接 → 启用压缩」（russh 需 flate2 feature；默认关闭）
    config.preferred.compression = if settings_flag("sshopt_compression", false) {
        std::borrow::Cow::Borrowed(&[russh::compression::ZLIB])
    } else {
        std::borrow::Cow::Borrowed(&[russh::compression::NONE])
    };

    let handler = SshClientHandler {
        app: app.clone(),
        session_id: key.to_string(),
        host: cfg.host.clone(),
        port: cfg.port,
        hostkey_strict: settings_flag("sshopt_hostkey_strict", false),
    };

    // 代理：启用时经 SOCKS5/HTTP CONNECT 握手建立流，再交给 russh connect_stream
    let proxy = ProxyConfig::from_settings().unwrap_or(None);
    // kex 期间 Handler 的 check_server_key 可能挂起等待前端确认
    let mut handle = if let Some(proxy) = &proxy {
        ssh_trace!(
            "[ssh] {key} connecting via proxy {}:{}",
            proxy.host,
            proxy.port
        );
        let stream = super::ssh_proxy::connect(cfg.host.as_str(), cfg.port, proxy).await?;
        russh::client::connect_stream(Arc::new(config), stream, handler).await?
    } else {
        russh::client::connect(Arc::new(config), (cfg.host.as_str(), cfg.port), handler).await?
    };
    ssh_trace!("[ssh] {} tcp+kex+hostkey done", cfg.id);

    // 认证（5 种方式按契约 AuthType）
    authenticate(&mut handle, cfg).await?;
    ssh_trace!("[ssh] {} auth done ({:?})", cfg.id, cfg.auth_type);

    // 打开 session channel 并请求 PTY + shell（默认 80x24，前端 resize 后更新）；
    // 终端类型按 SSH 选项「终端 → VT 模式」（白名单校验，缺省 xterm-256color）
    let term_type = match settings_text("sshopt_vt_term_type").as_str() {
        "xterm" | "vt100" | "vt102" | "vt220" | "ansi" | "linux" => {
            settings_text("sshopt_vt_term_type")
        }
        _ => "xterm-256color".to_string(),
    };
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| AppError::Ssh(format!("打开 channel 失败: {e}")))?;
    channel
        .request_pty(true, &term_type, DEFAULT_COLS, DEFAULT_ROWS, 0, 0, &[])
        .await
        .map_err(|e| AppError::Ssh(format!("请求 PTY 失败: {e}")))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| AppError::Ssh(format!("请求 shell 失败: {e}")))?;

    // 拆分读写半，分别交给读循环与写转发任务。
    // 日志键用稳定会话 id（非 per-tab 路由键 key）：同一会话多标签/重连共享一份
    // 落盘，关闭重开标签不产生孤儿日志目录（LogViewer 亦按会话 id 查询）。
    let (read_half, write_half) = channel.split();
    tauri::async_runtime::spawn(read_loop(cfg.id.clone(), read_half, on_output));
    let (write_tx, write_rx) = tokio::sync::mpsc::unbounded_channel();
    tauri::async_runtime::spawn(write_forward(write_rx, write_half));
    ssh_trace!("[ssh] {key} pty/shell ready, read_loop spawned");

    // 登录脚本：启用时 shell 就绪后按行间隔自动发送（SSH 选项「连接 → 登录脚本」）
    if settings_flag("sshopt_script_enabled", false) {
        let content = settings_text("sshopt_script_content");
        let delay_ms = settings_number("sshopt_script_delay", 200).clamp(50, 10_000);
        let lines = login_script::parse(&content);
        if !lines.is_empty() {
            tauri::async_runtime::spawn(run_login_script(
                key.to_string(),
                write_tx.clone(),
                lines,
                delay_ms,
            ));
        }
    }

    // 注册会话句柄（断开时清理）；Handle 用 Arc 包裹以便跨 await 共享
    let handle = Arc::new(handle);
    state.ssh_sessions.lock().map_err(lock_err)?.insert(
        key.to_string(),
        SshSessionHandle {
            handle: handle.clone(),
            write_tx,
        },
    );

    // SSH 选项自启（连接成功后触发，不阻塞 connect 返回）：
    // ① 隧道自启：该会话启用中的规则逐个后台启动
    //    （已在运行 / 远程转发等错误静默跳过，不影响会话本身）
    if settings_flag("sshopt_tunnel_auto_start", true) {
        let rules = crate::services::tunnel::list_rules(Some(&cfg.id)).unwrap_or_default();
        for rule in rules.into_iter().filter(|r| r.enabled) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::services::tunnel::start_tunnel(&app, rule);
            });
        }
    }
    // ② 日志自启：开启该会话输出落盘（read_loop 未启用时 write_log 立即返回）
    if settings_flag("sshopt_log_auto_start", false) {
        let _ = crate::services::session_log::toggle(&cfg.id, true);
    }

    emit_status(app, key, STATUS_CONNECTED);
    Ok(())
}

// ---------------------------------------------------------------------------
// SSH 选项辅助：settings 表读取（失败容错为默认值，不阻塞连接）
// ---------------------------------------------------------------------------

/// 读布尔设置（缺省/失败回退 default）
fn settings_flag(key: &str, default: bool) -> bool {
    crate::services::settings_store::get(key)
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(default)
}

/// 读文本设置（缺省/失败回退空串）
fn settings_text(key: &str) -> String {
    crate::services::settings_store::get(key)
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 读数字设置（缺省/失败/非有限回退 default）
fn settings_number(key: &str, default: u64) -> u64 {
    crate::services::settings_store::get(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

/// 登录脚本任务：shell 就绪后按行间隔自动发送脚本命令。
/// 每行经既有写转发路径发送（与键盘输入同路），发送失败（会话关闭）即终止。
async fn run_login_script(
    key: String,
    write_tx: tokio::sync::mpsc::UnboundedSender<SshWriteMsg>,
    lines: Vec<login_script::ScriptLine>,
    delay_ms: u64,
) {
    let total = lines.len();
    for line in lines {
        if write_tx
            .send(SshWriteMsg::Data(line.send.into_bytes()))
            .is_err()
        {
            return; // 会话已关闭
        }
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    ssh_trace!("[ssh] {key} login script done ({total} lines)");
}

/// 断开并清理会话：从 ssh_sessions 移除句柄（触发连接关闭），
/// 同时清理可能挂起的 HostKey 确认与残留 oneshot。
pub fn disconnect(state: &AppState, id: &str) {
    // 取出句柄并发送优雅断开消息；Handle drop 后连接同样会终止
    let removed = state
        .ssh_sessions
        .lock()
        .map(|mut m| m.remove(id))
        .unwrap_or_default();
    if let Some(handle) = removed {
        tauri::async_runtime::spawn(async move {
            let _ = handle
                .handle
                .disconnect(Disconnect::ByApplication, "", "")
                .await;
        });
    }
    // 清理挂起的 HostKey 确认（若有），挂起点将因 Sender 被丢弃而中止
    if let Ok(mut pending) = state.pending_hostkey.lock() {
        pending.remove(id);
    }
}

/// 键盘输入写入（按会话 ID 路由）
pub fn write(state: &AppState, id: &str, data: &[u8]) -> Result<(), AppError> {
    let sessions = state.ssh_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::Ssh(format!("会话 {id} 不存在或已断开")))?;
    handle.send_msg(SshWriteMsg::Data(data.to_vec()))
}

/// 终端尺寸变更（按会话 ID 路由 resize）
pub fn resize(state: &AppState, id: &str, cols: u32, rows: u32) -> Result<(), AppError> {
    let sessions = state.ssh_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::Ssh(format!("会话 {id} 不存在或已断开")))?;
    handle.send_msg(SshWriteMsg::Resize { cols, rows })
}
