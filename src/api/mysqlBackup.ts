/**
 * MySQL 备份 + 自动运行命令封装（契约：commands/mysql_backup.rs ↔ api/mysqlBackup.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 */
import { invoke } from '@tauri-apps/api/core';

/** 备份/还原执行结果（行数 + 耗时；耗时由本层客户端计时得出） */
export interface MySqlBackupResult {
  rows_total: number;
  duration_ms: number;
}

/** 备份任务配置（持久化档案，mysql_backup_profile_list 返回项） */
export interface MySqlBackupProfile {
  /** 档案 id（uuid，由 Rust 侧生成） */
  id: string;
  /** 档案名称 */
  name: string;
  /** 关联连接标识（本轮为空串占位，后续接入连接选择时补齐） */
  conn_host: string;
  /** 备份表名列表；空数组 = 全库所有表 */
  tables: string[];
  include_data: boolean;
  /** true = 附带 DROP TABLE IF EXISTS（建表结构始终导出） */
  include_create: boolean;
  /** 创建时间（RFC 3339 字符串） */
  created_at: string;
}

/** 备份任务保存入参（id 为 null = 新建，uuid 由 Rust 侧生成） */
export interface MySqlBackupProfileInput {
  id: string | null;
  name: string;
  /** 空数组 = 全库（后端 tables 传 None） */
  tables: string[];
  include_data: boolean;
  include_create: boolean;
}

/** 备份运行历史单条记录 */
export interface MySqlBackupRun {
  id: number;
  /** 关联的档案 id（null = 未关联档案的手动备份） */
  profile_id: string | null;
  file_path: string;
  rows_total: number;
  duration_ms: number;
  created_at: string;
  success: boolean;
}

/**
 * 执行备份
 * - tables 传 null = 全库所有表；传数组 = 逐表备份
 * - includeData / includeCreate 未传时后端默认 true / false
 */
export async function mysqlBackup(
  connId: string,
  tables: string[] | null,
  filePath: string,
  includeData?: boolean,
  includeCreate?: boolean,
): Promise<MySqlBackupResult> {
  const start = Date.now();
  const rowsTotal = await invoke<number>('mysql_backup', {
    connId,
    tables,
    filePath,
    includeData: includeData ?? null,
    includeCreate: includeCreate ?? null,
  });
  return { rows_total: rowsTotal, duration_ms: Date.now() - start };
}

/** 还原备份文件；危险操作需传 confirmed=true（后端已由前端负责确认，直接执行） */
export async function mysqlRestore(
  connId: string,
  filePath: string,
  confirmed?: boolean,
): Promise<MySqlBackupResult> {
  const start = Date.now();
  const rowsTotal = await invoke<number>('mysql_restore', {
    connId,
    filePath,
    confirmed: confirmed ?? null,
  });
  return { rows_total: rowsTotal, duration_ms: Date.now() - start };
}

/** 列出全部备份任务配置 */
export function mysqlBackupProfileList(): Promise<MySqlBackupProfile[]> {
  return invoke<MySqlBackupProfile[]>('mysql_backup_profile_list');
}

/** 新建/更新备份任务配置（id 为 null = 新建） */
export function mysqlBackupProfileSave(profile: MySqlBackupProfileInput): Promise<void> {
  const { id, name, tables, include_data, include_create } = profile;
  return invoke<void>('mysql_backup_profile_save', {
    id,
    name,
    tables,
    include_data,
    include_create,
  });
}

/** 删除备份任务配置 */
export function mysqlBackupProfileDelete(id: string): Promise<void> {
  return invoke<void>('mysql_backup_profile_delete', { id });
}

/** 列出最近备份运行历史（后端返回最近 100 条，倒序） */
export function mysqlBackupRunHistory(): Promise<MySqlBackupRun[]> {
  return invoke<MySqlBackupRun[]>('mysql_backup_run_list');
}
