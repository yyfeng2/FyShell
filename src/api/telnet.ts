/**
 * Telnet 终端命令封装（services/telnet.rs ↔ api/telnet.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 */
import { invoke } from '@tauri-apps/api/core';
import { createChannel } from './channels';

/** 终端输出块：Rust 侧 Vec<u8> 序列化后为数字数组（可能被包装为 Uint8Array） */
export type TelnetOutputChunk = Uint8Array | number[];

/**
 * 建立 Telnet 连接（TCP + 最小 IAC 协商），终端输出流走 Channel。
 *
 * @param id 连接路由键（Telnet 连接无持久会话，每标签唯一，由前端生成）
 * @param host 主机地址
 * @param port 端口，默认 23（Rust 侧未传时取 23）
 * @param onOutput 输出回调（≤4KB 批量字节流，ANSI 解析交给 xterm.js）
 */
export function telnetConnect(
  id: string,
  host: string,
  port: number | null,
  onOutput: (chunk: TelnetOutputChunk) => void,
): Promise<void> {
  const onOutputChannel = createChannel<TelnetOutputChunk>(onOutput);
  return invoke<void>('telnet_connect', {
    id,
    host,
    port: port ?? null,
    onOutput: onOutputChannel,
  });
}

/** 关闭并清理 Telnet 会话（shutdown 双向关闭 TCP，Rust 侧同步清理） */
export function telnetDisconnect(id: string): Promise<void> {
  return invoke<void>('telnet_disconnect', { id });
}

/** 键盘输入写入（按连接键路由） */
export function telnetWrite(
  id: string,
  data: Uint8Array | number[],
): Promise<void> {
  // invoke 参数走 JSON 序列化：Uint8Array 必须先归一化为 number[]
  const payload = Array.isArray(data) ? data : Array.from(data);
  return invoke<void>('telnet_write', { id, data: payload });
}

/** Telnet 会话存活查询：Rust 侧是否持有该会话连接句柄 */
export function telnetAlive(id: string): Promise<boolean> {
  return invoke<boolean>('telnet_alive', { id });
}
