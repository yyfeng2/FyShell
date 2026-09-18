//! Redis 服务：薄层 RESP2 客户端（tokio TcpStream，无 redis crate 依赖）
//!
//! 连接注册表保存在 Rust 内存（凭据红线：不落盘、不进前端）。
//! 命令往返以 tokio::time::timeout 包裹，避免服务端无响应时挂死。

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::models::redis::{RedisConnection, RedisExecResult};

/// 命令往返超时（秒）
const TIMEOUT: Duration = Duration::from_secs(10);

/// 连接注册表：conn_id -> 连接（全局 Mutex，桌面客户端串行执行足够）
static CONNS: OnceLock<Mutex<HashMap<String, BufReader<TcpStream>>>> = OnceLock::new();
/// 自增连接 id
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn conns() -> &'static Mutex<HashMap<String, BufReader<TcpStream>>> {
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 模块级注册表初始化（setup 时显式调用，与其它服务 init 模式一致）
pub fn init() {
    CONNS.get_or_init(|| Mutex::new(HashMap::new()));
}

/// RESP2 回复
#[derive(Debug)]
enum Reply {
    Status(String),
    Error(String),
    Integer(i64),
    Bulk(Option<Vec<u8>>),
    Array(Vec<Reply>),
}

/// 建立新连接并按需 AUTH / SELECT db，存入注册表
pub async fn connect(config: &RedisConnection) -> Result<String, AppError> {
    let mut conn = open(config).await?;
    auth(&mut conn, config).await?;
    if config.db != 0 {
        let args = vec!["SELECT".to_string(), config.db.to_string()];
        let reply = exec_args(&mut conn, args).await?;
        check_ok(&reply, "SELECT")?;
    }

    let id = format!("redis-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    conns().lock().await.insert(id.clone(), conn);
    Ok(id)
}

/// 断开并移除注册表连接（不存在视为已断开）
pub async fn disconnect(conn_id: &str) -> Result<(), AppError> {
    conns().lock().await.remove(conn_id);
    Ok(())
}

/// 连接测试：返回服务器版本摘要
pub async fn test(config: &RedisConnection) -> Result<String, AppError> {
    let mut conn = open(config).await?;
    auth(&mut conn, config).await?;

    let reply = exec_args(&mut conn, vec!["PING".to_string()]).await?;
    check_ok(&reply, "PING")?;
    // INFO server 提取 redis_version（失败不影响测试结论）
    let version = match exec_args(&mut conn, vec!["INFO".to_string(), "server".to_string()]).await {
        Ok(Reply::Bulk(Some(bytes))) => String::from_utf8_lossy(&bytes)
            .lines()
            .find_map(|l| l.strip_prefix("redis_version:"))
            .map(str::to_string)
            .unwrap_or_default(),
        _ => String::new(),
    };
    if version.is_empty() {
        Ok(format!("连接成功 @ {}:{}", config.host, config.port))
    } else {
        Ok(format!("连接成功 @ {}:{} (Redis {})", config.host, config.port, version))
    }
}

/// INFO 命令：解析为键值对（跳过注释行与空行）
pub async fn info(conn_id: &str) -> Result<Vec<(String, String)>, AppError> {
    let args = vec!["INFO".to_string()];
    let mut map = conns().lock().await;
    let conn = get_conn(&mut map, conn_id)?;
    let reply = timeout(exec_args(conn, args)).await?;
    let bytes = match reply {
        Reply::Bulk(Some(b)) => b,
        _ => return Err(AppError::general("INFO 返回了意外格式")),
    };
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect())
}

/// 切换库
pub async fn select_db(conn_id: &str, db: u8) -> Result<(), AppError> {
    let args = vec!["SELECT".to_string(), db.to_string()];
    let mut map = conns().lock().await;
    let conn = get_conn(&mut map, conn_id)?;
    let reply = timeout(exec_args(conn, args)).await?;
    check_ok(&reply, "SELECT")
}

/// KEYS pattern：返回匹配的键列表
pub async fn keys(conn_id: &str, pattern: &str) -> Result<Vec<String>, AppError> {
    let args = vec!["KEYS".to_string(), pattern.to_string()];
    let mut map = conns().lock().await;
    let conn = get_conn(&mut map, conn_id)?;
    let reply = timeout(exec_args(conn, args)).await?;
    match reply {
        Reply::Array(items) => Ok(items
            .into_iter()
            .filter_map(|r| match r {
                Reply::Bulk(Some(b)) => Some(String::from_utf8_lossy(&b).into_owned()),
                _ => None,
            })
            .collect()),
        _ => Err(AppError::general("KEYS 返回了意外格式")),
    }
}

