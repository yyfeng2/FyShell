//! SSH 隧道命令层：契约第 5.2 节的 5 个命令，薄层转发（参数校验 + 调用 services）。
//!
//! 隧道服务使用模块级注册表（不依赖 AppState），命令层无需 State；
//! 仅 tunnel_start 需要 AppHandle（事件推送与运行时任务持有）。


use crate::error::AppError;
use crate::models::tunnel::{StartAllReport, TunnelRule};
use crate::services::tunnel as tunnel_service;

/// 隧道规则列表：`session_id` 非空时仅返回该会话的规则
#[tauri::command]
#[specta::specta]
pub async fn tunnel_list(session_id: Option<String>) -> Result<Vec<TunnelRule>, AppError> {
    tunnel_service::list_rules(session_id.as_deref())
}

/// 保存规则（新规则的 id 为空时由后端生成；upsert），返回含 id 的规则快照
#[tauri::command]
#[specta::specta]
pub async fn tunnel_save(mut rule: TunnelRule) -> Result<TunnelRule, AppError> {
    if rule.id.is_empty() {
        rule.id = uuid::Uuid::new_v4().to_string();
    }
    tunnel_service::save_rule(&rule)?;
    Ok(rule)
}

/// 删除规则并停止（未启动的隧道忽略停止步骤）
#[tauri::command]
#[specta::specta]
pub async fn tunnel_delete(id: String) -> Result<(), AppError> {
    let _ = tunnel_service::stop(&id);
    tunnel_service::delete_rule(&id)
}

/// 启动监听
#[tauri::command]
#[specta::specta]
pub async fn tunnel_start(app: tauri::AppHandle, id: String) -> Result<(), AppError> {
    let rule = tunnel_service::get_rule(&id)?
        .ok_or_else(|| AppError::general(format!("隧道 {id} 不存在")))?;
    tunnel_service::start_tunnel(&app, rule)
}

/// 停止监听
#[tauri::command]
#[specta::specta]
pub async fn tunnel_stop(id: String) -> Result<(), AppError> {
    tunnel_service::stop(&id)
}

/// 一键全启：遍历全部规则逐个启动（已在运行中的跳过），返回成功/失败计数
#[tauri::command]
#[specta::specta]
pub async fn tunnel_start_all(app: tauri::AppHandle) -> Result<StartAllReport, AppError> {
    tunnel_service::start_all(&app)
}
