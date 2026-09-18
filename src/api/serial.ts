/**
 * 串口终端命令封装（services/serial.rs ↔ api/serial.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 */
import { invoke } from '@tauri-apps/api/core';
import { createChannel } from './channels';
import type { SerialPortInfo } from '@/bindings';

/** 终端输出块：Rust 侧 Vec<u8> 序列化后为数字数组（可能被包装为 Uint8Array） */
export type SerialOutputChunk = Uint8Array | number[];

// 串口信息与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
export type { SerialPortInfo }

/** 枚举本机串口列表（连接表单下拉选择用） */
export function serialList(): Promise<SerialPortInfo[]> {
  return invoke<SerialPortInfo[]>('serial_list');
}

/**
 * 打开串口并启动读写循环，终端输出流走 Channel。
 *
 * @param id 连接路由键（串口连接无持久会话，每标签唯一，由前端生成）
 * @param portName 串口名（如 "COM3"）
 * @param baudRate 波特率，默认 115200（Rust 侧未传时取 115200）
 * @param onOutput 输出回调（≤4KB 批量字节流）
 */
export function serialConnect(
  id: string,
  portName: string,
  baudRate: number | null,
  onOutput: (chunk: SerialOutputChunk) => void,
): Promise<void> {
  const onOutputChannel = createChannel<SerialOutputChunk>(onOutput);
  return invoke<void>('serial_connect', {
    id,
    portName,
    baudRate: baudRate ?? null,
    onOutput: onOutputChannel,
  });
}

/** 关闭并清理串口会话（Rust 侧同步清理） */
export function serialDisconnect(id: string): Promise<void> {
  return invoke<void>('serial_disconnect', { id });
}

/** 键盘输入写入（按连接键路由） */
export function serialWrite(
  id: string,
  data: Uint8Array | number[],
): Promise<void> {
  // invoke 参数走 JSON 序列化：Uint8Array 必须先归一化为 number[]
  const payload = Array.isArray(data) ? data : Array.from(data);
  return invoke<void>('serial_write', { id, data: payload });
}

/** 串口会话存活查询：Rust 侧是否持有该会话连接句柄 */
export function serialAlive(id: string): Promise<boolean> {
  return invoke<boolean>('serial_alive', { id });
}
