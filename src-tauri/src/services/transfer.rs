//! 传输队列管理：入队（Queued）→ tokio spawn 调度执行（Running）→ 终态（Completed/Failed/Cancelled）。
//!
//! 进度更新写回 `AppState.transfer_tasks` 并 emit "transfer-status" 事件（契约第 3 节），
//! 同时 send 到任务自己的 `Channel<TransferTask>`；取消通过 `cancelled_transfers` 定期检查实现。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::models::transfer::{TransferKind, TransferStatus, TransferTask};
use crate::services::sftp;
use crate::state::AppState;

/// 进度推送节流间隔：AppState 每批都更新，emit / Channel 按间隔节流（事件仅低频状态）
const EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// 单个传输任务整体超时兜底：底层 open_sftp / 读写在对端断连、通道异常等场景
/// 可能无限挂起（此前无任何超时，任务会永久停在 Running/Queued——即「上传卡住」
/// 的后端根因）。超时后任务归 Failed 并携带明确错误信息，前端不再无限等待。
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(120);

/// 取消标记信息（终态 error 字段用）
const CANCELLED_MSG: &str = "传输已取消";

/// 任务 id → 前端进度 Channel 注册表。
/// AppState 字段由 state.rs 固定，Queue 用模块级静态表避免给 AppState 加字段。
static PROGRESS_CHANNELS: OnceLock<Mutex<HashMap<String, Channel<TransferTask>>>> = OnceLock::new();

fn channels() -> &'static Mutex<HashMap<String, Channel<TransferTask>>> {
    PROGRESS_CHANNELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// transfer-status 事件 payload（契约第 3 节：{ task: TransferTask }）
#[derive(Serialize, Clone)]
struct TransferStatusEvent<'a> {
    task: &'a TransferTask,
}

/// 入队：任务以 Queued 状态写入快照并广播，随后 spawn 调度执行。
/// 返回入队后的任务快照（前端可凭 id 调 transfer_cancel）。
pub fn enqueue(
    app: &AppHandle,
    state: &AppState,
    kind: TransferKind,
    session_id: &str,
    local_path: String,
    remote_path: String,
    on_progress: Channel<TransferTask>,
) -> Result<TransferTask, AppError> {
    let task = TransferTask {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        kind,
        local_path,
        remote_path,
        // total_bytes 由核心逻辑探测实际大小后回填（progress 回调携带 total）
        total_bytes: 0,
        transferred_bytes: 0,
        status: TransferStatus::Queued,
        error: None,
    };
    let task_id = task.id.clone();

    state
        .transfer_tasks
        .lock()
        .expect("transfer_tasks 锁被污染")
        .push(task.clone());
    channels()
        .lock()
        .expect("进度 Channel 注册表锁被污染")
        .insert(task_id.clone(), on_progress);

    // Queued 快照广播
    broadcast(app, &task);

    // spawn 调度执行
    let app_handle = app.clone();
    let spawn_task_id = task_id.clone();
    tokio::spawn(async move {
        run_task(app_handle, spawn_task_id).await;
    });

    Ok(task)
}

