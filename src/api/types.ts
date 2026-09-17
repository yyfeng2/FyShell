/**
 * FyShell P0 IPC 契约数据类型（docs/ipc-contracts.md 第 1 节 / 第 3 节）
 *
 * 与 Rust 侧 models 同名同构：
 * - Rust snake_case 字段在 TS 侧保持原样（如 auth_type、folder_id），保证 invoke 参数/返回直传不转换
 * - AuthType / SessionNode 为可辨识联合，以 `type` / `kind` 字段区分
 */

/** 认证方式：5 种（对应 Rust enum AuthType） */
export type AuthType =
  | { type: 'password'; password: string }
  | { type: 'publicKey'; private_key_path: string; passphrase: string | null }
  | { type: 'interactive'; password: string } // keyboard-interactive
  | { type: 'noAuth' } // 不验证
  | { type: 'jump'; jump_session_id: string }; // 跳板机：引用另一个已存会话

/** 会话配置（Rust 侧 SQLite 持久化） */
export interface SessionConfig {
  id: string; // uuid v4，新会话由 Rust 侧生成
  name: string; // 显示名
  folder_id: string | null; // 所属文件夹（null=根级）
  host: string;
  port: number; // 默认 22
  username: string;
  auth_type: AuthType;
  encoding: string; // 默认 "UTF-8"
  color: string | null; // Tab 着色（hex）
  keepalive_interval: number; // 秒，默认 30
  profile_id?: string | null; // 认证配置文件（P1，null=未使用）
}

/** 会话树文件夹 */
export interface SessionFolder {
  id: string;
  name: string;
  parent_id: string | null;
}

/** 会话树节点：文件夹（discriminated union，kind 字段区分） */
export interface SessionFolderNode {
  kind: 'folder';
  id: string;
  name: string;
  parent_id: string | null;
}

/** 会话树节点：会话（discriminated union，kind 字段区分） */
export interface SessionNodeLeaf {
  kind: 'session';
  id: string;
  name: string;
  folder_id: string | null;
  /** 会话快照配置，供编辑/连接直接取用 */
  config: SessionConfig;
}

/** 会话树节点（session_list 返回：文件夹+会话混合树） */
export type SessionNode = SessionFolderNode | SessionNodeLeaf;

/** SFTP 文件条目 */
export interface FileEntry {
  name: string;
  is_dir: boolean;
  size: number; // u64
  modified_at: number; // Unix 秒（i64）
  permissions: string;
}

/** 传输类型（对应 Rust enum TransferKind：Upload | Download） */
export type TransferKind = 'Upload' | 'Download';

/** 传输状态（对应 Rust enum TransferStatus：Queued | Running | Completed | Failed | Cancelled） */
export type TransferStatus =
  | 'Queued'
  | 'Running'
  | 'Completed'
  | 'Failed'
  | 'Cancelled';

/** 传输任务 */
export interface TransferTask {
  id: string;
  session_id: string;
  kind: TransferKind;
  local_path: string;
  remote_path: string;
  total_bytes: number; // u64
  transferred_bytes: number; // u64
  status: TransferStatus;
  error: string | null;
}

/* ============ 第 3 节：事件 payload（emit，仅低频状态） ============ */

/** `session-status` 事件：连接状态变更 */
export interface SessionStatusEvent {
  id: string;
  status: 'connecting' | 'connected' | 'disconnected' | 'hostkey-verify';
}

/** `hostkey-prompt` 事件：HostKey 首次确认；确认结果调 sshHostkeyAccept(id, accept) */
export interface HostkeyPromptEvent {
  id: string;
  host: string;
  fingerprint: string;
}

/** `transfer-status` 事件：传输队列变更广播 */
export interface TransferStatusEvent {
  task: TransferTask;
}

/** session_test 返回值 */
export interface SessionTestResult {
  ok: boolean;
  message: string;
}

/* ============ P1 契约扩展（docs/ipc-contracts.md 第 5 节，第二期） ============ */

/** 监控采样（SSH exec 采集 /proc，Channel 推送） */
export interface MonitorSample {
  cpu_percent: number; // f64
  mem_used_mb: number; // u64
  mem_total_mb: number; // u64
  net_rx_kb: number; // u64，累计接收 KB
  net_tx_kb: number; // u64，累计发送 KB
}

/** Docker 容器（SSH exec docker 命令采集） */
export interface DockerContainer {
  id: string;
  names: string;
  image: string;
  state: string; // running / exited / paused
  status: string; // 人类可读状态
}

/** 快捷命令（树状分类，Xshell 8 模式） */
export interface QuickCommand {
  id: string;
  name: string;
  command_text: string;
  group_id: string | null; // 所属分组（null=根级）
}

/** 快捷命令文件夹 */
export interface QuickCommandFolder {
  id: string;
  name: string;
  parent_id: string | null;
}

/** 快捷命令树节点（serde tag "kind": "folder" | "command"，internally tagged） */
export type QuickCommandNode =
  | (QuickCommandFolder & { kind: 'folder' })
  | (QuickCommand & { kind: 'command' });

/** 隧道类型（对应 Rust enum TunnelKind：Local | Remote | Socks） */
export type TunnelKind = 'Local' | 'Remote' | 'Socks';

/** 隧道状态（对应 Rust enum TunnelStatus：Stopped | Listening | Error） */
export type TunnelStatus = 'Stopped' | 'Listening' | 'Error';

/** SSH 隧道规则 */
export interface TunnelRule {
  id: string;
  session_id: string; // 所属会话（复用其 SSH 连接）
  kind: TunnelKind;
  listen_host: string; // 默认 127.0.0.1
  listen_port: number; // u16
  target_host: string; // Local/Remote: 目标；Socks: 空串
  target_port: number; // u16
  enabled: boolean; // 会话连接时是否自动启动
  status: TunnelStatus;
  error: string | null;
}

/** 认证配置文件（一处改全局生效；SessionConfig 新增可选字段 profile_id） */
export interface AuthProfile {
  id: string;
  name: string;
  auth_type: AuthType;
}

/** SFTP 收藏路径（SQLite sftp_favorites 表；side: local | remote） */
export interface SftpFavorite {
  id: string;
  side: string;
  path: string;
  created_at: number; // Unix 秒（i64）
}

/** 隧道一键全启报告（tunnel_start_all 返回） */
export interface TunnelStartReport {
  total: number; // 规则总数
  started: number; // 本次成功启动数
  failed: number; // 启动失败数
  errors: string[]; // 失败原因列表
}

/** MySQL 连接参数（mysql_connect 入参） */
export interface MySqlConnection {
  host: string;
  port: number; // u16
  username: string;
  password: string;
  schema: string | null; // 默认选择的库（null=不选）
}

/** MySQL 表信息 */
export interface MySqlTableInfo {
  name: string;
  rows: number; // u64，估算行数
  comment: string;
  engine: string;
}

/** MySQL 查询结果（rows 内 null = SQL NULL） */
export interface MySqlQueryResult {
  columns: string[];
  rows: (string | null)[][];
  total: number; // u64，总行数
  page: number; // u32，当前页
  page_size: number; // u32，每页行数
}
