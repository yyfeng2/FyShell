//! SSH 连接代理（SSH 选项对话框「连接 → 代理」）
//!
//! 经 SOCKS5（RFC 1928，可选 RFC 1929 用户名/密码子协商）或 HTTP CONNECT 代理
//! 手工握手建立 tokio TcpStream，再交给 russh `client::connect_stream` 完成 SSH 握手
//! （russh 0.63 的 `connect` 仅支持直连，代理场景必须自带流）。

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::AppError;

/// 代理类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    /// SOCKS5 代理
    Socks5,
    /// HTTP CONNECT 代理
    Http,
}

/// 代理配置（从 settings 表解析）
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyConfig {
    /// 从 settings 表读取代理配置；未启用或 host 为空返回 None
    pub fn from_settings() -> Result<Option<ProxyConfig>, AppError> {
        let enabled = crate::services::settings_store::get("sshopt_proxy_enabled")?
            .map(|v| v == "true")
            .unwrap_or(false);
        if !enabled {
            return Ok(None);
        }
        let host = crate::services::settings_store::get("sshopt_proxy_host")?
            .unwrap_or_default()
            .trim()
            .to_string();
        if host.is_empty() {
            return Ok(None);
        }
        let kind = match crate::services::settings_store::get("sshopt_proxy_type")?.as_deref() {
            Some("http") => ProxyKind::Http,
            _ => ProxyKind::Socks5,
        };
        let port = crate::services::settings_store::get("sshopt_proxy_port")?
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(1080);
        let username = crate::services::settings_store::get("sshopt_proxy_username")?
            .filter(|v| !v.is_empty());
        let password = crate::services::settings_store::get("sshopt_proxy_password")?
            .filter(|v| !v.is_empty());
        Ok(Some(ProxyConfig {
            kind,
            host,
            port,
            username,
            password,
        }))
    }
}

/// 经代理建立到 `target_host:target_port` 的 TCP 流
pub async fn connect(
    target_host: &str,
    target_port: u16,
    proxy: &ProxyConfig,
) -> Result<tokio::net::TcpStream, AppError> {
    match proxy.kind {
        ProxyKind::Socks5 => connect_socks5(target_host, target_port, proxy).await,
        ProxyKind::Http => connect_http(target_host, target_port, proxy).await,
    }
}

/// SOCKS5 连接：greeting →（可选认证子协商）→ CONNECT → 回应校验
async fn connect_socks5(
    target_host: &str,
    target_port: u16,
    proxy: &ProxyConfig,
) -> Result<tokio::net::TcpStream, AppError> {
    let mut stream = tokio::net::TcpStream::connect((proxy.host.as_str(), proxy.port))
        .await
        .map_err(|e| AppError::Ssh(format!("连接代理服务器失败: {e}")))?;

    // greeting：支持无认证（0x00）与用户名/密码（0x02）
    let auth = if proxy.username.is_some() { 0x02u8 } else { 0x00u8 };
    let mut buf = [0u8; 2];
    read_exact(&mut stream, &mut buf).await?;
    if buf[0] != 0x05 {
        return Err(AppError::Ssh("SOCKS5 握手失败：非 SOCKS5 代理".into()));
    }
    if buf[1] != auth {
        return Err(AppError::Ssh(format!(
            "SOCKS5 代理拒绝了所选认证方式（要求 0x{auth:02x}）"
        )));
    }

    // RFC 1929 用户名/密码子协商
    if auth == 0x02 {
        let username = proxy.username.clone().unwrap_or_default();
        let password = proxy.password.clone().unwrap_or_default();
        let mut req = Vec::with_capacity(3 + username.len() + password.len());
        req.push(0x01);
        req.push(username.len() as u8);
        req.extend_from_slice(username.as_bytes());
        req.push(password.len() as u8);
        req.extend_from_slice(password.as_bytes());
        stream
            .write_all(&req)
            .await
            .map_err(socks_io_err("发送认证请求失败"))?;
        read_exact(&mut stream, &mut buf).await?;
        if buf[1] != 0x00 {
            return Err(AppError::Ssh("SOCKS5 代理认证失败（用户名或密码错误）".into()));
        }
    }

    // CONNECT 请求：域名模式（0x03），地址交由代理端解析
    let mut req = Vec::with_capacity(7 + target_host.len());
    req.extend_from_slice(&[0x05, 0x01, 0x00]);
    req.push(0x03);
    req.push(target_host.len() as u8);
    req.extend_from_slice(target_host.as_bytes());
    req.extend_from_slice(&target_port.to_be_bytes());
    stream
        .write_all(&req)
        .await
        .map_err(socks_io_err("发送 CONNECT 请求失败"))?;

    // 回应：VER REP RSV ATYP ADDR...（地址部分长度随 ATYP 变化，全部读掉）
    let mut head = [0u8; 5];
    read_exact(&mut stream, &mut head).await?;
    if head[1] != 0x00 {
        return Err(AppError::Ssh(format!(
            "SOCKS5 代理连接失败（回应码 0x{:02x}）",
            head[1]
        )));
    }
    let addr_len = match head[3] {
        0x01 => 4,  // IPv4
        0x03 => head[4] as usize + 2, // 域名（1 字节长度 + 域名）
        0x04 => 16, // IPv6
        _ => return Err(AppError::Ssh("SOCKS5 回应地址类型未知".into())),
    };
    let mut addr = vec![0u8; addr_len];
    read_exact(&mut stream, &mut addr).await?;
    Ok(stream)
}

