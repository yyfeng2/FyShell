//! SSH 服务封装：基于 russh 0.63 的客户端连接、5 种认证、HostKey 验证、
//! PTY/shell 请求、读循环与 keepalive。
//!
//! 架构红线：
//! - 终端输出 Rust 侧 4KB 批量读取再经 tauri::ipc::Channel 透传，ANSI 解析交给 xterm.js；
//! - 连接状态通过 `session-status` 事件推送（仅低频状态走 event）；
//! - 断开时同步清理 AppState::ssh_sessions 中的句柄。

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use russh::client::{AuthResult, DisconnectReason};
use russh::keys::known_hosts;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, load_secret_key};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, Disconnect};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

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

/// 写转发通道容量：有界缓冲，满时丢弃本批并告警（禁止无界增长导致内存膨胀）
const WRITE_CHANNEL_CAPACITY: usize = 1024;

/// HostKey 前端确认超时：超时中止连接，避免 oneshot 永久挂起
const HOSTKEY_CONFIRM_TIMEOUT: Duration = Duration::from_secs(15);

/// 进行中的 SSH 连接键集合（模块级静态，避免给 AppState 加字段）。
/// connect() 在真正建连前插入，建连结束（成功/失败）后移除——
/// SshSessionHandle 要等连接完成后才注册进 ssh_sessions，单纯查该表
/// 无法识别"正在建连"的中间态，并发触发同一会话会重复建连。
static CONNECTING_KEYS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

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

    /// 键盘输入消息队列，由独立写转发任务消费（同步接口下避免阻塞）。
    /// 有界缓冲（WRITE_CHANNEL_CAPACITY）：满时丢弃并告警，禁止无界增长——
    /// 键盘输入可丢（下一批自适应），但尺寸变更不可走此通道（见 resize_tx）。
    write_tx: tokio::sync::mpsc::Sender<SshWriteMsg>,

    /// 终端尺寸变更专用通道（独立无界）：尺寸消息「最新才有效、量级极低（拖窗口
    /// 经前端 50ms 防抖，每事件至多一条）」，且**绝对不可丢弃**——一旦被键盘输入
    /// 积压挤掉，PTY 列数将永久与 xterm 实际宽度错位，长命令行折行/覆盖（用户
    /// 实测 bug）再也无法自愈直到下次尺寸变化。无界在此安全，不会像键盘一样高频冲刷。
    resize_tx: tokio::sync::mpsc::UnboundedSender<SshWriteMsg>,
}

/// 写转发任务的消息：键盘输入或终端 resize
/// pub(crate)：zmodem 服务复用同一写路径回传协议字节
pub(crate) enum SshWriteMsg {
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

