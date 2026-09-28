/**
 * FyShell IPC 契约数据类型
 *
 * 类型权威来源为 src/bindings.ts（tauri-specta 编译期生成，见 src-tauri/src/lib.rs）。
 * 命令 IPC 模型一律从 bindings.ts re-export，不再手写重复模型；
 * src/api/*.ts 封装与既有消费方保持可用。
 *
 * 保留手写的类型（无法平滑迁移的原因见各节注释）：
 * - 事件 payload（§3）：emit 事件不在命令绑定（bindings.ts）范围内
 * - SessionNode 家族：wire 格式 kind 值为 "Folder"/"Session"（Rust serde tag 无 rename），
 *   手写为小写并叠加 is_folder/children 容错归一化，迁移需改前端判断逻辑
 */

export type {
  AuthType,
  SessionConfig,
  SessionFolder,
  FileEntry,
  TransferKind,
  TransferStatus,
  TransferTask,
  QuickCommand,
  QuickCommandFolder,
  QuickCommandNode,
  TunnelKind,
  TunnelStatus,
  TunnelRule,
  AuthProfile,
  SftpFavorite,
  MySqlConnection,
  MySqlTableInfo,
  MySqlQueryResult,
  MySqlDbFindHit,
  RedisConnection,
  RedisExecResult,
} from '../bindings';

// 局部引用：手写保留的类型（SessionNodeLeaf / TransferStatusEvent）需要
import type { SessionConfig, TransferTask } from '../bindings';

/** session_test 返回值（Rust 侧类型名 TestResult，契约 `{ ok, message }`） */
export type SessionTestResult = import('../bindings').TestResult;

/** 隧道一键全启报告（Rust 侧类型名 StartAllReport） */
export type TunnelStartReport = import('../bindings').StartAllReport;

/* ============ 手写保留：SessionNode 家族（kind 值与 wire 格式存在历史漂移） ============ */

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

/* ============ §3：事件 payload（emit，仅低频状态；不在命令绑定中） ============ */

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

/** `rzsz-start` 事件：检测到 rz/sz hex 帧头哨兵；用户选择后调 rzszRespond(key, action, localPath) */
export interface RzszStartEvent {
  /** 连接路由键（多标签同会话独立连接时每标签唯一） */
  key: string;
  /** 识别的传输方向："recv"=对端 sz（选保存目录）/ "send"=对端 rz（选上传文件）/ null=无法识别（弹层手选） */
  direction: 'recv' | 'send' | null;
}

/** `rzsz-progress` 事件：rz/sz 传输进度 */
export interface RzszProgressEvent {
  key: string;
  file_name: string;
  transferred: number;
  total: number;
}

/** `rzsz-end` 事件：rz/sz 传输结束（成功/失败/取消） */
export interface RzszEndEvent {
  key: string;
  ok: boolean;
  message: string;
}