/// HTTP CONNECT 代理：发送 CONNECT 请求，读到响应头结束并校验状态行
async fn connect_http(
    target_host: &str,
    target_port: u16,
    proxy: &ProxyConfig,
) -> Result<tokio::net::TcpStream, AppError> {
    let mut stream = tokio::net::TcpStream::connect((proxy.host.as_str(), proxy.port))
        .await
        .map_err(|e| AppError::Ssh(format!("连接代理服务器失败: {e}")))?;

    let mut req = format!("CONNECT {target_host}:{target_port} HTTP/1.1\r\nHost: {target_host}:{target_port}\r\n");
    if let (Some(user), Some(pass)) = (&proxy.username, &proxy.password) {
        let credentials = base64_encode(format!("{user}:{pass}").as_bytes());
        req.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(socks_io_err("发送 CONNECT 请求失败"))?;

    // 读到空行（响应头结束）为止，上限 16KB 防御异常代理
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if read_exact(&mut stream, &mut byte).await.is_err() {
            return Err(AppError::Ssh("HTTP CONNECT 代理响应不完整".into()));
        }
        response.push(byte[0]);
        if response.len() > 16 * 1024 {
            return Err(AppError::Ssh("HTTP CONNECT 代理响应头过大".into()));
        }
        if response.ends_with(b"\r\n\r\n") || response.ends_with(b"\n\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&response);
    if !head.starts_with("HTTP/") || !head.split('\r').next().unwrap_or("").contains(" 200") {
        return Err(AppError::Ssh(format!(
            "HTTP CONNECT 代理连接失败：{}",
            head.split("\r\n").next().unwrap_or("未知响应")
        )));
    }
    Ok(stream)
}

/// 读满缓冲（代理握手各阶段均为短小定长读取，失败统一报错）
async fn read_exact<S: AsyncReadExt + Unpin>(
    stream: &mut S,
    buf: &mut [u8],
) -> Result<(), AppError> {
    stream
        .read_exact(buf)
        .await
        .map(|_| ())
        .map_err(socks_io_err("代理握手读取失败"))
}

/// 代理 IO 错误统一包装（闭包返回固定前缀，避免 async fn 中多次 match）
fn socks_io_err(
    prefix: &'static str,
) -> impl Fn(std::io::Error) -> AppError {
    move |e| AppError::Ssh(format!("{prefix}: {e}"))
}

/// 最小 base64 编码（标准字母表 + '=' 填充，供 Proxy-Authorization 使用）
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) as usize & 0x3f] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple as usize & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}
