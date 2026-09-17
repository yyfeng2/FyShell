//! SSH 隧道转发服务（契约第 5.1 / 5.2 节）
//!
//! - 规则持久化：SQLite（同 fyshell.db，独立 Connection，模块级注册表 +
//!   `init(app_data_dir)` 注入，不触碰 AppState / lib.rs）
//! - Local：tokio TcpListener 监听 listen_port，每个入站连接经 SSH
//!   `open_direct_tcpip` 开通到 target_host:target_port 的通道，
//!   copy_bidirectional 双向转发
//! - Remote：P1 简化为记录配置，启动时返回明确错误"远程转发暂未实现"
//!   （russh server-side forward 留后续版本）
//! - Socks：本地最小 SOCKS5 服务端（仅 CONNECT 命令、无认证），
//!   目标地址经 SSH direct-tcpip 转发
//! - 启动/停止：模块级注册表管理监听任务，停止时关闭 listener 并
//!   终止全部转发任务；状态变更持久化并推送 `tunnel-status` 事件

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use rusqlite::{params, Connection};
use tauri::{Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

use crate::error::AppError;
use crate::models::tunnel::{TunnelKind, TunnelRule, TunnelStatus};
use crate::services::ssh::SshSessionHandle;
use crate::state::AppState;

/// 隧道状态事件名（仅低频状态走 event）
const EVENT_TUNNEL_STATUS: &str = "tunnel-status";

// ---------------------------------------------------------------------------
// 模块级注册表：运行时句柄 + SQLite 连接
// ---------------------------------------------------------------------------

/// 隧道运行时句柄注册表，key = 隧道 id。
/// 不修改 AppState / lib.rs，改用模块级注册表（OnceLock<Mutex<HashMap>>）模式。
static RUNTIMES: OnceLock<Mutex<HashMap<String, TunnelRuntime>>> = OnceLock::new();

/// SQLite 连接注册表：与 config_store 同用 fyshell.db，但持独立 Connection，
/// 避免与 ConfigStore 内部锁互相竞争。路径依赖 Tauri API（只能在应用启动后获取），
/// 故用模块级 OnceLock + init(app_data_dir) 注入。
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

/// 单条隧道的运行时句柄：
/// - accept 循环持有 `cancel_tx`（可为每个转发连接派生新的 watch Receiver）；
/// - stop() 发送 true 并移除注册表项后，accept 循环退出、listener 关闭，
///   全部转发任务的 `cancel_rx.changed()` 立即返回，连接随之丢弃。
struct TunnelRuntime {
    /// 应用句柄（stop 时持久化状态并推送事件用）
    app: tauri::AppHandle,
    /// 启动时的规则快照
    rule: TunnelRule,
    /// 停止信号：true = 停止；Sender 被 drop 后 changed() 同样立即返回
    cancel_tx: watch::Sender<bool>,
}

/// 锁住运行时注册表（Mutex 中毒视为不可恢复错误）
fn lock_runtimes() -> MutexGuard<'static, HashMap<String, TunnelRuntime>> {
    RUNTIMES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("tunnel 运行时注册表锁中毒")
}

/// 锁住 SQLite 连接（init 未调用视为编程错误）
fn lock_conn() -> MutexGuard<'static, Connection> {
    CONN.get()
        .expect("tunnel 数据库未初始化（init 未调用）")
        .lock()
        .expect("tunnel 数据库连接锁中毒")
}

