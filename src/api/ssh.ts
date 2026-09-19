/**
 * SSH 终端命令封装 + 事件订阅辅助（契约第 2/3 节：commands/ssh.rs ↔ api/ssh.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { createChannel } from './channels';
import type {
  HostkeyPromptEvent,
  SessionStatusEvent,
  ZmodemEndEvent,
  ZmodemProgressEvent,
  ZmodemStartEvent,
} from './types';

/** SSH 终端输出块：Rust 侧 Vec<u8> 序列化后为数字数组（可能被包装为 Uint8Array） */
export type SshOutputChunk = Uint8Array | number[];

/**
 * 建立 SSH 连接（PTTY/shell），终端输出流走 Channel。
 *
 * @param id 会话 ID（Rust 侧按它从持久化存储加载会话配置）
 * @param onOutput 输出回调（4KB 批量字节流，ANSI 解析交给 xterm.js）
 * @param connId 连接路由键（多标签同会话独立连接时每标签唯一；不传则用会话 ID）
 */
export function sshConnect(
  id: string,
  onOutput: (chunk: SshOutputChunk) => void,
  connId?: string,
): Promise<void> {
  const onOutputChannel = createChannel<SshOutputChunk>(onOutput);
  return invoke<void>('ssh_connect', {
    id,
    connId: connId ?? null,
    onOutput: onOutputChannel,
  });
}

/** 关闭并清理 session/PTY（标签关闭时调用，Rust 侧同步清理） */
export function sshDisconnect(id: string): Promise<void> {
  return invoke<void>('ssh_disconnect', { id });
}

/** 键盘输入写入（按会话 ID 路由） */
export function sshWrite(
  id: string,
  data: Uint8Array | number[],
): Promise<void> {
  // invoke 参数走 JSON 序列化：Uint8Array 直接 stringify 会变成 {"0":..} 对象，
  // 必须先归一化为 number[] 才能匹配 Rust 侧 Vec<u8>
  const payload = Array.isArray(data) ? data : Array.from(data);
  return invoke<void>('ssh_write', { id, data: payload });
}

/** 终端尺寸变更（按会话 ID 路由） */
export function sshResize(
  id: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke<void>('ssh_resize', { id, cols, rows });
}

/** HostKey 确认结果回传（收到 hostkey-prompt 事件后由前端调用） */
export function sshHostkeyAccept(id: string, accept: boolean): Promise<void> {
  return invoke<void>('ssh_hostkey_accept', { id, accept });
}

/**
 * 订阅 `session-status` 事件（连接状态变更）。
 * @returns Promise<UnlistenFn>，调用方在组件卸载时执行
 */
export function listenSessionStatus(
  handler: (event: SessionStatusEvent) => void,
): Promise<UnlistenFn> {
  return listen<SessionStatusEvent>('session-status', (event) =>
    handler(event.payload),
  );
}

/**
 * 订阅 `hostkey-prompt` 事件（HostKey 首次确认弹层）。
 * @returns Promise<UnlistenFn>，调用方在组件卸载时执行
 */
export function listenHostkeyPrompt(
  handler: (event: HostkeyPromptEvent) => void,
): Promise<UnlistenFn> {
  return listen<HostkeyPromptEvent>('hostkey-prompt', (event) =>
    handler(event.payload),
  );
}

/** ZMODEM 传输用户选择回传（收到 zmodem-start 事件后由前端调用）：
 *  action = "recv"（localPath 为保存目录）/ "send"（localPath 为本地文件）/ "cancel" */
export function zmodemRespond(
  key: string,
  action: 'recv' | 'send' | 'cancel',
  localPath: string,
): Promise<void> {
  return invoke<void>('zmodem_respond', { key, action, localPath: localPath });
}

/** 订阅 `zmodem-start` 事件（终端 ZMODEM 传输请求弹层） */
export function listenZmodemStart(
  handler: (event: ZmodemStartEvent) => void,
): Promise<UnlistenFn> {
  return listen<ZmodemStartEvent>('zmodem-start', (event) =>
    handler(event.payload),
  );
}

/** 订阅 `zmodem-progress` 事件（ZMODEM 传输进度） */
export function listenZmodemProgress(
  handler: (event: ZmodemProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<ZmodemProgressEvent>('zmodem-progress', (event) =>
    handler(event.payload),
  );
}

/** 订阅 `zmodem-end` 事件（ZMODEM 传输结束） */
export function listenZmodemEnd(
  handler: (event: ZmodemEndEvent) => void,
): Promise<UnlistenFn> {
  return listen<ZmodemEndEvent>('zmodem-end', (event) =>
    handler(event.payload),
  );
}
