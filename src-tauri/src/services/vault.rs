//! 凭据保险库：主密码保护已存连接等明文凭据（信封式加密）。
//!
//! 设计（对齐架构红线 §1.3「凭据仅存 Rust 侧 SQLite，不进 WebView」）：
//! - **DEK**（Data Encryption Key，32 字节随机）实际加密数据；明文只存 Rust 内存
//!   （解锁后），落盘与跨桥的一律为被 KEK 包裹的密文，从不上传到前端
//! - **KEK**（Key Encryption Key）= PBKDF2-HMAC-SHA256(主密码, salt, 20 万轮)；
//!   信任根是用户主密码。换主密码 = 用新 KEK 重包同一 DEK（数据密钥不变，
//!   已存密文无需迁移）
//! - **AEAD** = ChaCha20-Poly1305（加密数据明文 / 包裹 DEK）
//! - 存储：复用 fyshell.db 的 meta 表（config_store 同库，本模块独立 Connection，
//!   参照 mysql_console 模式，不触碰 AppState）
//!
//! 交互路径：
//! - 未设置主密码：调用方沿用既有明文存储（vault 不介入）
//! - `master_password_set`（首次）：rekey() 生成 DEK 并注册信封，此后保存凭据转加密
//! - `master_password_set`（改密）：命令层先 rekey_migrate(old, new)（校验旧密码 +
//!   迁移 DEK 信封），成功后再覆盖 FNV 哈希
//! - `vault_unlock`：以主密码解包 DEK 入内存；应用重启后需再次解锁
//! - `vault_encrypt/vault_decrypt`：未解锁时返回明确错误，由前端引导解锁

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use chacha20poly1305::aead::{Aead, KeyInit, Nonce};
use chacha20poly1305::ChaCha20Poly1305;
use pbkdf2::pbkdf2_hmac;
use rusqlite::{params, Connection};
use sha2::Sha256;

use crate::error::AppError;

/// 数据加密密钥长度 / KEK 长度（AES-256 等价强度）
const KEY_LEN: usize = 32;
/// 主密码盐长度（PBKDF2 输入盐）
const SALT_LEN: usize = 16;
/// AEAD nonce 长度（ChaCha20-Poly1305 标准 12 字节）
const NONCE_LEN: usize = 12;
/// v1 历史信封固定轮数：200_000（v1 信封无 rounds 字段，解开存量密文的唯一
/// 派生依据就是它——**严禁对 v1 密文改用新默认轮数**，否则派生 KEK 不符，
/// AEAD 认证整体失败、所有已存密文不可解。此常量只读永不改）
const PBKDF2_ROUNDS_V1_LEGACY: u32 = 200_000;
/// v2 信封默认迭代轮数（由 v1 的 20 万提升至 60 万，对齐 OWASP ≥ 60 万建议；
/// 新 wrap 一律以此为输出值并写入信封 rounds 字段，解密方按信封内值派生）
const PBKDF2_ROUNDS: u32 = 600_000;
/// v2 信封 rounds 字段允许上限（防御性：仅挡恶意超大 rounds 引发 CPU 消耗型
/// DoS，10 万倍于默认 60 万，远高于任何现实配置；解析同时走检查化防溢出）
const PBKDF2_ROUNDS_MAX: u32 = 10_000_000;
/// meta 表中包裹后 DEK 的键名
const META_KEY: &str = "vault_dek";

/// SQLite 连接（与 config_store 同库 fyshell.db，独立 Connection）
static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();
/// 解锁后的数据加密密钥（None = 未解锁 / 未初始化）
static DEK: OnceLock<Mutex<Option<[u8; KEY_LEN]>>> = OnceLock::new();

/// 初始化：打开 fyshell.db（独立连接）并兜底建 meta 表（config_store 启动后必已在，
/// 此处幂等；含 vault 启动早于 config_store 的场景）。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(app_data_dir)?;
    let conn = Connection::open(app_data_dir.join("fyshell.db"))?;
    let cell = CONN.get_or_init(|| Mutex::new(conn));
    let guard = cell.lock().expect("vault 数据库连接锁中毒");
    guard.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT
        );",
    )?;
    drop(guard);
    DEK.get_or_init(|| Mutex::new(None));
    Ok(())
}

fn lock_conn() -> MutexGuard<'static, Connection> {
    CONN.get()
        .expect("vault 数据库未初始化（init 未调用）")
        .lock()
        .expect("vault 数据库连接锁中毒")
}

fn lock_dek() -> MutexGuard<'static, Option<[u8; KEY_LEN]>> {
    DEK.get()
        .expect("vault 未初始化（init 未调用）")
        .lock()
        .expect("vault DEK 锁中毒")
}

fn meta_get(key: &str) -> Result<Option<String>, AppError> {
    let conn = lock_conn();
    match conn.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn meta_set(key: &str, value: &str) -> Result<(), AppError> {
    let conn = lock_conn();
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 编解码与随机
// ---------------------------------------------------------------------------

/// 小端 hex 编码（避免引入 base64 依赖；密文规模毫秒级）
fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn from_hex(s: &str) -> Result<Vec<u8>, AppError> {
    if s.len() % 2 != 0 {
        return Err(AppError::general("密文数据损坏（hex 长度非法）"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| AppError::general("密文数据损坏（非法 hex）"))
        })
        .collect()
}

/// 随机字节：拼两个 uuid v4（系统 RNG 来源），截断到 n
fn random_bytes(n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(&uuid::Uuid::new_v4().as_bytes()[..]);
    }
    out.truncate(n);
    out
}

// ---------------------------------------------------------------------------
// 加密原语（AEAD + KEK 派生）
// ---------------------------------------------------------------------------

/// PBKDF2-HMAC-SHA256 派生 KEK；轮数由调用方按信封版本显式指定
///（v1 → PBKDF2_ROUNDS_V1_LEGACY，v2 → 信封内 rounds），杜绝全局默认漂移
fn derive_kek(password: &str, salt: &[u8], rounds: u32) -> [u8; KEY_LEN] {
    let mut kek = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, rounds, &mut kek);
    kek
}