/// 初始化：打开同 fyshell.db（独立 Connection）并完成建表迁移。
/// 由应用启动流程（setup / 集成层）调用一次；重复调用幂等。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    // 建库时自动创建目录
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    let cell = CONN.get_or_init(|| Mutex::new(conn));
    let guard = cell.lock().expect("tunnel 数据库连接锁中毒");
    guard.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS tunnels (
             id          TEXT PRIMARY KEY,
             session_id  TEXT NOT NULL,
             kind        TEXT NOT NULL,
             listen_host TEXT NOT NULL,
             listen_port INTEGER NOT NULL,
             target_host TEXT NOT NULL,
             target_port INTEGER NOT NULL,
             enabled     INTEGER NOT NULL,
             status      TEXT NOT NULL,
             error       TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_tunnels_session ON tunnels (session_id);
         COMMIT;",
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 规则持久化（SQLite CRUD）
// ---------------------------------------------------------------------------

/// 行 -> TunnelRule 的公共映射函数（kind / status 为字符串形式）
fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<TunnelRule> {
    let kind: String = row.get("kind")?;
    let status: String = row.get("status")?;
    Ok(TunnelRule {
        id: row.get("id")?,
        session_id: row.get("session_id")?,
        kind: TunnelKind::parse(&kind),
        listen_host: row.get("listen_host")?,
        listen_port: {
            let p: i64 = row.get("listen_port")?;
            p as u16
        },
        target_host: row.get("target_host")?,
        target_port: {
            let p: i64 = row.get("target_port")?;
            p as u16
        },
        enabled: {
            let e: i64 = row.get("enabled")?;
            e != 0
        },
        status: TunnelStatus::parse(&status),
        error: row.get("error")?,
    })
}

/// 隧道规则列表：`session_id` 非空时仅返回该会话的规则
pub fn list_rules(session_id: Option<&str>) -> Result<Vec<TunnelRule>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, kind, listen_host, listen_port, target_host, target_port,
                enabled, status, error
         FROM tunnels
         WHERE (?1 IS NULL OR session_id = ?1)
         ORDER BY listen_port",
    )?;
    let rules: Vec<TunnelRule> = stmt
        .query_map([session_id], row_to_rule)?
        .collect::<Result<_, _>>()?;
    Ok(rules)
}

/// 按 id 读取单条规则；不存在返回 Ok(None)
pub fn get_rule(id: &str) -> Result<Option<TunnelRule>, AppError> {
    let conn = lock_conn();
    let mut stmt = conn.prepare_cached(
        "SELECT id, session_id, kind, listen_host, listen_port, target_host, target_port,
                enabled, status, error
         FROM tunnels WHERE id = ?1",
    )?;
    let mut rows = stmt.query_map([id], row_to_rule)?;
    Ok(rows.next().transpose()?)
}

/// 保存规则（按 id upsert；新规则的 id 由 commands 层生成后传入）
pub fn save_rule(rule: &TunnelRule) -> Result<(), AppError> {
    // 参数校验（薄校验留在服务层，commands 层保持更薄）
    if rule.listen_port == 0 {
        return Err(AppError::general("监听端口不能为 0"));
    }
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO tunnels (id, session_id, kind, listen_host, listen_port,
                              target_host, target_port, enabled, status, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET
             session_id = excluded.session_id,
             kind = excluded.kind,
             listen_host = excluded.listen_host,
             listen_port = excluded.listen_port,
             target_host = excluded.target_host,
             target_port = excluded.target_port,
             enabled = excluded.enabled,
             status = excluded.status,
             error = excluded.error",
        params![
            rule.id,
            rule.session_id,
            rule.kind.as_str(),
            rule.listen_host,
            rule.listen_port as i64,
            rule.target_host,
            rule.target_port as i64,
            rule.enabled as i64,
            rule.status.as_str(),
            rule.error,
        ],
    )?;
    Ok(())
}

