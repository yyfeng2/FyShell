/**
 * MySQL 工具级功能命令封装（commands/mysql_tools.rs ↔ api/mysqlTools.ts）
 *
 * 数据传输/数据生成/数据同步/结构同步（对齐 Navicat 工具菜单）。
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 *
 * 命名约定：结构体字段一律 snake_case（serde 按 Rust 侧字段名反序列化，
 * 禁 rename_all="camelCase"）；invoke 顶层参数沿用 camelCase（Tauri 自动转换）。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  MySqlTransferOptions,
  MySqlTransferTableResult,
  MySqlGenerateOptions,
  MySqlGenerateColumnRule,
  MySqlDataSyncOptions,
  MySqlDataSyncOutcome,
  MySqlStructureSyncOptions,
  MySqlStructureSyncPlan,
} from '@/bindings';

// 模型类型与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源
export type {
  MySqlTransferOptions,
  MySqlTransferTableResult,
  MySqlGenerateOptions,
  MySqlGenerateColumnRule,
  MySqlDataSyncOptions,
  MySqlDataSyncOutcome,
  MySqlStructureSyncOptions,
  MySqlStructureSyncPlan,
}

/**
 * 数据传输（跨连接复制表结构与数据）。
 * 目标连接（options.targetConnId）须已通过 mysqlConnect 预先建立。
 */
export function mysqlDataTransfer(
  sourceConnId: string,
  options: MySqlTransferOptions,
): Promise<MySqlTransferTableResult[]> {
  return invoke<MySqlTransferTableResult[]>('mysql_data_transfer', { sourceConnId, options });
}

/** 数据生成（单表按列规则批量生成测试数据），返回插入行数 */
export function mysqlDataGenerate(
  connId: string,
  options: MySqlGenerateOptions,
): Promise<number> {
  return invoke<number>('mysql_data_generate', { connId, options });
}

/**
 * 数据同步（按主键比对两表数据）。execute=false 仅比对返回差异；
 * true 按选项应用（缺失行 INSERT / 多余行 DELETE / 不一致行 REPLACE）。
 */
export function mysqlDataSync(
  sourceConnId: string,
  options: MySqlDataSyncOptions,
  execute?: boolean,
): Promise<MySqlDataSyncOutcome> {
  return invoke<MySqlDataSyncOutcome>('mysql_data_sync', {
    sourceConnId,
    options,
    execute: execute ?? null,
  });
}

/**
 * 结构同步（比对两库表结构）。execute=false 仅比对返回差异计划；
 * true 在目标端执行（缺失表 CREATE / 列差异 ALTER）。
 */
export function mysqlStructureSync(
  sourceConnId: string,
  options: MySqlStructureSyncOptions,
  execute?: boolean,
): Promise<MySqlStructureSyncPlan> {
  return invoke<MySqlStructureSyncPlan>('mysql_structure_sync', {
    sourceConnId,
    options,
    execute: execute ?? null,
  });
}
