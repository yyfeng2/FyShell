//! 服务器监控服务：基于 SSH exec 采集 /proc 统计信息（CPU / 内存 / 网络）。
//!
//! 架构要点：
//! - 通过 `SshSessionHandle::exec(command)` 执行远端命令采集（该公开方法由 services/ssh 提供）；
//! - 每次间隔执行一次 exec，单命令合并采集 /proc/stat、/proc/meminfo、/proc/net/dev，
//!   用标记行切分输出后在 Rust 侧解析；
//! - CPU 百分比由相邻两次采样的时间片差值计算（首次采样无基准，返回 0）；
//! - 组装契约第 5.1 节的 MonitorSample 经 `Channel<MonitorSample>` 推送（监控走 Channel，无新增事件）；
//! - 采样任务用 tokio spawn 运行，经模块级注册表（OnceLock）管理，可随时停止；
//! - 不修改 AppState：运行时句柄用模块级注册表，主会话统一在 lib.rs 接线。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::Manager;

use crate::error::AppError;
use crate::services::ssh::SshSessionHandle;

// ---------------------------------------------------------------------------
// 契约数据模型（IPC 契约第 5.1 节，Rust / TS 同名同构）
// ---------------------------------------------------------------------------

/// 监控采样（SSH exec 采集 /proc，Channel 推送）
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MonitorSample {
    pub cpu_percent: f64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    /// 累计接收 KB
    pub net_rx_kb: u64,
    /// 累计发送 KB
    pub net_tx_kb: u64,
}

/// Docker 容器（SSH exec docker 命令采集）
#[derive(Debug, Clone, Serialize)]
pub struct DockerContainer {
    pub id: String,
    pub names: String,
    pub image: String,
    /// running / exited / paused
    pub state: String,
    /// 人类可读状态
    pub status: String,
}

// ---------------------------------------------------------------------------
// 采样任务注册表（不修改 AppState：参考 transfer.rs 进度 Channel 注册表的做法）
// ---------------------------------------------------------------------------

/// 单个监控任务的注册表项
struct MonitorEntry {
    /// 停止信号发送端：注册表持有，stop() 取出后 send 触发任务退出
    stop_tx: tokio::sync::mpsc::UnboundedSender<()>,
    /// 任务代号：同一会话重启监控时递增，旧任务退出时据此判断注册表项是否仍指向自己
    generation: u64,
}

/// 采样任务注册表：key = 会话 id。AppState 字段固定，用模块级静态表避免给 AppState 加字段。
static MONITOR_TASKS: OnceLock<Mutex<HashMap<String, MonitorEntry>>> = OnceLock::new();

/// 全局任务代号自增器：每次 start 递增
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// std::sync::Mutex 中毒时的统一错误转换
fn lock_err<T>(_e: PoisonError<T>) -> AppError {
    AppError::general("监控任务注册表锁被污染")
}

fn registry() -> &'static Mutex<HashMap<String, MonitorEntry>> {
    MONITOR_TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 初始化模块级注册表（主会话在 lib.rs setup 中统一接线调用）。
/// 监控数据不做本地持久化，`app_data_dir` 参数保留注入模式（与 ConfigStore::open 一致）。
pub fn init(app_data_dir: &Path) -> Result<(), AppError> {
    let _ = app_data_dir;
    registry();
    Ok(())
}

// ---------------------------------------------------------------------------
// 采样任务
// ---------------------------------------------------------------------------

/// 单次 exec 采集命令：合并读取三份 /proc 文件，用标记行分隔输出。
/// 一次 exec 完成全部采集（契约：每次 interval 执行一次 exec）。
const COLLECT_COMMAND: &str = "cat /proc/stat 2>/dev/null; echo __FY_MON_MEM__; \
    cat /proc/meminfo 2>/dev/null; echo __FY_MON_NET__; cat /proc/net/dev 2>/dev/null";

/// 输出分隔标记（echo 标记行）
const MEM_MARKER: &str = "__FY_MON_MEM__";
const NET_MARKER: &str = "__FY_MON_NET__";

/// 连续采集失败容忍上限：超过该次数视为会话已不可用，停止采样任务
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// 一次采样的原始计数值（用于相邻两次采样的差值计算）
struct RawSample {
    /// /proc/stat 的 idle + iowait 时间片
    idle: u64,
    /// /proc/stat 全部时间片之和
    total: u64,
    mem_total_mb: u64,
    mem_used_mb: u64,
    net_rx_kb: u64,
    net_tx_kb: u64,
}