/// 删除单条规则（调用方须先停止运行中的隧道）
pub fn delete_rule(id: &str) -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute("DELETE FROM tunnels WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 状态更新（持久化 + tunnel-status 事件）
// ---------------------------------------------------------------------------

/// tunnel-status 事件 payload
#[derive(Clone, serde::Serialize)]
struct TunnelStatusPayload {
    id: String,
    status: TunnelStatus,
    error: Option<String>,
}

/// 更新隧道状态：持久化到 SQLite；`app` 为 Some 时同时推送 tunnel-status 事件
fn set_status(
    app: Option<&tauri::AppHandle>,
    id: &str,
    status: TunnelStatus,
    error: Option<String>,
) -> Result<(), AppError> {
    {
        let conn = lock_conn();
        conn.execute(
            "UPDATE tunnels SET status = ?1, error = ?2 WHERE id = ?3",
            params![status.as_str(), error, id],
        )?;
    }
    if let Some(app) = app {
        let _ = app.emit(
            EVENT_TUNNEL_STATUS,
            TunnelStatusPayload {
                id: id.to_string(),
                status,
                error,
            },
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 公开 API：启动 / 停止
// ---------------------------------------------------------------------------

/// 启动隧道监听：同步绑定端口（绑定失败同步返回，前端即时可见），
/// 注册运行时句柄后派生 accept 循环。
pub fn start_tunnel(app: &tauri::AppHandle, rule: TunnelRule) -> Result<(), AppError> {
    // 远程转发：P1 简化为记录配置，启动时返回明确错误
    if rule.kind == TunnelKind::Remote {
        return Err(AppError::general(
            "远程转发（Remote）暂未实现，请使用本地转发或 SOCKS5",
        ));
    }
    // 防止重复启动
    if lock_runtimes().contains_key(&rule.id) {
        return Err(AppError::general(format!("隧道 {} 已在运行中", rule.id)));
    }

    // 同步绑定监听端口：bind 为亚毫秒级阻塞，绑定失败同步返回错误
    let std_listener =
        std::net::TcpListener::bind((rule.listen_host.as_str(), rule.listen_port))?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    // 注册运行时（bind 成功后再入表，缩小与防重检查之间的竞态窗口）
    // cancel_tx.clone()：accept 循环还需为每个转发连接派生新的 watch Receiver
    let entry = TunnelRuntime {
        app: app.clone(),
        rule: rule.clone(),
        cancel_tx: cancel_tx.clone(),
    };
    lock_runtimes().insert(rule.id.clone(), entry);

    // 状态 Listening：持久化 + 推送
    set_status(Some(app), &rule.id, TunnelStatus::Listening, None)?;

    // 启动 accept 循环（转发任务在循环内按连接派生）
    let socks = rule.kind == TunnelKind::Socks;
    tauri::async_runtime::spawn(accept_loop(
        app.clone(),
        rule.clone(),
        listener,
        cancel_tx.clone(),
        cancel_rx,
        socks,
    ));
    Ok(())
}

/// 停止隧道：关闭 listener 与全部转发任务，状态持久化为 Stopped 并推送。
/// 未运行的隧道：校验规则存在后幂等置为 Stopped（仅持久化，不推送）。
pub fn stop(id: &str) -> Result<(), AppError> {
    // 取出运行时句柄：广播停止信号后，accept 循环退出关闭 listener，
    // 全部转发任务的 cancel_rx.changed() 立即返回、连接被丢弃
    let runtime = lock_runtimes().remove(id);
    if let Some(rt) = runtime {
        let _ = rt.cancel_tx.send(true);
        set_status(Some(&rt.app), id, TunnelStatus::Stopped, None)?;
    } else {
        // 未运行：校验规则存在（不存在的 id 报错），幂等置为 Stopped
        if get_rule(id)?.is_none() {
            return Err(AppError::general(format!("隧道 {id} 不存在")));
        }
        set_status(None, id, TunnelStatus::Stopped, None)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// accept 循环与转发任务
// ---------------------------------------------------------------------------

/// accept 循环：监听入站连接并按连接派生转发任务；
/// 停止信号到达或运行时句柄被移除时退出循环（listener 随之关闭）。
async fn accept_loop(
    app: tauri::AppHandle,
    rule: TunnelRule,
    listener: tokio::net::TcpListener,
    cancel_tx: watch::Sender<bool>,
    mut cancel_rx: watch::Receiver<bool>,
    socks: bool,
) {
    loop {
        tokio::select! {
            // 停止信号 / Sender 被 drop（stop 已执行）：退出循环关闭 listener
            _ = cancel_rx.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let app = app.clone();
                    let rule = rule.clone();
                    let mut rx = cancel_tx.subscribe();
                    tauri::async_runtime::spawn(async move {
                        // select 停止信号：隧道停止时立即丢弃该连接
                        tokio::select! {
                            _ = rx.changed() => {}
                            _ = forward_one(&app, &rule, stream, peer, socks) => {}
                        }
                    });                }
                Err(e) => {
                    // 监听器异常：置 Error 状态并退出（停止接受新连接）
                    let _ = set_status(
                        Some(&app),
                        &rule.id,
                        TunnelStatus::Error,
                        Some(format!("监听异常终止: {e}")),
                    );
                    break;
                }
            },
        }
    }
}

/// 单连接转发分发：Local 直连目标 / Socks 走 SOCKS5 握手
async fn forward_one(
    app: &tauri::AppHandle,
    rule: &TunnelRule,
    local: tokio::net::TcpStream,
    peer: SocketAddr,
    socks: bool,
) {
    if socks {
        socks5_connection(app, rule, local, peer).await;
    } else {
        local_forward_connection(app, rule, local, peer).await;
    }
}

/// 按 session_id 从全局状态取 SSH 会话句柄（只读 AppState，不修改）
fn session_handle(app: &tauri::AppHandle, session_id: &str) -> Option<SshSessionHandle> {
    let state = app.try_state::<AppState>()?;
    let sessions = state.ssh_sessions.lock().ok()?;
    sessions.get(session_id).cloned()
}

/// Local 转发单连接：经 SSH direct-tcpip 开通到 target 的通道后双向转发。
/// SSH 会话未连接 / 目标不可达等瞬态错误仅关闭该入站连接，不影响隧道监听。
async fn local_forward_connection(
    app: &tauri::AppHandle,
    rule: &TunnelRule,
    mut local: tokio::net::TcpStream,
    peer: SocketAddr,
) {
    let Some(handle) = session_handle(app, &rule.session_id) else {
        return; // SSH 会话未连接：直接关闭入站连接
    };
    // 开通 direct-tcpip 通道（originator 取本地入站连接的对端地址）
    let mut remote = match handle
        .open_direct_tcpip(
            &rule.target_host,
            rule.target_port,
            &peer.ip().to_string(),
            peer.port(),
        )
        .await
    {
        Ok(channel) => channel.into_stream(),
        Err(_) => return, // 瞬态错误仅关闭该连接
    };
    // 双向转发：任一侧关闭/出错即结束
    let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
}

/// SOCKS5 转发单连接（最小实现：仅 CONNECT 命令、无认证），
/// 目标地址经 SSH direct-tcpip 转发。
async fn socks5_connection(
    app: &tauri::AppHandle,
    rule: &TunnelRule,
    mut local: tokio::net::TcpStream,
    peer: SocketAddr,
) {
    // 阶段 1：方法协商，统一回复无认证（VER=0x05 METHOD=0x00）
    let mut header = [0u8; 2];
    if local.read_exact(&mut header).await.is_err() {
        return;
    }
    if header[0] != 0x05 {
        return; // 非 SOCKS5 协议，直接断开
    }
    let mut methods = vec![0u8; header[1] as usize];
    if local.read_exact(&mut methods).await.is_err() {
        return;
    }
    if local.write_all(&[0x05, 0x00]).await.is_err() {
        return;
    }

    // 阶段 2：请求解析（VER CMD RSV ATYP ADDR PORT）
    let mut req = [0u8; 4];
    if local.read_exact(&mut req).await.is_err() {
        return;
    }
    let (ver, cmd, _rsv, atyp) = (req[0], req[1], req[2], req[3]);
    if ver != 0x05 {
        return;
    }
    // 仅支持 CONNECT（0x01）；不支持时回复 0x07（command not supported）
    if cmd != 0x01 {
        let _ = local
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await;
        return;
    }
    let target_host = match atyp {
        0x01 => {
            // IPv4
            let mut buf = [0u8; 4];
            if local.read_exact(&mut buf).await.is_err() {
                return;
            }
            Ipv4Addr::from(buf).to_string()
        }
        0x03 => {
            // 域名：1 字节长度前缀 + 域名
            let mut len = [0u8; 1];
            if local.read_exact(&mut len).await.is_err() {
                return;
            }
            let mut buf = vec![0u8; len[0] as usize];
            if local.read_exact(&mut buf).await.is_err() {
                return;
            }
            String::from_utf8(buf).unwrap_or_default()
        }
        0x04 => {
            // IPv6
            let mut buf = [0u8; 16];
            if local.read_exact(&mut buf).await.is_err() {
                return;
            }
            Ipv6Addr::from(buf).to_string()
        }
        _ => {
            // 不支持的地址类型：回复 0x08（address type not supported）
            let _ = local
                .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return;
        }
    };
    let mut port_buf = [0u8; 2];
    if local.read_exact(&mut port_buf).await.is_err() {
        return;
    }
    let target_port = u16::from_be_bytes(port_buf);

    let Some(handle) = session_handle(app, &rule.session_id) else {
        return; // SSH 会话未连接：直接关闭入站连接
    };
    // 目标地址经 SSH direct-tcpip 转发；失败回复 general SOCKS server failure（0x01）
    let mut remote = match handle
        .open_direct_tcpip(&target_host, target_port, &peer.ip().to_string(), peer.port())
        .await
    {
        Ok(channel) => channel.into_stream(),
        Err(_) => {
            let _ = local
                .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return;
        }
    };
    // 回复连接成功（bound address 用 0.0.0.0:0 占位）
    if local
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .is_err()
    {
        return;
    }
    // 双向转发：任一侧关闭/出错即结束
    let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
}
