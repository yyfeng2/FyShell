# FyShell P0 IPC 契约（所有智能体的唯一接口标准）

> 本文件是前后端接口的唯一事实来源。任何修改需经主会话确认。
> 前端组件**禁止**直接调用 `invoke`，必须通过 `src/api/` 封装层。

## 1. 数据模型（Rust models / TS types 同名同构）

### SessionConfig（会话配置，存 Rust 侧 SQLite）
```rust
struct SessionConfig {
    id: String,              // uuid v4
    name: String,            // 显示名
    folder_id: Option<String>, // 所属文件夹（null=根级）
    host: String,
    port: u16,               // 默认 22
    username: String,
    auth_type: AuthType,     // 5 种认证方式
    encoding: String,        // 默认 "UTF-8"
    color: Option<String>,   // Tab 着色（hex）
    keepalive_interval: u32, // 秒，默认 30
}
```

### AuthType（5 种认证方式）
```rust
enum AuthType {
    Password { password: String },
    PublicKey { private_key_path: String, passphrase: Option<String> },
    Interactive { password: String },   // keyboard-interactive
    NoAuth,                             // 不验证
    Jump { jump_session_id: String },   // 跳板机：引用另一个已存会话
}
```
TS 侧为可辨识联合（discriminated union），字段名一致，`type` 字段值为 `"password" | "publicKey" | "interactive" | "noAuth" | "jump"`。

### SessionFolder（会话树文件夹）
```rust
struct SessionFolder { id: String, name: String, parent_id: Option<String> }
```

### FileEntry（SFTP 文件条目）
```rust
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
    modified_at: i64,  // Unix 秒
    permissions: String,
}
```

### TransferTask（传输任务）
```rust
struct TransferTask {
    id: String,
    session_id: String,
    kind: TransferKind,          // Upload | Download
    local_path: String,
    remote_path: String,
    total_bytes: u64,
    transferred_bytes: u64,
    status: TransferStatus,      // Queued | Running | Completed | Failed | Cancelled
    error: Option<String>,
}
```

## 2. 命令（invoke，Rust commands 层）

> **P0 实现备注**（2026-09-17 集成后）：
> - `session_list` 增加可选参数 `filter: Option<String>`（按名称/主机模糊过滤，前端不传即全量）
> - `SessionNode` serde tag 为 `kind: "folder" | "session"`（internally tagged），层级由 folder_id/parent_id 表达，前端组装
> - Jump（跳板机）认证 P0 连接时返回明确错误"跳板机模式暂未实现"（配置可保存，嵌套转发留 P1）
> - `sftp_upload`/`sftp_download` 返回 `TransferTask`（含生成的任务 id）
> - 新增本地文件命令：`local_mkdir`、`local_rename`、`local_delete`（std::fs 实现）

### 会话管理（commands/session.rs ↔ api/session.ts）
| 命令 | 签名 | 返回 |
|---|---|---|
| `session_list` | `()` | `Vec<SessionNode>`（树形：文件夹+会话混合节点） |
| `session_save` | `(config: SessionConfig)` | `SessionConfig`（新会话生成 id） |
| `session_delete` | `(id: String, is_folder: bool)` | `()` |
| `session_test` | `(config: SessionConfig)` | `{ ok: bool, message: String }` |
| `folder_save` | `(folder: SessionFolder)` | `SessionFolder` |

### SSH 终端（commands/ssh.rs ↔ api/ssh.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `ssh_connect` | `(id: String, onOutput: Channel<Vec<u8>>)` | 建立 PTY/shell；输出流走 Channel |
| `ssh_disconnect` | `(id: String)` | 关闭并清理 session/PTY |
| `ssh_write` | `(id: String, data: Vec<u8>)` | 键盘输入写入 |
| `ssh_resize` | `(id: String, cols: u32, rows: u32)` | 按会话 ID 路由 resize |

### SFTP 传输（commands/sftp.rs ↔ api/sftp.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `sftp_list` | `(id: String, path: String)` | `Vec<FileEntry>` |
| `sftp_mkdir` | `(id: String, path: String)` | 创建目录 |
| `sftp_delete` | `(id: String, path: String, is_dir: bool)` | 删除（前端二次确认） |
| `sftp_rename` | `(id: String, old_path: String, new_path: String)` | 重命名 |
| `sftp_upload` | `(id: String, local_path: String, remote_path: String, onProgress: Channel<TransferTask>)` | 入队上传，进度走 Channel |
| `sftp_download` | `(id: String, remote_path: String, local_path: String, onProgress: Channel<TransferTask>)` | 入队下载 |
| `transfer_list` | `()` | `Vec<TransferTask>` 队列快照 |
| `transfer_cancel` | `(task_id: String)` | 取消任务 |
| `transfer_clear` | `()` | 清除已完成/失败记录 |
| `local_list` | `(path: String)` | `Vec<FileEntry>` 本地目录列表（供双栏左侧使用，`..` 返回上级） |

## 3. 事件（emit，仅低频状态）

| 事件 | payload | 说明 |
|---|---|---|
| `session-status` | `{ id: String, status: "connecting" \| "connected" \| "disconnected" \| "hostkey-verify" }` | 连接状态变更 |
| `hostkey-prompt` | `{ id: String, host: String, fingerprint: String }` | HostKey 首次确认；前端调 `ssh_hostkey_accept(id, accept: bool)` |
| `transfer-status` | `{ task: TransferTask }` | 传输队列变更广播 |

补充命令：`ssh_hostkey_accept(id: String, accept: bool) -> ()`

## 4. 架构红线（摘自 plans/new-version-architecture.md §1.4）

