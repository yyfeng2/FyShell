/**
 * 会话日志命令封装（契约 5.2：commands/session_log.rs ↔ api/sessionLog.ts）
 *
 * 日志落盘位置：app_data_dir/logs/<session_id>/<日期>.log
 */
import { invoke } from '@tauri-apps/api/core';

/** 输出落盘开关（打开后该会话终端输出写入当日日志文件） */
export function sessionLogToggle(
  sessionId: string,
  enabled: boolean,
): Promise<void> {
  return invoke<void>('session_log_toggle', { sessionId, enabled });
}

/** 列出该会话已有日志的日期列表（格式由 Rust 侧落盘约定，如 "2026-09-17"） */
export function sessionLogList(sessionId: string): Promise<string[]> {
  return invoke<string[]>('session_log_list', { sessionId });
}

/** 读取指定日期的日志内容 */
export function sessionLogRead(
  sessionId: string,
  date: string,
): Promise<string> {
  return invoke<string>('session_log_read', { sessionId, date });
}
