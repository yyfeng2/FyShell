//! 会话管理命令（契约第 2 节）：薄层，参数校验 + 调用 config_store。
//!
//! `session_test` 独立实现简化的"连接 + 认证 + 断开"逻辑（不依赖 services/ssh
//! 的内部 API），支持密码与私钥两种主路径；HostKey 确认走正式连接的事件流。

use tauri::State;

use crate::error::AppError;
use crate::models::session::{AuthType, NodeReorderItem, SessionConfig, SessionFolder, SessionNode};
use crate::state::AppState;

/// 会话/文件夹树（扁平：文件夹+会话混合节点，前端按 folderId/parentId 组树）
///
/// 契约签名为 `()`；`filter` 为可选扩展参数（Tauri 允许省略 Option 参数），
/// 前端搜索过滤可下沉到 Rust 侧执行。
#[tauri::command]
#[specta::specta]
pub fn session_list(
    state: State<'_, AppState>,
    filter: Option<String>,
) -> Result<Vec<SessionNode>, AppError> {
    state.config_store.list_nodes(filter.as_deref())
}

/// 保存会话（新会话生成 id）
#[tauri::command]
#[specta::specta]
pub fn session_save(
    state: State<'_, AppState>,
    mut config: SessionConfig,
) -> Result<SessionConfig, AppError> {
    validate_session(&config)?;
    if config.id.trim().is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
    }
    state.config_store.save_session(&config)?;
    Ok(config)
}

/// 删除会话或文件夹（is_folder 区分，前端已二次确认）
#[tauri::command]
#[specta::specta]
pub fn session_delete(state: State<'_, AppState>, id: String, is_folder: bool) -> Result<(), AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("id 不能为空"));
    }
    if is_folder {
        state.config_store.delete_folder(&id)
    } else {
        state.config_store.delete_session(&id)
    }
}

/// 测试连接：一次性"连接 + 认证 + 断开"的轻量测试，返回 `{ ok, message }`
///
/// State 由 Tauri 注入（不属于前端 IPC 签名），仅用于 Jump 时读取被引用的跳板会话。
#[tauri::command]
#[specta::specta]
pub async fn session_test(
    state: State<'_, AppState>,
    config: SessionConfig,
) -> Result<TestResult, AppError> {
    if config.host.trim().is_empty() {
        return Ok(TestResult::fail("主机地址不能为空"));
    }
    if config.port == 0 {
        return Ok(TestResult::fail("端口不能为 0"));
    }

    // 跳板机：沿 jump_session_id 链解析到最终目标（防环，最多 8 级），测试最终目标
    let mut target = config;
    let mut hops = 0usize;
    while let AuthType::Jump { jump_session_id } = &target.auth_type {
        if hops >= 8 {
            return Ok(TestResult::fail("跳板机引用链过深（超过 8 级）"));
        }
        if *jump_session_id == target.id {
            return Ok(TestResult::fail("跳板机引用了自身会话"));
        }
        target = match state.config_store.get_session(jump_session_id)? {
            Some(next) => next,
            None => {
                return Ok(TestResult::fail(format!(
                    "跳板会话 {jump_session_id} 不存在"
                )))
            }
        };
        hops += 1;
    }

    let hop_note = if hops > 0 {
        format!("（经 {hops} 级跳板）")
    } else {
        String::new()
    };

    match ssh_test_connect(&target).await {
        Ok(detail) => Ok(TestResult {
            ok: true,
            message: format!("连接成功 {hop_note}{detail}"),
        }),
        Err(e) => Ok(TestResult {
            ok: false,
            message: format!("连接失败 {hop_note}: {e}"),
        }),
    }
}

/// 导航树批量重排（拖拽归类/排序）：folders/sessions 父级与顺序一次事务更新
#[tauri::command]
#[specta::specta]
pub fn session_reorder(
    state: State<'_, AppState>,
    items: Vec<NodeReorderItem>,
) -> Result<(), AppError> {
    if items.is_empty() {
        return Err(AppError::general("重排项不能为空"));
    }
    for item in &items {
        if item.id.trim().is_empty() {
            return Err(AppError::general("重排项 id 不能为空"));
        }
        if item.kind != "folder" && item.kind != "session" {
            return Err(AppError::general(format!("未知节点类型 {}", item.kind)));
        }
    }
    state.config_store.reorder_nodes(&items)
}

