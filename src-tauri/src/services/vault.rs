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

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::aead::generic_array::GenericArray;
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
/// PBKDF2 迭代轮数（OWASP 建议 ≥ 60 万；取 20 万平衡交互速度与安全）
const PBKDF2_ROUNDS: u32 = 200_000;
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

fn derive_kek(password: &str, salt: &[u8]) -> [u8; KEY_LEN] {
    let mut kek = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ROUNDS, &mut kek);
    kek
}

fn aead_cipher(key: &[u8; KEY_LEN]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(key.into())
}

/// AEAD 加密：随机 nonce + 密文（含 auth tag）
fn aead_encrypt(key: &[u8; KEY_LEN], plain: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let nonce_bytes = random_bytes(NONCE_LEN);
    let nonce = GenericArray::clone_from_slice(&nonce_bytes);
    let cipher = aead_cipher(key)
        .encrypt(&nonce, plain)
        .map_err(|_| AppError::general("凭据加密失败"))?;
    Ok((nonce_bytes, cipher))
}

/// AEAD 解密（auth 校验失败统一为数据损坏 / 密钥不符错误）
fn aead_decrypt(key: &[u8; KEY_LEN], nonce: &[u8], cipher: &[u8]) -> Result<Vec<u8>, AppError> {
    let nonce = GenericArray::clone_from_slice(nonce);
    aead_cipher(key)
        .decrypt(&nonce, cipher)
        .map_err(|_| AppError::general("凭据解密失败（数据损坏或密钥不符）"))
}

/// 用主密码派生 KEK 包裹 DEK，生成 meta 可存值：`v1$salt$nonce$cipher`
fn wrap_dek(dek: &[u8; KEY_LEN], password: &str) -> Result<String, AppError> {
    let salt = random_bytes(SALT_LEN);
    let kek = derive_kek(password, &salt);
    let (nonce, cipher) = aead_encrypt(&kek, dek)?;
    Ok(format!("v1${}${}${}", to_hex(&salt), to_hex(&nonce), to_hex(&cipher)))
}

/// 解包 DEK；主密码错误 / 数据损坏返回错误
fn unwrap_dek(value: &str, password: &str) -> Result<[u8; KEY_LEN], AppError> {
    let parts: Vec<&str> = value.split('$').collect();
    if parts.len() != 4 || parts[0] != "v1" {
        return Err(AppError::general("保险库数据损坏（格式非法）"));
    }
    let salt = from_hex(parts[1])?;
    let nonce = from_hex(parts[2])?;
    let cipher = from_hex(parts[3])?;
    let kek = derive_kek(password, &salt);
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