    /// 发送键盘输入消息（内部使用）。同步接口无法 async 等待，通道满时丢弃并告警，
    /// 保证不会无界积压；通道关闭（会话断开）才返回错误。尺寸变更请走 `send_resize`
    /// （专用无界通道，避免被键盘输入背压挤掉）。
    fn send_msg(&self, msg: SshWriteMsg) -> Result<(), AppError> {
        match self.write_tx.try_send(msg) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                Err(AppError::Ssh("会话已关闭，无法写入".into()))
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                // 写通道满（下游写入积压，如 SSH 窗口暂停/拥堵）：丢弃本批并告警
                eprintln!("[ssh] 写缓冲已满，输入被丢弃（下游写入积压）");
                Ok(())
            }
        }
    }

    /// 发送终端尺寸变更：走独立无界通道，绝对不因键盘输入积压而丢弃
    /// （丢弃会导致 PTY 列数与 xterm 实际宽度永久错位）。仅会话关闭时失败。
    fn send_resize(&self, cols: u32, rows: u32) -> Result<(), AppError> {
        self.resize_tx
            .send(SshWriteMsg::Resize { cols, rows })
            .map_err(|_| AppError::Ssh("会话已关闭，无法调整尺寸".into()))
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
        // 前端确认前挂起；Sender 被清理（如断开）视为拒绝。
        // 加超时防止 OOB 消息不匹配时永久挂起（前端窗口已关/事件丢失时挂死整个连接）
        let accepted = match tokio::time::timeout(HOSTKEY_CONFIRM_TIMEOUT, rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => false, // Sender 被清理（如断开）→ 视为拒绝
            Err(_) => {
                // 超时未收到前端确认：移除挂起点并返回错误，不再无限挂起
                if let Some(state) = self.app.try_state::<AppState>() {
                    if let Ok(mut pending) = state.pending_hostkey.lock() {
                        pending.remove(&self.session_id);
                    }
                }
                return Err(AppError::Ssh(format!(
                    "等待主机密钥确认超时（{}s 内未收到前端确认），连接已中止",
                    HOSTKEY_CONFIRM_TIMEOUT.as_secs()
                )));
            }
        };

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
            // 保险库已锁定时 auth_type 里是密文（解密容错原样透传）——连接前给出明确引导
            // 而非让密文走密码认证报"认证被拒绝"
            if crate::services::vault::is_ciphertext(password)
                && !crate::services::vault::is_unlocked()
            {
                return Err(AppError::Ssh(
                    "会话密码受主密码保护，请先在工具菜单解锁保险库后连接".into(),
                ));
            }
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
///
/// ZMODEM：输出流中出现 ZRQINIT 哨兵（**\x18B）时切到 ZMODEM 模式——
/// 输出整块改道到传输任务管道（不写 xterm），任务结束移除条目后恢复常规转发。
async fn read_loop(
    session_id: String,
    key: String,
    mut read_half: russh::ChannelReadHalf,
    on_output: Channel<Vec<u8>>,
    app: AppHandle,
    write_tx: tokio::sync::mpsc::Sender<SshWriteMsg>,
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
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => break,
            // 窗口调整等其他消息直接忽略
            Some(_) => {}
            // 空缓冲：流结束（对端断开）；非空缓冲：超时 → 冲刷积压输出
            None if buf.is_empty() => break,
            None => {}
        }

        if buf.is_empty() {
            continue;
        }

        // ZMODEM 传输中：输出整块改道到传输任务管道（不写 xterm/日志）。
        // 发送失败（任务刚好退出）时本块回落到常规终端输出（如传输完成后的提示符）
        if let Some(pipe_tx) = zmodem_pipe_tx(&app, &key) {
            let chunk = std::mem::take(&mut buf);
            match pipe_tx.send(chunk) {
                Ok(()) => continue,
                Err(e) => {
                    if on_output.send(e.0).is_err() {
                        break;
                    }
                }
            }
        }

        // 哨兵检测：输出流出现 ZRQINIT → 启动 ZMODEM 传输（sz/rz 公共起始）
        if let Some(pos) = find_zmodem_sentinel(&buf) {
            let tail = buf.split_off(pos);
            let head = std::mem::take(&mut buf);
            // 哨兵前内容（回显等）照常显示终端
            crate::services::session_log::write_log(&session_id, &head);
            if on_output.send(head).is_err() {
                break;
            }
            let state = app.state::<AppState>();
            crate::services::zmodem::start(&app, &state, &key, tail, write_tx.clone());
            continue;
        }

        // 常规冲刷：部分哨兵滞留尾部时扣留等下一批（防跨批漏检）
        let hold = partial_sentinel_len(&buf);
        let cut = buf.len() - hold;
        if cut == 0 {
            continue;
        }
        let chunk: Vec<u8> = buf.drain(..cut).collect();
        crate::services::session_log::write_log(&session_id, &chunk);
        if on_output.send(chunk).is_err() {
            eprintln!("[ssh] {} channel send failed", session_id);
            break;
        }
    }
    // 冲刷残留输出
    if !buf.is_empty() {
        crate::services::session_log::write_log(&session_id, &buf);
        let _ = on_output.send(buf);
    }
}