/// 执行单个传输任务：Running → 核心传输 → 终态
async fn run_task(app: AppHandle, task_id: String) {
    let state = app.state::<AppState>();

    // 入队后、启动前已被取消（Queued 状态取消）
    if is_cancelled(&state, &task_id) {
        if let Some(task) = update_task(&state, &task_id, |t| {
            t.status = TransferStatus::Cancelled;
            t.error = Some(CANCELLED_MSG.to_string());
        }) {
            broadcast(&app, &task);
        }
        cleanup(&state, &task_id);
        return;
    }

    // 标记 Running
    if let Some(task) = update_task(&state, &task_id, |t| {
        t.status = TransferStatus::Running;
    }) {
        broadcast(&app, &task);
    }

    // 取出任务参数
    let (kind, session_id, local_path, remote_path) = {
        let tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
        match tasks.iter().find(|t| t.id == task_id) {
            Some(t) => (
                t.kind,
                t.session_id.clone(),
                t.local_path.clone(),
                t.remote_path.clone(),
            ),
            None => {
                // 任务不存在（异常态）也先释放锁再走终态清理，
                // 避免进度 Channel 注册表项/取消标记泄漏
                drop(tasks);
                cleanup(&state, &task_id);
                return;
            }
        }
    };

    // 进度回调：每批更新 AppState 并检查取消，emit / Channel 按间隔节流
    // （核心传输 sftp.rs 内部打开的读/写文件句柄由 RAII 自动关闭，无 fd 泄漏）
    let mut last_emit = Instant::now() - EMIT_INTERVAL; // 允许立即首次推送
    let mut progress = |transferred: u64, total: u64| -> bool {
        // 取消检查：命中即中止
        if is_cancelled(&state, &task_id) {
            return false;
        }
        // 写回 AppState（每批都更新）
        update_task(&state, &task_id, |t| {
            t.transferred_bytes = transferred;
            t.total_bytes = total;
        });
        // 节流推送（终态推送由下方兜底）
        if last_emit.elapsed() >= EMIT_INTERVAL {
            last_emit = Instant::now();
            if let Some(task) = get_task(&state, &task_id) {
                broadcast(&app, &task);
            }
        }
        true
    };

    let result = match kind {
        TransferKind::Upload => {
            timeout_transport(sftp::upload_file(
                &state,
                &session_id,
                &local_path,
                &remote_path,
                &mut progress,
            ))
            .await
        }
        TransferKind::Download => {
            timeout_transport(sftp::download_file(
                &state,
                &session_id,
                &remote_path,
                &local_path,
                &mut progress,
            ))
            .await
        }
    };

    // 终态：命中取消集合优先归为 Cancelled，其次失败，最后成功
    let cancelled = is_cancelled(&state, &task_id);
    let status = if cancelled {
        TransferStatus::Cancelled
    } else if result.is_ok() {
        TransferStatus::Completed
    } else {
        TransferStatus::Failed
    };
    let error = if cancelled {
        Some(CANCELLED_MSG.to_string())
    } else {
        result.err().map(|e| e.to_string())
    };

    if let Some(task) = update_task(&state, &task_id, |t| {
        t.status = status;
        t.error = error;
    }) {
        broadcast(&app, &task);
    }
    cleanup(&state, &task_id);
}

/// 包一层整体传输超时：底层挂起（对端断连/通道异常等）超过限时即失败返回，
/// 避免任务无限期停留在 Running/Queued（上传卡住整治）。内部错误原样透传。
async fn timeout_transport(
    fut: impl std::future::Future<Output = Result<(), AppError>>,
) -> Result<(), AppError> {
    match tokio::time::timeout(TRANSPORT_TIMEOUT, fut).await {
        Err(_) => Err(AppError::general(format!(
            "传输超时（超过 {} 秒无进展），任务已中止",
            TRANSPORT_TIMEOUT.as_secs()
        ))),
        Ok(r) => r,
    }
}

/// 任务是否已被取消
fn is_cancelled(state: &AppState, task_id: &str) -> bool {
    state
        .cancelled_transfers
        .lock()
        .expect("cancelled_transfers 锁被污染")
        .contains(task_id)
}

/// 更新任务字段（在 AppState.transfer_tasks 中按 id 定位），返回更新后的快照
fn update_task<F>(state: &AppState, task_id: &str, f: F) -> Option<TransferTask>
where
    F: FnOnce(&mut TransferTask),
{
    let mut tasks = state.transfer_tasks.lock().expect("transfer_tasks 锁被污染");
    let task = tasks.iter_mut().find(|t| t.id == task_id)?;
    f(task);
    Some(task.clone())
}

/// 读取任务快照
fn get_task(state: &AppState, task_id: &str) -> Option<TransferTask> {
    state
        .transfer_tasks
        .lock()
        .expect("transfer_tasks 锁被污染")
        .iter()
        .find(|t| t.id == task_id)
        .cloned()
}

/// 推送任务快照：emit "transfer-status" 广播 + send 到任务自己的进度 Channel
fn broadcast(app: &AppHandle, task: &TransferTask) {
    let _ = app.emit("transfer-status", TransferStatusEvent { task });
    if let Some(ch) = channels()
        .lock()
        .expect("进度 Channel 注册表锁被污染")
        .get(&task.id)
        .cloned()
    {
        let _ = ch.send(task.clone());
    }
}

/// 终态清理：移除取消标记与 Channel 注册表项
fn cleanup(state: &AppState, task_id: &str) {
    state
        .cancelled_transfers
        .lock()
        .expect("cancelled_transfers 锁被污染")
        .remove(task_id);
    channels()
        .lock()
        .expect("进度 Channel 注册表锁被污染")
        .remove(task_id);
}

/// 清除指定任务的进度 Channel（transfer_clear 清记录时调用）
pub fn remove_channels(task_ids: &[String]) {
    let mut map = channels()
        .lock()
        .expect("进度 Channel 注册表锁被污染");
    for id in task_ids {
        map.remove(id);
    }
}