- Rust commands 层保持薄：参数校验 + 转发，重计算用 `async` / `spawn_blocking`
- 统一错误类型：`error.rs` 定义 `AppError`（thiserror + Serialize，serialize 为字符串）
- 终端输出：Rust 侧 4KB 批量读取再 send；ANSI 解析交给 xterm.js
- 所有前端 `listen` 在组件卸载时 `unlisten()`
- 标签关闭时 Rust 侧同步清理 SSH session/PTY
- resize 按会话 ID 路由

---

# 5. P1 契约扩展（第二期）

## 5.1 数据模型

```rust
/// 监控采样（SSH exec 采集 /proc，Channel 推送）
struct MonitorSample {
    cpu_percent: f64,
    mem_used_mb: u64,
    mem_total_mb: u64,
    net_rx_kb: u64,   // 累计接收 KB
    net_tx_kb: u64,   // 累计发送 KB
}

/// Docker 容器（SSH exec docker 命令采集）
struct DockerContainer {
    id: String,
    names: String,
    image: String,
    state: String,   // running / exited / paused
    status: String,  // 人类可读状态
}

/// 快捷命令（树状分类，Xshell 8 模式）
struct QuickCommand { id: String, name: String, command_text: String, group_id: Option<String> }
struct QuickCommandFolder { id: String, name: String, parent_id: Option<String> }
enum QuickCommandNode { Folder(QuickCommandFolder), Command(QuickCommand) }
// serde tag "kind": "folder" | "command"

/// SSH 隧道规则
struct TunnelRule {
    id: String,
    session_id: String,
    kind: TunnelKind,    // Local | Remote | Socks
    listen_host: String, // 默认 127.0.0.1
    listen_port: u16,
    target_host: String, // Local/Remote: 目标；Socks: 空串
    target_port: u16,
    enabled: bool,       // 会话连接时是否自动启动
    status: TunnelStatus, // Stopped | Listening | Error
    error: Option<String>,
}

/// 认证配置文件（一处改全局生效；SessionConfig 新增可选字段 profile_id）
struct AuthProfile { id: String, name: String, auth_type: AuthType }

/// MySQL 基础
struct MySqlConnection { host: String, port: u16, username: String, password: String, schema: Option<String> }
struct MySqlTableInfo { name: String, rows: u64, comment: String, engine: String }
struct MySqlQueryResult {
    columns: Vec<String>,
    rows: Vec<Vec<Option<String>>>, // None = SQL NULL
    total: u64,
    page: u32,
    page_size: u32,
}
```

## 5.2 命令（P1 新增）

### 服务器监控（commands/monitor.rs ↔ api/monitor.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `monitor_start` | `(id: String, interval_secs: u32, on_sample: Channel<MonitorSample>)` | 开始采集（SSH exec /proc） |
| `monitor_stop` | `(id: String)` | 停止采集 |
| `docker_list` | `(id: String)` | `Vec<DockerContainer>` |
| `docker_operate` | `(id: String, container_id: String, action: String)` | action: start/stop/restart |

### 快捷命令（commands/quick_command.rs ↔ api/quickCommand.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `qc_list` | `()` | `Vec<QuickCommandNode>` |
| `qc_save_command` | `(cmd: QuickCommand)` | `QuickCommand` |
| `qc_save_folder` | `(folder: QuickCommandFolder)` | `QuickCommandFolder` |
| `qc_delete` | `(id: String, is_folder: bool)` | 删除 |

发送到会话复用既有 `ssh_write`，无新命令。

### SSH 隧道（commands/tunnel.rs ↔ api/tunnel.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `tunnel_list` | `(session_id: Option<String>)` | `Vec<TunnelRule>` |
| `tunnel_save` | `(rule: TunnelRule)` | `TunnelRule` |
| `tunnel_delete` | `(id: String)` | 删除并停止 |
| `tunnel_start` | `(id: String)` | 启动监听 |
| `tunnel_stop` | `(id: String)` | 停止监听 |

### 会话增强（commands/session_log.rs + auth_profile.rs）
| 命令 | 签名 | 说明 |
|---|---|---|
| `auth_profile_list` | `()` | `Vec<AuthProfile>` |
| `auth_profile_save` | `(profile: AuthProfile)` | `AuthProfile` |
| `auth_profile_delete` | `(id: String)` | 删除 |
| `session_clone` | `(id: String)` | `SessionConfig`（新 id） |
| `session_log_toggle` | `(session_id: String, enabled: bool)` | 输出落盘开关（app_data_dir/logs/<session_id>/日期.log） |
| `session_log_list` | `(session_id: String)` | `Vec<String>` 日期列表 |
| `session_log_read` | `(session_id: String, date: String)` | `String` 日志内容 |

### MySQL 基础（commands/mysql.rs ↔ api/mysql.ts）
| 命令 | 签名 | 说明 |
|---|---|---|
| `mysql_connect` | `(config: MySqlConnection)` | String connection_id |
| `mysql_disconnect` | `(conn_id: String)` | 关闭连接 |
| `mysql_list_tables` | `(conn_id: String)` | `Vec<MySqlTableInfo>` |
| `mysql_query` | `(conn_id: String, sql: String, page: u32, page_size: u32)` | `MySqlQueryResult` |
| `mysql_execute` | `(conn_id: String, sql: String)` | `u64` 受影响行数 |
| `mysql_begin` / `mysql_commit` / `mysql_rollback` | `(conn_id: String)` | 事务 |

## 5.3 P1 前端增强

- 标签拖出新窗口：`@tauri-apps/api/webviewWindow` 的 WebviewWindow，标签携带 session id，新窗口内自行连接
- 监控走 Channel 推送，无新增事件
- 认证配置文件：SessionForm 增加配置文件选择器，选择后 auth_type 从配置文件解析