/// ZMODEM hex 帧头哨兵：ZPAD ZPAD ZDLE 'B'（sz/rz 输出帧的公共起始）
pub(crate) const ZMODEM_SENTINEL: [u8; 4] = [b'*', b'*', 0x18, b'B'];

/// 在缓冲区中查找 ZMODEM 哨兵的起始位置
pub(crate) fn find_zmodem_sentinel(buf: &[u8]) -> Option<usize> {
    buf.windows(ZMODEM_SENTINEL.len())
        .position(|w| w == ZMODEM_SENTINEL)
}

/// 缓冲区尾部是哨兵前缀（部分匹配）的最长字节数，无则 0
fn partial_sentinel_len(buf: &[u8]) -> usize {
    (1..ZMODEM_SENTINEL.len())
        .rev()
        .find(|&n| buf.ends_with(&ZMODEM_SENTINEL[..n]))
        .unwrap_or(0)
}

/// 查询当前会话的 ZMODEM 数据管道（传输进行中时 Some）
fn zmodem_pipe_tx(app: &AppHandle, key: &str) -> Option<std::sync::mpsc::Sender<Vec<u8>>> {
    let state = app.state::<AppState>();
    state
        .zmodem_sessions
        .lock()
        .ok()
        .and_then(|guard| guard.get(key).map(|entry| entry.pipe_tx.clone()))
}