/// 版本感知的信封解析 + KEK 派生统一入口（所有 DEK 包裹/解包路径都走它）：
/// - `v1$salt$nonce$cipher`：轮数固定取 PBKDF2_ROUNDS_V1_LEGACY（200_000）——
///   这是「防 v1 被新默认轮数误解」的关键：v1 永远按历史值派生，与全局默认
///   PBKDF2_ROUNDS 是否被再调大无关
/// - `v2$<rounds>$salt$nonce$cipher`：轮数取信封内 rounds（检查化解析 + 上限）
/// 返回 (nonce, cipher, kek)；畸形版本 / 缺字段 / 非法 rounds / 超长一律返回
/// 错误而非 panic/溢出（安全失败）。
fn parse_envelope_and_derive_kek(
    value: &str,
    password: &str,
) -> Result<(Vec<u8>, Vec<u8>, [u8; KEY_LEN]), AppError> {
    let parts: Vec<&str> = value.split('$').collect();
    let (rounds, salt_hex, nonce_hex, cipher_hex) = match parts.first().copied() {
        // v1 旧信封：无 rounds 字段 → 强制历史轮数（封装期 200_000 是唯一派生依据）
        Some("v1") => {
            if parts.len() != 4 {
                return Err(AppError::general("保险库数据损坏（格式非法）"));
            }
            (PBKDF2_ROUNDS_V1_LEGACY, parts[1], parts[2], parts[3])
        }
        // v2 新信封：rounds 字段显式携带 → 按信封内值派生
        Some("v2") => {
            if parts.len() != 5 {
                return Err(AppError::general("保险库数据损坏（格式非法）"));
            }
            (parse_rounds(parts[1])?, parts[2], parts[3], parts[4])
        }
        _ => return Err(AppError::general("保险库数据损坏（格式非法）")),
    };
    let salt = from_hex(salt_hex)?;
    let nonce = from_hex(nonce_hex)?;
    let cipher = from_hex(cipher_hex)?;
    let kek = derive_kek(password, &salt, rounds);
    Ok((nonce, cipher, kek))
}

/// 检查化解析 rounds 字符串：拒绝空 / 超长 / 非数字 / 溢出 / 0 / 超上限，
/// 全程返回错误不 panic（恶意畸形信封安全失败）。
fn parse_rounds(s: &str) -> Result<u32, AppError> {
    // u32 十进制最多 10 位（4294967295）；超长即畸形，先拦下避免长字符串遍历
    if s.is_empty() || s.len() > 10 {
        return Err(AppError::general("保险库数据损坏（rounds 非法）"));
    }
    let mut rounds: u32 = 0;
    for ch in s.chars() {
        let d = ch.to_digit(10).ok_or_else(|| AppError::general("保险库数据损坏（rounds 非法）"))?;
        rounds = rounds
            .checked_mul(10)
            .and_then(|r| r.checked_add(d))
            .ok_or_else(|| AppError::general("保险库数据损坏（rounds 溢出）"))?;
    }
    if rounds == 0 {
        return Err(AppError::general("保险库数据损坏（rounds 非法）"));
    }
    if rounds > PBKDF2_ROUNDS_MAX {
        return Err(AppError::general("保险库数据损坏（rounds 超上限）"));
    }
    Ok(rounds)
}

fn aead_cipher(key: &[u8; KEY_LEN]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(key.into())
}

/// AEAD 加密：随机 nonce + 密文（含 auth tag）
fn aead_encrypt(key: &[u8; KEY_LEN], plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let nonce_bytes = random_bytes(NONCE_LEN);
    // Nonce 别名即 ChaCha20-Poly1305 的 nonce 类型；用 From<[u8; N]> 构造，避开
    // generic-array 0.14 已弃用的 from_slice/clone_from_slice（行为一致：长度不符 panic）
    let mut nonce_buf = [0u8; NONCE_LEN];
    nonce_buf.copy_from_slice(&nonce_bytes);
    let nonce = Nonce::<ChaCha20Poly1305>::from(nonce_buf);
    let cipher = aead_cipher(key)
        .encrypt(&nonce, plain)
        .map_err(|_| AppError::general("凭据加密失败"))?;
    Ok((nonce_bytes, cipher))
}

/// AEAD 解密（auth 校验失败统一为数据损坏 / 密钥不符错误）；nonce 长度不符
/// 直接报错而非 panic（畸形信封/密文安全失败）
fn aead_decrypt(key: &[u8; KEY_LEN], nonce: &[u8], cipher: &[u8]) -> Result<Vec<u8>, AppError> {
    if nonce.len() != NONCE_LEN {
        return Err(AppError::general("凭据解密失败（数据损坏或密钥不符）"));
    }
    // 与 encrypt 对称：From<[u8; N]> 构造 nonce（长度不符 panic 与旧 from_slice 行为一致）
    let mut nonce_buf = [0u8; NONCE_LEN];
    nonce_buf.copy_from_slice(nonce);
    let nonce = Nonce::<ChaCha20Poly1305>::from(nonce_buf);
    aead_cipher(key)
        .decrypt(&nonce, cipher)
        .map_err(|_| AppError::general("凭据解密失败（数据损坏或密钥不符）"))
}