/// 保存文件夹（新文件夹生成 id）
#[tauri::command]
#[specta::specta]
pub fn folder_save(
    state: State<'_, AppState>,
    mut folder: SessionFolder,
) -> Result<SessionFolder, AppError> {
    if folder.name.trim().is_empty() {
        return Err(AppError::general("文件夹名称不能为空"));
    }
    if folder.id.trim().is_empty() {
        folder.id = uuid::Uuid::new_v4().to_string();
    }
    state.config_store.save_folder(&folder)?;
    Ok(folder)
}

/// 会话克隆：读原配置 → 新 uuid → 名称加"副本"后缀 → save_session → 返回新配置
///
/// 克隆复制完整配置（含 folder_id / auth_type / profile_id 引用），不建立连接。
#[tauri::command]
#[specta::specta]
pub fn session_clone(state: State<'_, AppState>, id: String) -> Result<SessionConfig, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::general("id 不能为空"));
    }
    let mut config = state
        .config_store
        .get_session(&id)?
        .ok_or_else(|| AppError::general(format!("会话 {id} 不存在")))?;
    config.id = uuid::Uuid::new_v4().to_string();
    config.name = format!("{} 副本", config.name);
    state.config_store.save_session(&config)?;
    Ok(config)
}

/// 会话参数校验（commands 层薄校验）
fn validate_session(config: &SessionConfig) -> Result<(), AppError> {
    if config.name.trim().is_empty() {
        return Err(AppError::general("会话名称不能为空"));
    }
    if config.host.trim().is_empty() {
        return Err(AppError::general("主机地址不能为空"));
    }
    if config.port == 0 {
        return Err(AppError::general("端口必须大于 0"));
    }
    if config.username.trim().is_empty() {
        return Err(AppError::general("用户名不能为空"));
    }
    Ok(())
}

/// session_test 的返回结构（契约：`{ ok: bool, message: String }`）
#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

impl TestResult {
    /// 失败结果便捷构造
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

/// russh 客户端 Handler：P0 轻量测试用，接受任意 HostKey
/// （正式连接的 HostKey 确认走 session-status / hostkey-prompt 事件流，由 commands/ssh.rs 实现）
struct TestHostKeyHandler;

impl russh::client::Handler for TestHostKeyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // P0 简化：测试连接阶段不校验 HostKey
        Ok(true)
    }
}

/// 独立的简化认证连接逻辑：连接 + 按认证方式认证 + 断开，返回描述信息
async fn ssh_test_connect(config: &SessionConfig) -> Result<String, AppError> {
    let addr = format!("{}:{}", config.host, config.port);

    // 连接（russh client::connect 内部完成 TCP 连接与 SSH 握手）
    let russh_config = std::sync::Arc::new(russh::client::Config::default());
    let mut handle = russh::client::connect(russh_config, addr.as_str(), TestHostKeyHandler)
        .await
        .map_err(|e| AppError::Ssh(format!("{addr} SSH 握手失败: {e}")))?;

    // 按认证方式认证（ russh 0.63 API：返回 AuthResult 枚举）
    let auth_result = match &config.auth_type {
        AuthType::Password { password } => {
            handle
                .authenticate_password(&config.username, password)
                .await
        }
        // keyboard-interactive：P0 简化为密码方式尝试（多数服务器两者等价）
        AuthType::Interactive { password } => {
            handle
                .authenticate_password(&config.username, password)
                .await
        }
        AuthType::PublicKey {
            private_key_path,
            passphrase,
        } => {
            let key = russh::keys::load_secret_key(private_key_path, passphrase.as_deref())
                .map_err(|e| AppError::Ssh(format!("私钥加载失败: {e}")))?;
            let key_with_hash =
                russh::keys::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None);
            handle.authenticate_publickey(&config.username, key_with_hash).await
        }
        AuthType::NoAuth => handle.authenticate_none(&config.username).await,
        AuthType::Jump { .. } => {
            // 已在 session_test 中解析为最终目标，不会到达此处
            unreachable!("Jump 认证已在 session_test 中解析")
        }
    };

    // 断开（认证结果检查前先断开，避免句柄泄漏；错误不影响结果判定）
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "test", "zh-CN")
        .await;

    match auth_result {
        Ok(russh::client::AuthResult::Success) => Ok(format!(
            "（{}@{}:{}）",
            config.username, config.host, config.port
        )),
        _ => Err(AppError::Ssh("认证失败：用户名、密码或密钥不正确".into())),
    }
}