/// 任意命令执行：按回复形状转换为前端可渲染的 RedisExecResult
pub async fn exec(conn_id: &str, args: &[String]) -> Result<RedisExecResult, AppError> {
    if args.is_empty() {
        return Err(AppError::general("命令不能为空"));
    }
    let mut map = conns().lock().await;
    let conn = get_conn(&mut map, conn_id)?;
    let reply = timeout(exec_args(conn, args.to_vec())).await?;
    Ok(match reply {
        Reply::Status(s) => RedisExecResult { kind: "status".into(), value: serde_json::json!(s) },
        Reply::Error(s) => RedisExecResult { kind: "error".into(), value: serde_json::json!(s) },
        Reply::Integer(i) => RedisExecResult { kind: "int".into(), value: serde_json::json!(i) },
        Reply::Bulk(None) => RedisExecResult { kind: "nil".into(), value: serde_json::Value::Null },
        Reply::Bulk(Some(b)) => match String::from_utf8(b) {
            Ok(s) => RedisExecResult { kind: "string".into(), value: serde_json::json!(s) },
            Err(bytes) => {
                // 二进制内容转 hex 字符串（无 base64 依赖）
                let hex: String = bytes.into_bytes().iter().map(|b| format!("{b:02x}")).collect();
                RedisExecResult { kind: "blob".into(), value: serde_json::json!(hex) }
            }
        },
        Reply::Array(items) => {
            let arr: Vec<serde_json::Value> = items
                .into_iter()
                .map(|r| match r {
                    Reply::Bulk(Some(b)) => serde_json::json!(String::from_utf8_lossy(&b)),
                    Reply::Integer(i) => serde_json::json!(i),
                    Reply::Status(s) => serde_json::json!(s),
                    Reply::Error(s) => serde_json::json!(s),
                    Reply::Bulk(None) => serde_json::Value::Null,
                    Reply::Array(_) => serde_json::Value::Null,
                })
                .collect();
            RedisExecResult { kind: "array".into(), value: serde_json::json!(arr) }
        }
    })
}

/// 按需 AUTH（ACL 用户存在时 AUTH user pass，否则 AUTH pass）
async fn auth(conn: &mut BufReader<TcpStream>, config: &RedisConnection) -> Result<(), AppError> {
    let Some(password) = config.password.as_ref().filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    let reply = match config.username.as_ref().filter(|u| !u.is_empty()) {
        Some(user) => exec_args(conn, vec!["AUTH".into(), user.clone(), password.clone()]).await?,
        None => exec_args(conn, vec!["AUTH".into(), password.clone()]).await?,
    };
    check_ok(&reply, "AUTH")
}

/// 新建裸连接（TCP 连接，无 AUTH/SELECT）
async fn open(config: &RedisConnection) -> Result<BufReader<TcpStream>, AppError> {
    let addr = (config.host.as_str(), config.port);
    let stream = TcpStream::connect(addr).await?;
    Ok(BufReader::new(stream))
}

/// 从注册表取连接（不存在报错）
fn get_conn<'a>(
    map: &'a mut HashMap<String, BufReader<TcpStream>>,
    conn_id: &str,
) -> Result<&'a mut BufReader<TcpStream>, AppError> {
    map.get_mut(conn_id)
        .ok_or_else(|| AppError::general(format!("连接不存在或已断开: {conn_id}")))
}

/// 命令往返超时包裹（future 输出为 Result，超时错误与命令错误统一透传）
async fn timeout<T>(fut: impl Future<Output = Result<T, AppError>>) -> Result<T, AppError> {
    tokio::time::timeout(TIMEOUT, fut)
        .await
        .map_err(|_| AppError::general(format!("命令超时（{}s）", TIMEOUT.as_secs())))?
}

/// 编码 RESP2 请求并读取回复
async fn exec_args(conn: &mut BufReader<TcpStream>, args: Vec<String>) -> Result<Reply, AppError> {
    let mut req = format!("*{}\r\n", args.len());
    for arg in &args {
        req.push_str(&format!("${}\r\n", arg.len()));
        req.push_str(arg);
        req.push_str("\r\n");
    }
    conn.write_all(req.as_bytes()).await?;
    conn.flush().await?;
    read_reply(conn).await
}

/// 读取并解析一个 RESP2 回复
async fn read_reply(conn: &mut BufReader<TcpStream>) -> Result<Reply, AppError> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    // 读类型标记字节（+ - : $ *）
    conn.read_exact(&mut byte).await?;
    let kind = byte[0];
    // 读行剩余部分（含 CRLF，存内容部分）
    loop {
        let mut ch = [0u8; 1];
        conn.read_exact(&mut ch).await?;
        if ch[0] == b'\n' {
            break;
        }
        if ch[0] != b'\r' {
            buf.push(ch[0]);
        }
    }
    let line = String::from_utf8_lossy(&buf).into_owned();
    Ok(match kind {
        b'+' => Reply::Status(line),
        b'-' => Reply::Error(line),
        b':' => Reply::Integer(line.parse().map_err(|_| AppError::general(format!("整数解析失败: {line}")))?),
        b'$' => {
            let len: i64 = line
                .parse()
                .map_err(|_| AppError::general(format!("Bulk 长度解析失败: {line}")))?;
            if len < 0 {
                Reply::Bulk(None)
            } else {
                let mut data = vec![0u8; len as usize + 2]; // 含结尾 CRLF
                conn.read_exact(&mut data).await?;
                data.truncate(len as usize);
                Reply::Bulk(Some(data))
            }
        }
        b'*' => {
            let count: i64 = line
                .parse()
                .map_err(|_| AppError::general(format!("数组长度解析失败: {line}")))?;
            if count < 0 {
                Reply::Array(Vec::new())
            } else {
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(Box::pin(read_reply(conn)).await?);
                }
                Reply::Array(items)
            }
        }
        other => return Err(AppError::general(format!("未知 RESP 类型: {}", other as char))),
    })
}

/// 校验 Status 回复为 +OK，否则报错
fn check_ok(reply: &Reply, op: &str) -> Result<(), AppError> {
    match reply {
        Reply::Status(s) if s.eq_ignore_ascii_case("OK") => Ok(()),
        Reply::Error(s) => Err(AppError::general(format!("{op} 失败: {s}"))),
        other => Err(AppError::general(format!("{op} 失败: 意外回复 {other:?}"))),
    }
}
