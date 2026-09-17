/**
 * SQL 控制台 P2 命令封装（查询历史 + 执行计划 + 多结果集）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';
import type { MySqlQueryResult } from './types';

/** 查询历史条目（每次执行 SQL 后由 Rust 侧落库） */
export interface MySqlQueryHistoryItem {
  id: number;
  conn_id: string;
  sql: string;
  created_at: string;
  /** 执行耗时（毫秒）；null 表示未记录 */
  duration_ms: number | null;
  success: boolean;
}

/** EXPLAIN 结果：rows 表格 / tree 文本树 */
export interface MySqlExplainResult {
  format: 'rows' | 'tree';
  columns: string[];
  rows: (string | null)[][];
  /** 文本树（format="tree" 时非空） */
  tree: string | null;
}

/** 列出查询历史（默认 100 条，按时间倒序） */
export function mysqlHistoryList(limit: number): Promise<MySqlQueryHistoryItem[]> {
  return invoke<MySqlQueryHistoryItem[]>('mysql_history_list', { limit });
}

/** 按关键词搜索查询历史 */
export function mysqlHistorySearch(
  keyword: string,
  limit: number,
): Promise<MySqlQueryHistoryItem[]> {
  return invoke<MySqlQueryHistoryItem[]>('mysql_history_search', { keyword, limit });
}

/** 清空查询历史（组件调用前需经 uiStore.confirm 二次确认） */
export function mysqlHistoryClear(): Promise<void> {
  return invoke<void>('mysql_history_clear');
}

/** 执行计划（EXPLAIN）；analyze=true 时执行 EXPLAIN ANALYZE */
export function mysqlExplain(connId: string, sql: string, analyze?: boolean): Promise<MySqlExplainResult> {
  return invoke<MySqlExplainResult>('mysql_explain', { connId, sql, analyze: analyze ?? null });
}

/** 多结果集查询：一次执行多条 SELECT，逐条返回结果 */
export function mysqlQueryMulti(connId: string, sql: string): Promise<MySqlQueryResult[]> {
  return invoke<MySqlQueryResult[]>('mysql_query_multi', { connId, sql });
}