/// 写转发任务：分别消费键盘输入（有界，满则丢）与终端尺寸变更（独立无界，必达）
/// 消息队列，转发到 russh channel。
/// 「最新才有效」的尺寸消息不可被键盘背压挤出（一旦挤掉 PTY 尺寸错位无法自愈），
/// 故用 tokio::select! 双通道并发 poll；两通道均关闭（会话句柄 drop 断开）才退出。
async fn write_forward(
    mut rx: tokio::sync::mpsc::Receiver<SshWriteMsg>,
    mut resize_rx: tokio::sync::mpsc::UnboundedReceiver<SshWriteMsg>,
    half: russh::ChannelWriteHalf<russh::client::Msg>,
) {
    let mut data_open = true;
    let mut resize_open = true;
    while data_open || resize_open {
        tokio::select! {
            msg = rx.recv(), if data_open => {
                match msg {
                    Some(SshWriteMsg::Data(data)) => {
                        // &[u8] 实现 AsyncRead；russh 内部按窗口/包大小分片
                        if half.data(&data[..]).await.is_err() {
                            break;
                        }
                    }
                    Some(SshWriteMsg::Resize { cols, rows }) => {
                        let _ = half.window_change(cols, rows, 0, 0).await;
                    }
                    None => data_open = false,
                }
            }
            resize = resize_rx.recv(), if resize_open => {
                match resize {
                    Some(SshWriteMsg::Resize { cols, rows }) => {
                        let _ = half.window_change(cols, rows, 0, 0).await;
                    }
                    _ => resize_open = false,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// CONNECTING_KEYS 的 RAII Drop 守卫：acquire 把 key 登记进「建连中」集合，
/// drop 时无论成功/失败/future 被 abort 一律移除——避免 connect_impl 的 future
/// 被 abort（会话切换/应用关闭等）后残留 key 永久拒绝该会话后续连接。
struct ConnectGuard(String);

impl ConnectGuard {
    /// 尝试登记建连标记；key 已在集合（并发重复建连）返回 None
    fn acquire(key: &str) -> Result<Option<Self>, AppError> {
        let mut connecting = CONNECTING_KEYS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .map_err(lock_err)?;
        if !connecting.insert(key.to_string()) {
            return Ok(None);
        }
        Ok(Some(ConnectGuard(key.to_string())))
    }
}

impl Drop for ConnectGuard {
    fn drop(&mut self) {
        // 锁中毒时静默跳过（与既有 lock 风格一致，避免 drop 中 panic）
        if let Ok(mut connecting) = CONNECTING_KEYS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
        {
            connecting.remove(&self.0);
        }
    }
}

/// 建立 SSH 连接（公开入口）：连接去重 + 记录建连进行中，实际步骤见 [`connect_impl`]。
/// key：连接路由键（多标签同会话独立连接时为每标签唯一，未传时等于会话 id）。
pub async fn connect(
    app: &tauri::AppHandle,
    state: &AppState,
    cfg: &SessionConfig,
    key: &str,
    on_output: tauri::ipc::Channel<Vec<u8>>,
) -> Result<(), AppError> {
    // 连接去重：同一路由键（未传时等于会话 id）已有连接进行中时拒绝重复建连。
    // SshSessionHandle 要到连接完成后才注册进 ssh_sessions，单纯查该表覆盖不了
    // "正在建连"的中间态（TCP/SOCKS 握手 + kex + HostKey 确认可能耗时数秒），
    // 并发触发同一会话会重复建立连接。建连标记用 RAII 守卫在真正建连前 acquire，
    // 守卫 drop 时（含 connect_impl 的 future 被 abort）自动移除。
    let _guard = match ConnectGuard::acquire(key)? {
        Some(guard) => guard,
        None => {
            return Err(AppError::Ssh(format!(
                "会话 {key} 正在连接中，请等待当前连接完成（勿重复连接）"
            )));
        }
    };
    connect_impl(app, state, cfg, key, on_output).await
}

/// 连接核心实现：TCP 连接 → HostKey 验证 → 认证 → PTY/shell → 读循环。
/// 输出流走 tauri::ipc::Channel（4KB 批量），连接状态通过 session-status 事件推送。
async fn connect_impl(
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
    // 有界写通道：下游写入积压（SSH 窗口暂停等）时满则丢批告警，禁止无界增长。
    // 仅承载键盘输入；尺寸变更走后独立无界通道 resize_rx（不可丢，见 send_resize）。
    let (write_tx, write_rx) = tokio::sync::mpsc::channel(WRITE_CHANNEL_CAPACITY);
    let (resize_tx, resize_rx) = tokio::sync::mpsc::unbounded_channel();
    tauri::async_runtime::spawn(write_forward(write_rx, resize_rx, write_half));
    tauri::async_runtime::spawn(read_loop(
        cfg.id.clone(),
        key.to_string(),
        read_half,
        on_output,
        app.clone(),
        write_tx.clone(),
    ));
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
            resize_tx,
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
    write_tx: tokio::sync::mpsc::Sender<SshWriteMsg>,
    lines: Vec<login_script::ScriptLine>,
    delay_ms: u64,
) {
    let total = lines.len();
    for line in lines {
        // 有界通道 async 发送：满时挂起等待（登录脚本低频，等待优于丢行）
        if write_tx
            .send(SshWriteMsg::Data(line.send.into_bytes()))
            .await
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
pub fn disconnect(state: &AppState, app: &tauri::AppHandle, id: &str) {
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
        // 应用主动断开不触发 Handler::disconnected（那只在远端断开/网络错误时回调），
        // 主动推送 disconnected，前端状态条依赖此事件把标签置灰
        emit_status(app, id, STATUS_DISCONNECTED);
    }
    // 清理挂起的 HostKey 确认（若有），挂起点将因 Sender 被丢弃而中止
    if let Ok(mut pending) = state.pending_hostkey.lock() {
        pending.remove(id);
    }
    // 清理进行中的 ZMODEM 传输（若有），任务经管道 Disconnected 退出并恢复终端
    if let Ok(mut zmodem) = state.zmodem_sessions.lock() {
        zmodem.remove(id);
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
/// 走专用无界通道（send_resize）：键盘输入积压不影响尺寸消息送达，
/// 保证 xterm 宽度变更必然同步到远端 PTY（长命令行折行/覆盖的根因修复）。
pub fn resize(state: &AppState, id: &str, cols: u32, rows: u32) -> Result<(), AppError> {
    let sessions = state.ssh_sessions.lock().map_err(lock_err)?;
    let handle = sessions
        .get(id)
        .ok_or_else(|| AppError::Ssh(format!("会话 {id} 不存在或已断开")))?;
    handle.send_resize(cols, rows)
}