/// 用主密码派生 KEK 包裹 DEK，生成 meta 可存值：`v2$<rounds>$salt$nonce$cipher`
///（rounds 固定为当前默认 PBKDF2_ROUNDS = 60 万，显式入信封；新写信封一律 v2）
fn wrap_dek(dek: &[u8; KEY_LEN], password: &str) -> Result<String, AppError> {
    let salt = random_bytes(SALT_LEN);
    let kek = derive_kek(password, &salt, PBKDF2_ROUNDS);
    let (nonce, cipher) = aead_encrypt(&kek, dek)?;
    Ok(format!(
        "v2${}${}${}${}",
        PBKDF2_ROUNDS,
        to_hex(&salt),
        to_hex(&nonce),
        to_hex(&cipher)
    ))
}

/// 解包 DEK；主密码错误 / 数据损坏返回错误。兼容 v1 旧信封（按历史 200_000 轮
/// 派生，存量 v1 信封无需重加密即可继续解开）与 v2 新信封（按信封内 rounds）。
fn unwrap_dek(value: &str, password: &str) -> Result<[u8; KEY_LEN], AppError> {
    let (nonce, cipher, kek) = parse_envelope_and_derive_kek(value, password)?;
    let plain = aead_decrypt(&kek, &nonce, &cipher)?;
    let mut dek = [0u8; KEY_LEN];
    if plain.len() != KEY_LEN {
        return Err(AppError::general("保险库数据损坏（DEK 长度非法）"));
    }
    dek.copy_from_slice(&plain);
    Ok(dek)
}