/// 开始采集：`pub fn start(app, session_id, interval_secs, on_sample)`。
/// 采样任务用 tokio spawn 运行，注册表按会话 id 管理；
/// 同一会话重复 start 时先停止旧任务（幂等重启）。
pub fn start(
    app: &tauri::AppHandle,
    session_id: &str,
    interval_secs: u32,
    on_sample: tauri::ipc::Channel<MonitorSample>,
) -> Result<(), AppError> {
    // 从 AppState 取出会话句柄克隆（AppState 未修改，仅读取）
    let handle = {
        let state = app.state::<crate::state::AppState>();
        let handle = state
            .ssh_sessions
            .lock()
            .map_err(lock_err)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::general(format!("会话 {session_id} 不存在或未连接")))?;
        if !handle.is_alive() {
            return Err(AppError::Ssh(format!(
                "会话 {session_id} 已断开，无法开始监控"
            )));
        }
        handle
    };

    // 同一会话已有采集在运行：先停止旧任务，再注册新任务（幂等重启）
    stop(session_id);

    // 停止信号通道：注册表持有发送端，任务持有接收端
    let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    registry()
        .lock()
        .map_err(lock_err)?
        .insert(
            session_id.to_string(),
            MonitorEntry {
                stop_tx,
                generation,
            },
        );

    // 采样任务：tokio spawn，独立于终端读循环运行
    let session_id_owned = session_id.to_string();
    tauri::async_runtime::spawn(async move {
        run_monitor(
            handle,
            session_id_owned,
            interval_secs,
            on_sample,
            stop_rx,
            generation,
        )
        .await;
    });
    Ok(())
}

/// 停止采集：从注册表移除该会话的任务并发出停止信号（不存在时静默返回）
pub fn stop(session_id: &str) {
    let removed = registry()
        .lock()
        .map(|mut m| m.remove(session_id))
        .unwrap_or(None);
    if let Some(entry) = removed {
        // Receiver 已被 drop（任务已自行退出）时发送失败无影响
        let _ = entry.stop_tx.send(());
    }
}

/// 采样任务主循环：每次间隔执行一次 exec 采集 → 组装 MonitorSample → Channel 推送。
///
/// 退出条件（任一命中即结束并清理注册表）：
/// - 收到停止信号（registry 发出）；
/// - 前端 Channel 关闭（send 失败）；
/// - 连续采集失败超过容忍次数（会话断开等不可恢复场景）。
async fn run_monitor(
    handle: SshSessionHandle,
    session_id: String,
    interval_secs: u32,
    on_sample: tauri::ipc::Channel<MonitorSample>,
    mut stop_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    generation: u64,
) {
    let interval = Duration::from_secs(u64::from(interval_secs.max(1)));
    // 上一次采样的 /proc/stat 时间片 (idle, total)：两次采样差值算 CPU 百分比
    let mut prev_stat: Option<(u64, u64)> = None;
    let mut consecutive_failures: u32 = 0;

    loop {
        match collect_sample(&handle, &mut prev_stat).await {
            Ok(sample) => {
                consecutive_failures = 0;
                // 前端 Channel 关闭（组件卸载 / 窗口关闭）即结束采样
                if on_sample.send(sample).is_err() {
                    break;
                }
            }
            Err(_) => {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    break;
                }
            }
        }

        // 停止信号与定时器二选一：stop 立即生效，否则按 interval 等待下一轮采集
        tokio::select! {
            _ = stop_rx.recv() => break,
            _ = tokio::time::sleep(interval) => {}
        }
    }

    // 清理注册表：仅当注册表项仍指向本任务（generation 匹配）时移除，
    // 避免旧任务退出时误清同会话重启后的新任务
    if let Ok(mut tasks) = registry().lock() {
        let same = tasks
            .get(&session_id)
            .map(|e| e.generation)
            .is_some_and(|g| g == generation);
        if same {
            tasks.remove(&session_id);
        }
    }
}

