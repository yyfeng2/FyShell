/**
 * 服务器监控命令封装（契约 5.2：commands/monitor.rs ↔ api/monitor.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 监控走 Channel 推送，无新增事件（契约 5.3）。
 */
import { invoke } from '@tauri-apps/api/core';
import { createChannel } from './channels';
import type { DockerContainer, MonitorSample } from './types';

/** 监控采样回调：收到 Rust 侧推送的每条 MonitorSample */
export type MonitorSampleHandler = (sample: MonitorSample) => void;

/**
 * 开始采集（SSH exec 采集 /proc），采样数据走 Channel 推送。
 *
 * @param id 会话 ID
 * @param intervalSecs 采样间隔（秒）
 * @param onSample 每条采样回调
 */
export function monitorStart(
  id: string,
  intervalSecs: number,
  onSample: MonitorSampleHandler,
): Promise<void> {
  const onSampleChannel = createChannel<MonitorSample>(onSample);
  return invoke<void>('monitor_start', {
    id,
    intervalSecs,
    onSample: onSampleChannel,
  });
}

/** 停止采集（组件卸载/离开监控面板时务必调用，避免 Rust 侧任务泄漏） */
export function monitorStop(id: string): Promise<void> {
  return invoke<void>('monitor_stop', { id });
}

/** 列出 Docker 容器（SSH exec docker 命令采集） */
export function dockerList(id: string): Promise<DockerContainer[]> {
  return invoke<DockerContainer[]>('docker_list', { id });
}

/** Docker 容器操作：action 为 "start" | "stop" | "restart" */
export function dockerOperate(
  id: string,
  containerId: string,
  action: string,
): Promise<void> {
  return invoke<void>('docker_operate', { id, containerId, action });
}