fn generate_dek() -> [u8; KEY_LEN] {
    let bytes = random_bytes(KEY_LEN);
    let mut dek = [0u8; KEY_LEN];
    dek.copy_from_slice(&bytes);
    dek
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 是否已解锁（DEK 在内存中）
pub fn is_unlocked() -> bool {
    DEK.get()
        .map(|d| d.lock().map(|s| s.is_some()).unwrap_or(false))
        .unwrap_or(false)
}

/// 是否已设置主密码（meta 表存在 master_password 键，与 config_store 同表）
pub fn has_master_password() -> bool {
    meta_get("master_password").ok().flatten().is_some()
}

/// 首次设置主密码：注册 DEK 信封（此前无 vault 数据，直接生成新 DEK）并自动解锁。
pub fn rekey(password: &str) -> Result<(), AppError> {
    let dek = generate_dek();
    meta_set(META_KEY, &wrap_dek(&dek, password)?)?;
    *lock_dek() = Some(dek);
    Ok(())
}

/// 改主密码：用旧密码解包现有 DEK → 用新密码重包（存量密文无需迁移）。
///
/// 旧版本可能有主密码但无 DEK 信封（vault 引入前）：此时直接生成新 DEK 并注册
///（无加密存量，安全）。旧密码校验失败时阻止更新，防止凭据变成不可解。
/// 成功后自动解锁（DEK 未变，保持内存密钥）。
pub fn rekey_migrate(old_password: &str, new_password: &str) -> Result<(), AppError> {
    let dek = match meta_get(META_KEY)? {
        Some(wrapped) => unwrap_dek(&wrapped, old_password)
            .map_err(|_| AppError::general("旧主密码不正确，无法迁移已存凭据"))?,
        None => generate_dek(),
    };
    meta_set(META_KEY, &wrap_dek(&dek, new_password)?)?;
    *lock_dek() = Some(dek);
    Ok(())
}

/// 解锁：以主密码解包 DEK 入内存；密码错误 / 数据损坏返回带提示错误
pub fn unlock(password: &str) -> Result<(), AppError> {
    let wrapped = meta_get(META_KEY)?
        .ok_or_else(|| AppError::general("保险库未初始化（尚未设置主密码）"))?;
    let dek = unwrap_dek(&wrapped, password)?;
    *lock_dek() = Some(dek);
    Ok(())
}

/// 锁定：丢弃内存中的 DEK（应用锁定时调用；下次读写凭据需重新解锁）
pub fn lock() {
    if let Some(cell) = DEK.get() {
        if let Ok(mut guard) = cell.lock() {
            *guard = None;
        }
    }
}

/// 加密一段明文凭据（保存路径）；未解锁时返回明确错误由前端引导解锁。
/// 返回 `v1$nonce$cipher`（hex）。
pub fn encrypt(plain: &str) -> Result<String, AppError> {
    let dek = *lock_dek()
        .as_ref()
        .ok_or_else(|| AppError::general("保险库已锁定，请先解锁后保存连接"))?;
    let (nonce, cipher) = aead_encrypt(&dek, plain.as_bytes())?;
    Ok(format!("v1${}${}", to_hex(&nonce), to_hex(&cipher)))
}

/// 解密（读取路径）；未解锁报错
pub fn decrypt(encoded: &str) -> Result<String, AppError> {
    let dek = *lock_dek()
        .as_ref()
        .ok_or_else(|| AppError::general("保险库已锁定，请先解锁"))?;
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() != 3 || parts[0] != "v1" {
        return Err(AppError::general("凭据密文格式损坏"));
    }
    let nonce = from_hex(parts[1])?;
    let cipher = from_hex(parts[2])?;
    let plain = aead_decrypt(&dek, &nonce, &cipher)?;
    String::from_utf8(plain).map_err(|_| AppError::general("凭据解密内容编码错误"))
}

/// 值是否为本库密文格式（`v1$...` 前缀；明文密码恰以此开头时解密失败走容错保持原样）
pub fn is_ciphertext(value: &str) -> bool {
    value.starts_with("v1$")
}

/// 会话 auth_type JSON 写入前加密：已解锁且 password 字段非空非密文时加密该字段
///（JSON 结构保留），否则原样返回（未解锁时沿用明文，迁移在解锁时兜底）。
pub fn encrypt_auth_json(auth_json: &str) -> Result<String, AppError> {
    if !is_unlocked() {
        return Ok(auth_json.to_string());
    }
    encrypt_json_field(auth_json)
}

/// 会话 auth_type JSON 读取后解密：已解锁且 password 字段为密文时解回明文；
/// 未解锁 / 解密失败一律原样返回（密文 round-trip 无损，连接报错由调用方引导）。
pub fn decrypt_auth_json(auth_json: &str) -> String {
    if !is_unlocked() {
        return auth_json.to_string();
    }
    decrypt_json_field(auth_json).unwrap_or_else(|_| auth_json.to_string())
}

/// 加密 auth_type JSON 的 password 字段（内部：调用方已保证解锁检查）
fn encrypt_json_field(auth_json: &str) -> Result<String, AppError> {
    let mut auth: serde_json::Value = serde_json::from_str(auth_json)
        .map_err(|e| AppError::general(&format!("auth_type JSON 解析失败：{e}")))?;
    let Some(password) = auth.get_mut("password").and_then(|p| p.as_str().map(str::to_string)) else {
        return Ok(auth_json.to_string());
    };
    if password.is_empty() || is_ciphertext(&password) {
        return Ok(auth_json.to_string());
    }
    auth["password"] = serde_json::Value::String(encrypt(&password)?);
    serde_json::to_string(&auth).map_err(|e| AppError::general(&format!("auth_type JSON 序列化失败：{e}")))
}

/// 解密 auth_type JSON 的 password 字段（内部：调用方已保证解锁检查）
fn decrypt_json_field(auth_json: &str) -> Result<String, AppError> {
    let mut auth: serde_json::Value = serde_json::from_str(auth_json)
        .map_err(|e| AppError::general(&format!("auth_type JSON 解析失败：{e}")))?;
    let Some(password) = auth.get_mut("password").and_then(|p| p.as_str().map(str::to_string)) else {
        return Ok(auth_json.to_string());
    };
    if password.is_empty() || !is_ciphertext(&password) {
        return Ok(auth_json.to_string());
    }
    auth["password"] = serde_json::Value::String(decrypt(&password)?);
    serde_json::to_string(&auth).map_err(|e| AppError::general(&format!("auth_type JSON 序列化失败：{e}")))
}

/// 明文存量迁移：解锁后一次性扫描全部明文凭据并加密写回。
///
/// 覆盖：settings 表 mysql/redis_saved_connections（整 JSON 加密，与前端读写路径
/// 一致）、sshopt_proxy_password（单值）、sessions 表 auth_type JSON 的 password
/// 字段（结构保留，仅加密 password 值）、auth_profiles 表认证配置 auth_type 的
/// password 字段（与 sessions 同模式）。解密失败的条目跳过保持原样。
/// 在 `master_password_set` / `vault_unlock` 成功后调用（DEK 已在内存）。
pub fn migrate_plaintext() -> Result<usize, AppError> {
    if !is_unlocked() {
        return Err(AppError::general("保险库未解锁，无法迁移明文凭据"));
    }
    let mut migrated = 0usize;
    migrated += migrate_setting_value("mysql_saved_connections")?;
    migrated += migrate_setting_value("redis_saved_connections")?;
    migrated += migrate_setting_value("sshopt_proxy_password")?;
    migrated += migrate_table_auth_types("sessions")?;
    migrated += migrate_table_auth_types("auth_profiles")?;
    Ok(migrated)
}

/// 迁移 settings 表单个键：值非空且非密文时整体加密写回
fn migrate_setting_value(key: &str) -> Result<usize, AppError> {
    let Some(value) = meta_get(key)? else {
        return Ok(0);
    };
    if value.is_empty() || is_ciphertext(&value) {
        return Ok(0);
    }
    meta_set(key, &encrypt(&value)?)?;
    Ok(1)
}

/// 迁移指定表（sessions / auth_profiles）的 auth_type JSON：password 字段非空且
/// 非密文时加密写回整行。表缺失（对应服务模块尚未建表）时静默跳过返回 0，
/// 与各服务模块建表进度解耦，不阻塞其余表迁移。
fn migrate_table_auth_types(table: &str) -> Result<usize, AppError> {
    // 先在锁内读取待迁移行，锁释放后再解密写回（Mutex 不可重入，避免嵌套 lock_conn 死锁）
    let rows: Vec<(String, String)> = {
        let conn = lock_conn();
        // 表不存在（服务建表条件变化/极端旧库）时跳过，避免 migrate_plaintext 报错
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |r| r.get(0),
        )?;
        if !table_exists {
            return Ok(0);
        }
        let mut stmt = conn.prepare(&format!(
            "SELECT id, auth_type FROM {table} WHERE auth_type LIKE '%password%'"
        ))?;
        let collected =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        collected
    };

    let mut updates: Vec<(String, String)> = Vec::new();
    for (id, auth_json) in rows {
        let Ok(mut auth) =
            serde_json::from_str::<serde_json::Value>(&auth_json)
        else {
            continue;
        };
        let Some(password) = auth.get_mut("password").and_then(|p| p.as_str().map(str::to_string))
        else {
            continue;
        };
        if password.is_empty() || is_ciphertext(&password) {
            continue;
        }
        let enc = match encrypt(&password) {
            Ok(v) => v,
            // 单条失败（如含非法 UTF-8）跳过，不阻塞整体迁移
            Err(_) => continue,
        };
        auth["password"] = serde_json::Value::String(enc);
        if let Ok(updated) = serde_json::to_string(&auth) {
            updates.push((updated, id));
        }
    }

    let count = updates.len();
    if count > 0 {
        let conn = lock_conn();
        for (updated, id) in &updates {
            conn.execute(
                &format!("UPDATE {table} SET auth_type = ?1 WHERE id = ?2"),
                params![updated, id],
            )?;
        }
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// 单元测试（服务级：不依赖真实 MySQL/前端，直接验证 DEK 信封全流程）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// OnceLock 全局单例（CONN/DEK 一次初始化，跨测试共享同一临时库），
    /// 用互斥锁串行化全部用例，避免 DEK/信封互相覆盖。
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// 测试库目录（与 init 同目录，保证 auth_profile 等服务模块共用同一 fyshell.db）
    fn test_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("fyshell-vault-unit-test")
    }

    /// 初始化服务（幂等：CONN 已有实例时取第一次打开的库；同库足够，各用例自洽）
    fn setup() {
        let dir = test_dir();
        let _ = std::fs::create_dir_all(&dir);
        init(&dir).expect("vault 测试初始化失败");
    }

    /// 模拟 config_store 写入 master_password 键（与库同表同 key）
    fn fake_set_master_password() {
        meta_set("master_password", "mock-hash").expect("写入 mock 主密码失败");
    }

    #[test]
    fn rekey_encrypt_decrypt_roundtrip() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        fake_set_master_password();

        assert!(!is_unlocked(), "初始应处于未解锁状态");
        rekey("master-pw").expect("首次设置主密码应成功");
        assert!(is_unlocked(), "设置后应自动解锁");
        assert!(has_master_password(), "应能感知已设置主密码");

        let enc = encrypt("敏感凭据-用户:密码").expect("加密应成功");
        assert!(enc.starts_with("v1$"), "密文应带 v1 格式前缀");
        // 明文不应出现在密文中（不泄露凭据内容）
        assert!(!enc.contains("敏感凭据"), "密文不应包含明文");
        assert_eq!(decrypt(&enc).expect("解密应成功"), "敏感凭据-用户:密码");

        // 同明文两次加密不同（随机 nonce）
        let enc2 = encrypt("敏感凭据-用户:密码").expect("二次加密应成功");
        assert_ne!(enc, enc2, "随机 nonce 应产生不同密文");
    }

    #[test]
    fn rekey_migrate_preserves_ciphertext() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        rekey("old-pw").expect("首次设置应成功");
        let enc = encrypt("改密后仍应可解").expect("加密应成功");

        // 改密走 rekey_migrate：旧密码解包 → 新密码重包同一 DEK，存量密文无需迁移
        rekey_migrate("old-pw", "new-pw").expect("旧密码正确应能改密");
        assert!(is_unlocked(), "改密后应保持解锁");
        assert_eq!(decrypt(&enc).expect("存量密文应可解"), "改密后仍应可解");

        // 锁定 → 解密/加密被拒
        lock();
        assert!(!is_unlocked(), "锁定后应处于锁定态");
        assert!(decrypt(&enc).is_err(), "锁定后解密应失败");
        assert!(encrypt("x").is_err(), "锁定后加密应失败");

        // 错误密码解锁被拒，正确密码解锁后可解存量密文
        assert!(unlock("wrong-pw").is_err(), "错误主密码应拒绝解锁");
        assert!(!is_unlocked(), "解锁失败不应改变状态");
        unlock("new-pw").expect("正确主密码应能解锁");
        assert_eq!(decrypt(&enc).expect("解锁后可解存量密文"), "改密后仍应可解");
    }

    #[test]
    fn rekey_migrate_rejects_wrong_old_password() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        rekey("truE-密码").expect("首次设置应成功");
        let enc = encrypt("数据应保持原样").expect("加密应成功");

        // 旧密码错误：拒绝迁移（防已存凭据变成不可解）
        assert!(rekey_migrate("wrong-old", "new").is_err(), "旧密码错误应拒绝改密");
        // DEK 未被篡改：仍可解原密文
        assert_eq!(decrypt(&enc).expect("改密失败不应影响存量"), "数据应保持原样");
    }

    #[test]
    fn corrupt_or_garbage_cipher_rejected() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        rekey("pw").expect("首次设置应成功");

        // 格式非法 / hex 损坏 / 长度非法：一律报错而非 panic
        assert!(decrypt("garbage").is_err(), "非法格式应报错");
        assert!(decrypt("").is_err(), "空串应报错");
        assert!(decrypt("v1$zz$zz").is_err(), "非法 hex 应报错");
        assert!(decrypt("v1$12$34$56").is_err(), "nonce 长度非法应报错");
        assert!(decrypt("v2$12$34").is_err(), "未知版本应报错");

        // 篡改密文（翻转首字节）应被 AEAD 认证拒绝，且不泄露明文
        let (nonce, cipher) = aead_encrypt(&[0u8; KEY_LEN], b"hello").unwrap();
        let mut tampered = cipher.clone();
        if let Some(b) = tampered.first_mut() {
            *b ^= 0xff;
        }
        assert!(
            aead_decrypt(&[0u8; KEY_LEN], &nonce, &tampered).is_err(),
            "篡改密文应认证失败"
        );

        // 错误密钥解密（信封包裹场景：密码不符 = 密钥不符）
        assert!(
            aead_decrypt(&[1u8; KEY_LEN], &nonce, &cipher).is_err(),
            "密钥不符应解密失败"
        );
    }

    #[test]
    fn unlock_before_init_cleanup_and_yield() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        // 未设主密码（无 vault_dek 信封）时解锁报"未初始化"
        assert!(unlock("any").is_err(), "无信封时应报未初始化");
        // 首次设置即解锁，原样验证 is_unlocked 幂等
        rekey("pw").expect("首次设置应成功");
        assert!(is_unlocked());
    }

    #[test]
    fn migrate_plaintext_covers_saved_conns_and_auth_types() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        fake_set_master_password();

        // 迁移扫描依赖 settings/sessions 表（应用侧由 config_store 建，测试内补建）
        {
            let conn = lock_conn();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT);
                 CREATE TABLE IF NOT EXISTS sessions (
                     id TEXT PRIMARY KEY, name TEXT, auth_type TEXT);",
            )
            .expect("建测试表失败");
        }

        // 模拟明文存量：settings 表整 JSON + sessions 表 auth_type
        {
            let conn = lock_conn();
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('mysql_saved_connections', ?1)",
                params![r#"[{"id":"a","name":"root@h","host":"h","port":3306,"username":"root","password":"明文pw","schema":null}]"#],
            )
            .expect("写入 mysql 明文存量失败");
            conn.execute(
                "INSERT OR REPLACE INTO sessions (id, name, auth_type) VALUES ('s1', '192.168.1.1', ?1)",
                params![r#"{"type":"password","password":"ssh明文pw"}"#],
            )
            .expect("写入会话明文存量失败");
        }

        // 未解锁时迁移被拒
        assert!(migrate_plaintext().is_err(), "未解锁时迁移应被拒");

        // 解锁后迁移：明文 → 密文
        rekey("master-pw").expect("设主密码应成功");
        let n = migrate_plaintext().expect("迁移应成功");
        assert!(n >= 2, "至少迁移 mysql_saved_connections 与 auth_type 两条");

        // settings 表已密文：前端读取路径 decrypt 后还原明文
        let wrapped = {
            let conn = lock_conn();
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'mysql_saved_connections'",
                [],
                |r| r.get::<_, String>(0),
            )
            .expect("读取迁移后密文失败")
        };
        assert!(is_ciphertext(&wrapped), "settings 存量应已加密");
        assert!(
            decrypt(&wrapped).unwrap().contains("明文pw"),
            "解密后应还原明文内容"
        );

        // sessions 表 auth_type 已密文：decrypt_auth_json 还原明文字段
        let auth_json = {
            let conn = lock_conn();
            conn.query_row("SELECT auth_type FROM sessions WHERE id = 's1'", [], |r| {
                r.get::<_, String>(0)
            })
            .expect("读取迁移后 auth_type 失败")
        };
        assert!(is_ciphertext(&auth_json), "auth_type 应已加密");
        let restored = decrypt_auth_json(&auth_json);
        assert!(restored.contains("ssh明文pw"), "解密后应还原明文密码");

        // round-trip：再次迁移幂等（已密文条目跳过）
        let n2 = migrate_plaintext().expect("二次迁移应成功");
        assert_eq!(n2, 0, "已密文条目不应重复迁移");

        // 未解锁时 decrypt_auth_json 原样透传（密文 round-trip 无损）
        lock();
        assert_eq!(
            decrypt_auth_json(&auth_json),
            auth_json,
            "未解锁时 auth_type 密文应原样透传"
        );
    }

    #[test]
    fn migrate_plaintext_covers_auth_profiles() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        fake_set_master_password();

        // 建 auth_profiles 表（应用侧由 auth_profile::init 建，测试内补建，
        // 结构须与 services/auth_profile.rs 一致）
        {
            let conn = lock_conn();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS auth_profiles (
                     id        TEXT PRIMARY KEY,
                     name      TEXT NOT NULL,
                     auth_type TEXT NOT NULL
                 );",
            )
            .expect("建 auth_profiles 表失败");
        }

        // 模拟明文存量：password 型 profile 明文 + 无 password 字段的 publicKey profile
        {
            let conn = lock_conn();
            conn.execute(
                "INSERT OR REPLACE INTO auth_profiles (id, name, auth_type) VALUES ('p1', '跳板机', ?1)",
                params![r#"{"type":"password","password":"profile明文pw"}"#],
            )
            .expect("写入 password profile 明文存量失败");
            conn.execute(
                "INSERT OR REPLACE INTO auth_profiles (id, name, auth_type) VALUES ('p2', '公钥', ?1)",
                params![r#"{"type":"publicKey","privateKeyPath":"C:\\keys\\id_rsa","passphrase":null}"#],
            )
            .expect("写入 publicKey profile 失败");
        }

        // 未解锁时迁移被拒
        assert!(migrate_plaintext().is_err(), "未解锁时迁移应被拒");

        // 解锁后迁移：password profile 加密，publicKey profile（无 password 字段）不动
        rekey("master-pw").expect("设主密码应成功");
        let n = migrate_plaintext().expect("迁移应成功");
        assert!(n >= 1, "至少迁移 auth_profiles 一条");

        let auth_json = {
            let conn = lock_conn();
            conn.query_row("SELECT auth_type FROM auth_profiles WHERE id = 'p1'", [], |r| {
                r.get::<_, String>(0)
            })
            .expect("读取迁移后 profile auth_type 失败")
        };
        assert!(is_ciphertext(&auth_json), "auth_profiles 存量应已加密");
        let restored = decrypt_auth_json(&auth_json);
        assert!(restored.contains("profile明文pw"), "解密后应还原明文密码");

        // 无 password 字段的 profile 无需加密（与 sessions 行为一致，不解密照常可用）
        let pub_json = {
            let conn = lock_conn();
            conn.query_row("SELECT auth_type FROM auth_profiles WHERE id = 'p2'", [], |r| {
                r.get::<_, String>(0)
            })
            .expect("读取 publicKey profile 失败")
        };
        assert!(!is_ciphertext(&pub_json), "无 password 字段的 profile 不应被加密");
    }

    #[test]
    fn auth_profile_save_list_vault_roundtrip() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();
        fake_set_master_password();

        // 初始化 auth_profile 服务（与应用 setup 同目录，共用同一 fyshell.db）
        crate::services::auth_profile::init(&test_dir()).expect("auth_profile 初始化失败");

        // 解锁前保存：未加密，明文落库（兼容未设主密码场景）
        let plain_profile = crate::models::auth_profile::AuthProfile {
            id: "prof-unlocked".to_string(),
            name: "未解锁配置".to_string(),
            auth_type: crate::models::session::AuthType::Password {
                password: "明文pre".to_string(),
            },
        };
        crate::services::auth_profile::save(&plain_profile).expect("解锁前保存应成功");
        let listed = crate::services::auth_profile::list().expect("解锁前列表应成功");
        assert!(
            listed.iter().any(|p| matches!(&p.auth_type,
                crate::models::session::AuthType::Password { password } if password == "明文pre")),
            "解密前保存的 profile 应保持明文可取"
        );

        // 设置主密码并解锁 → 加密路径保存，落库为密文
        rekey("master-pw").expect("设主密码应成功");
        let secret = crate::models::auth_profile::AuthProfile {
            id: "prof-secret".to_string(),
            name: "机密配置".to_string(),
            auth_type: crate::models::session::AuthType::Password {
                password: "加密后密码".to_string(),
            },
        };
        crate::services::auth_profile::save(&secret).expect("加密保存应成功");

        // 落库内容应为密文（不出现明文密码）
        let stored: String = {
            let conn = lock_conn();
            conn.query_row(
                "SELECT auth_type FROM auth_profiles WHERE id = 'prof-secret'",
                [],
                |r| r.get(0),
            )
            .expect("读取加密 profile 失败")
        };
        assert!(is_ciphertext(&stored), "解锁保存的 auth_profiles 应为密文");
        assert!(!stored.contains("加密后密码"), "密文不应包含明文密码");

        // 解锁状态读取：取回明文密码
        let listed = crate::services::auth_profile::list().expect("解锁列表应成功");
        assert!(
            listed.iter().any(|p| matches!(&p.auth_type,
                crate::models::session::AuthType::Password { password } if password == "加密后密码")),
            "解锁后读取应还原明文密码"
        );

        // 锁定后读取：密文 round-trip 无损（不解密原样返回，不崩）
        lock();
        let listed = crate::services::auth_profile::list().expect("锁定列表应成功");
        assert!(
            listed.iter().any(|p| matches!(&p.auth_type,
                crate::models::session::AuthType::Password { password } if password.starts_with("v1$"))),
            "锁定后 profile password 应为密文原样透传"
        );

        // 重新解锁后再次取回明文（换密码等路径不破坏存量）
        unlock("master-pw").expect("重新解锁应成功");
        let listed = crate::services::auth_profile::list().expect("再解锁列表应成功");
        assert!(
            listed.iter().any(|p| matches!(&p.auth_type,
                crate::models::session::AuthType::Password { password } if password == "加密后密码")),
            "重新解锁后仍应取回明文密码"
        );
    }

    // ---------------------------------------------------------------------------
    // 信封版本化（v1 兼容 / v2 新格式 / 畸形安全失败）
    // ---------------------------------------------------------------------------

    /// v1 旧信封（无 rounds 字段）必须按历史 200_000 轮解开——这是版本路由
    /// 防「v1 被新默认轮数误解」的核心回归测试：手工构造固定输入的可复现向量，
    /// 无论全局默认 PBKDF2_ROUNDS 如何变更，v1 只能按 PBKDF2_ROUNDS_V1_LEGACY 派生。
    #[test]
    fn v1_envelope_legacy_rounds_opens() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        // 固定向量：DEK / salt / nonce 全固定 → ChaCha20-Poly1305 确定性，
        // 每次运行产物一致（可跑环境下等价已知向量）
        let dek = [0x42u8; KEY_LEN];
        let salt = [0x11u8; SALT_LEN];
        let nonce = [0x22u8; NONCE_LEN];
        let kek = derive_kek("known-pw", &salt, PBKDF2_ROUNDS_V1_LEGACY);
        let cipher = aead_cipher(&kek)
            .encrypt(&Nonce::<ChaCha20Poly1305>::from(nonce), &dek[..])
            .expect("用 200_000 轮 KEK 加密 DEK 失败");
        let v1_envelope = format!("v1${}${}${}", to_hex(&salt), to_hex(&nonce), to_hex(&cipher));

        // 关键路由断言：v1 必须按 200_000 轮解开，解出 DEK 与构造时一致
        assert_eq!(
            unwrap_dek(&v1_envelope, "known-pw").expect("v1 信封应能解包"),
            dek,
            "v1 解出 DEK 应与构造一致"
        );
        // 错误密码被 AEAD 认证拒绝
        assert!(unwrap_dek(&v1_envelope, "wrong-pw").is_err(), "v1 信封错误密码应拒绝");

        // 对照：若有人用新默认 600_000 轮手工造出一个「v1」信封，本路由固定走
        // 200_000 轮派生 → KEK 不符 → 必须解不开（反向证明 v1 永不被新默认误解）
        let kek_new = derive_kek("known-pw", &salt, PBKDF2_ROUNDS);
        let cipher_new = aead_cipher(&kek_new)
            .encrypt(&Nonce::<ChaCha20Poly1305>::from(nonce), &dek[..])
            .expect("用新默认轮数加密 DEK 失败");
        let fake_v1 = format!("v1${}${}${}", to_hex(&salt), to_hex(&nonce), to_hex(&cipher_new));
        assert!(
            unwrap_dek(&fake_v1, "known-pw").is_err(),
            "v1 路由必须固定 200_000 轮，新默认轮数派生的密文不应被解开"
        );
    }

    /// v2 新信封 round-trip：wrap 输出 v2 且 rounds 为当前默认；解包还原同一
    /// DEK；篡改信封内 rounds 会让派生 KEK 变化 → 认证失败（证明路由真实使用
    /// 信封内轮数，而非全局默认）。
    #[test]
    fn v2_envelope_roundtrip_with_rounds_routing() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        let dek = generate_dek();
        let wrapped = wrap_dek(&dek, "test-pw").expect("v2 包装应成功");
        let head: Vec<&str> = wrapped.split('$').collect();
        assert!(wrapped.starts_with("v2$"), "新 wrap 应输出 v2 信封");
        assert_eq!(head[1], PBKDF2_ROUNDS.to_string(), "v2 信封 rounds 应为当前默认");
        assert_eq!(head.len(), 5, "v2 信封应有 5 段：版本+rounds+salt+nonce+cipher");

        // round-trip：按信封内 rounds 派生解包，还原同一 DEK
        assert_eq!(
            unwrap_dek(&wrapped, "test-pw").expect("v2 信封应能解包"),
            dek,
            "v2 round-trip 应还原 DEK"
        );

        // 篡改 rounds：rounds-1 依然合法且 ≤ 上限，但派生 KEK 不同 → 认证失败
        let tampered = wrapped.replace(
            &format!("v2${}", PBKDF2_ROUNDS),
            &format!("v2${}", PBKDF2_ROUNDS - 1),
        );
        assert!(
            tampered != wrapped && unwrap_dek(&tampered, "test-pw").is_err(),
            "rounds 篡改应导致解包失败（证明信封内轮数被真实采用）"
        );
    }

    /// 畸形信封一律安全失败返回错误，绝不 panic/溢出：未知版本、缺/多字段、
    /// rounds 非数字 / 0 / 溢出 / 超长 / 空串。
    #[test]
    fn malformed_envelopes_fail_safely() {
        let _g = TEST_LOCK.lock().unwrap();
        setup();

        // 未知版本 / 缺版本
        assert!(parse_envelope_and_derive_kek("v3$1$2$3", "pw").is_err(), "未知版本应报错");
        assert!(parse_envelope_and_derive_kek("$1$2$3", "pw").is_err(), "缺版本应报错");
        assert!(parse_envelope_and_derive_kek("", "pw").is_err(), "空串应报错");
        // v1 缺/多字段
        assert!(parse_envelope_and_derive_kek("v1$a$b", "pw").is_err(), "v1 缺字段应报错");
        assert!(parse_envelope_and_derive_kek("v1$a$b$c$d", "pw").is_err(), "v1 多字段应报错");
        // v2 缺字段 / rounds 非法
        assert!(parse_envelope_and_derive_kek("v2$a$b", "pw").is_err(), "v2 缺字段应报错");
        assert!(parse_envelope_and_derive_kek("v2$abc$a$b$c", "pw").is_err(), "rounds 非数字应报错");
        assert!(parse_envelope_and_derive_kek("v2$0$a$b$c", "pw").is_err(), "rounds=0 应报错");
        assert!(parse_envelope_and_derive_kek("v2$-1$a$b$c", "pw").is_err(), "rounds 带负号应报错");
        assert!(parse_envelope_and_derive_kek("v2$1.0$a$b$c", "pw").is_err(), "rounds 带点号应报错");
        // rounds 溢出（u32 最大 4294967295）与超上限
        assert!(
            parse_envelope_and_derive_kek("v2$4294967296$a$b$c", "pw").is_err(),
            "rounds 超 u32 应报错"
        );
        assert!(
            parse_envelope_and_derive_kek(&format!("v2${}$a$b$c", "9".repeat(20)), "pw").is_err(),
            "rounds 超长应报错"
        );
        assert!(
            parse_envelope_and_derive_kek("v2$10000001$a$b$c", "pw").is_err(),
            "rounds 超上限应报错"
        );
        // 合法轮数 + 非法 hex → from_hex 报错（整链路安全失败）
        assert!(
            parse_envelope_and_derive_kek("v2$600000$zz$bb$cc", "pw").is_err(),
            "非法 hex 应报错"
        );
        // 畸形信封经 unwrap_dek 同样安全失败（统一走同一解析入口）
        assert!(unwrap_dek("v2$abc$1$2$3", "pw").is_err(), "unwrap 也应安全拒绝畸形信封");
    }
}
