/**
 * 本地终端命令封装（services/local_shell.rs ↔ api/localShell.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 */
import { invoke } from '@tauri-apps/api/core';
import { createChannel } from './channels';

/** 终端输出块：Rust 侧 Vec<u8> 序列化后为数字数组（可能被包装为 Uint8Array） */
export type LocalShellOutputChunk = Uint8Array | number[];

/**
 * 建立本地终端连接并打开 PTY/shell，终端输出流走 Channel。
 *
 * @param connId 连接路由键（本地终端无持久会话，每标签唯一，由前端生成）
 * @param onOutput 输出回调（≤4KB 批量字节流，ANSI 解析交给 xterm.js）
 */
export function localShellConnect(
  connId: string,
  onOutput: (chunk: LocalShellOutputChunk) => void,
): Promise<void> {
  const onOutputChannel = createChannel<LocalShellOutputChunk>(onOutput);
  return invoke<void>('local_shell_connect', {
    connId,
    onOutput: onOutputChannel,
  });
}

/** 关闭并清理本地终端会话/PTY（标签关闭时调用，Rust 侧同步清理） */
export function localShellDisconnect(id: string): Promise<void> {
  return invoke<void>('local_shell_disconnect', { id });
}

/** 键盘输入写入（按连接键路由） */
export function localShellWrite(
  id: string,
  data: Uint8Array | number[],
): Promise<void> {
  // invoke 参数走 JSON 序列化：Uint8Array 直接 stringify 会变成 {"0":..} 对象，
  // 必须先归一化为 number[] 才能匹配 Rust 侧 Vec<u8>
  const payload = Array.isArray(data) ? data : Array.from(data);
  return invoke<void>('local_shell_write', { id, data: payload });
}

/** 终端尺寸变更（按连接键路由） */
export function localShellResize(
  id: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke<void>('local_shell_resize', { id, cols, rows });
}

/** 本地终端会话存活查询：Rust 侧是否持有该会话连接句柄 */
export function localShellAlive(id: string): Promise<boolean> {
  return invoke<boolean>('local_shell_alive', { id });
}