/// 执行一次 exec 采集并解析，计算 CPU 百分比（差值法）
async fn collect_sample(
    handle: &SshSessionHandle,
    prev_stat: &mut Option<(u64, u64)>,
) -> Result<MonitorSample, AppError> {
    let output = handle.exec(COLLECT_COMMAND).await?;
    let raw = parse_proc(&output).ok_or_else(|| {
        // 输出无法解析：非 Linux / proc 文件缺失
        AppError::general("采集失败：/proc 输出解析异常（远端可能不是 Linux 系统）")
    })?;

    // CPU：相邻两次采样的时间片差值；首次采样无基准，返回 0
    let cpu_percent = match *prev_stat {
        Some((prev_idle, prev_total)) => {
            let idle_delta = raw.idle.saturating_sub(prev_idle);
            let total_delta = raw.total.saturating_sub(prev_total);
            if total_delta > 0 {
                ((total_delta - idle_delta) as f64 / total_delta as f64) * 100.0
            } else {
                0.0
            }
        }
        None => 0.0,
    };
    // 记录本次时间片，供下次差值计算
    *prev_stat = Some((raw.idle, raw.total));

    Ok(MonitorSample {
        cpu_percent: cpu_percent.clamp(0.0, 100.0),
        mem_used_mb: raw.mem_used_mb,
        mem_total_mb: raw.mem_total_mb,
        net_rx_kb: raw.net_rx_kb,
        net_tx_kb: raw.net_tx_kb,
    })
}

/// 解析一次 exec 采集的原始输出（/proc/stat + /proc/meminfo + /proc/net/dev）
fn parse_proc(output: &str) -> Option<RawSample> {
    // 按标记切分为三段：stat 段在第一个标记前，mem 段在两标记之间，net 段在第二个标记后
    let mem_pos = output.find(MEM_MARKER)?;
    let net_pos = output.find(NET_MARKER)?.max(mem_pos); // 防御：标记乱序时避免切片 panic
    let stat_section = &output[..mem_pos];
    let mem_section = &output[mem_pos + MEM_MARKER.len()..net_pos.min(output.len())];
    let net_section = &output[(net_pos + NET_MARKER.len()).min(output.len())..];

    let (idle, total) = parse_stat(stat_section)?;
    let (mem_total_mb, mem_used_mb) = parse_meminfo(mem_section)?;
    let (net_rx_kb, net_tx_kb) = parse_net_dev(net_section);

    Some(RawSample {
        idle,
        total,
        mem_total_mb,
        mem_used_mb,
        net_rx_kb,
        net_tx_kb,
    })
}

/// 解析 /proc/stat 首行："cpu  user nice system idle iowait ..."（cpuN 子核行跳过，取总行）
fn parse_stat(stat_section: &str) -> Option<(u64, u64)> {
    let line = stat_section
        .lines()
        .find(|l| l.starts_with("cpu ") || l.starts_with("cpu\t"))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1) // 跳过 "cpu" 标识
        .filter_map(|f| f.parse::<u64>().ok())
        .collect();
    // 至少 user/nice/system/idle 四个字段
    if fields.len() < 4 {
        return None;
    }
    // idle 时间片 = idle + iowait（iowait 同样视为空闲）
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().sum();
    Some((idle, total))
}

/// 解析 /proc/meminfo：MemTotal 与 MemAvailable（旧内核无 MemAvailable 时回退 MemFree）
fn parse_meminfo(mem_section: &str) -> Option<(u64, u64)> {
    let mut total_kb = None;
    let mut available_kb = None;
    let mut free_kb = None;
    for line in mem_section.lines() {
        if let Some(v) = line.strip_prefix("MemTotal:") {
            total_kb = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("MemFree:") {
            free_kb = parse_kb_value(v);
        }
    }
    let total_kb = total_kb?;
    // used = total - available（可用内存优先，旧内核回退 free）
    let used_kb = total_kb - available_kb.or(free_kb).unwrap_or(0);
    // kB → MB（1024 进制）
    Some((total_kb / 1024, used_kb / 1024))
}

/// 解析 /proc/meminfo 行内的数值部分（去掉 kB 后缀）
fn parse_kb_value(value: &str) -> Option<u64> {
    value
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .ok()
}

/// 解析 /proc/net/dev："  eth0: rx_bytes packets ... tx_bytes ..."，
/// 累计所有网卡的接收 / 发送字节（含 lo，与系统总收发一致）
fn parse_net_dev(net_section: &str) -> (u64, u64) {
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    for line in net_section.lines() {
        // 跳过表头行（"Inter-|   Receive ..." 与 " face |..."）
        if !line.contains(':') {
            continue;
        }
        let colon = line.find(':').unwrap();
        // 冒号后字段：rx_bytes 为第 1 个，tx_bytes 为第 9 个（中间隔 7 个接收统计字段）
        let fields: Vec<&str> = line[colon + 1..].split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        rx_total += fields[0].parse::<u64>().unwrap_or(0);
        tx_total += fields[8].parse::<u64>().unwrap_or(0);
    }
    // 字节 → KB（1024 进制，累计值）
    (rx_total / 1024, tx_total / 1024)
}

// ---------------------------------------------------------------------------
// Docker 容器：SSH exec docker 命令采集
// ---------------------------------------------------------------------------

/// docker 命令不存在的友好错误文案
const DOCKER_NOT_FOUND_MSG: &str = "远程服务器未安装 docker 或 docker 命令不可用";

/// 执行 docker 命令并识别"docker 未安装"场景（docker 命令不存在时返回友好错误）
async fn exec_docker(handle: &SshSessionHandle, command: &str) -> Result<String, AppError> {
    let output = handle.exec(command).await.map_err(|e| {
        // exec 报错且提示命令不存在（127 语义）时转为友好错误
        let msg = e.to_string().to_lowercase();
        if msg.contains("command not found") || msg.contains("not found") {
            AppError::general(DOCKER_NOT_FOUND_MSG)
        } else {
            AppError::Ssh(format!("执行 docker 命令失败: {e}"))
        }
    })?;
    // docker 不在 PATH 时部分 shell 把错误并入 stdout，此处统一识别
    if output.to_lowercase().contains("command not found") {
        return Err(AppError::general(DOCKER_NOT_FOUND_MSG));
    }
    Ok(output)
}

/// 列出全部 Docker 容器：`docker ps -a --format ...` 解析为契约的 DockerContainer
pub async fn docker_list(
    state: &crate::state::AppState,
    id: &str,
) -> Result<Vec<DockerContainer>, AppError> {
    let handle = get_session_handle(state, id)?;
    // Go template 用 "|" 分隔：容器名 / 镜像名均不含 "|"，避免 shell 转义歧义
    let output = exec_docker(
        &handle,
        r#"docker ps -a --format "{{.ID}}|{{.Names}}|{{.Image}}|{{.State}}|{{.Status}}""#,
    )
    .await?;

    let mut containers = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() < 5 {
            // 无法解析的行直接跳过
            continue;
        }
        containers.push(DockerContainer {
            id: fields[0].trim().to_string(),
            names: fields[1].trim().to_string(),
            image: fields[2].trim().to_string(),
            state: fields[3].trim().to_string(),
            status: fields[4].trim().to_string(),
        });
    }
    Ok(containers)
}

/// 容器操作：action 为 start / stop / restart（契约第 5.2 节）
pub async fn docker_operate(
    state: &crate::state::AppState,
    id: &str,
    container_id: &str,
    action: &str,
) -> Result<(), AppError> {
    // 安全护栏：action 白名单 + 容器 ID 字符白名单，防止拼接成任意远端命令
    if !matches!(action, "start" | "stop" | "restart") {
        return Err(AppError::general(format!(
            "不支持的容器操作: {action}（仅支持 start/stop/restart）"
        )));
    }
    if container_id.is_empty()
        || !container_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(AppError::general(format!("非法容器 ID: {container_id}")));
    }
    let handle = get_session_handle(state, id)?;
    let command = format!("docker {action} {container_id}");
    exec_docker(&handle, &command).await?;
    Ok(())
}

/// 从 AppState 取出会话句柄克隆（避免跨 await 持有锁）
fn get_session_handle(
    state: &crate::state::AppState,
    id: &str,
) -> Result<SshSessionHandle, AppError> {
    let handle = state
        .ssh_sessions
        .lock()
        .map_err(lock_err)?
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::general(format!("会话 {id} 不存在或未连接")))?;
    if !handle.is_alive() {
        return Err(AppError::Ssh(format!("会话 {id} 已断开")));
    }
    Ok(handle)
}
