/**
 * MySQL 表设计器命令封装（契约：commands/mysql.rs ↔ api/mysqlDesign.ts）
 *
 * 组件禁止直接调用 invoke，一律通过本文件。
 * 错误处理：Rust 侧 AppError 以字符串形式 reject，由调用方捕获处理。
 * 类型命名：一律 snake_case，与 Rust 契约保持一致（禁 camelCase rename）。
 */
import { invoke } from '@tauri-apps/api/core';
import type {
  MySqlColumnInfo,
  MySqlForeignKeyInfo,
  MySqlIndexInfo,
  MySqlTableDesign,
} from '@/bindings';

// 读取类模型与后端 Rust 契约同源（bindings.ts 自动生成）：re-export 消除契约漂移源；
// 变更载荷（MySqlColumnChange / MySqlDesignChange）因前端必填约束更严，保留本地定义
export type { MySqlColumnInfo, MySqlForeignKeyInfo, MySqlIndexInfo, MySqlTableDesign }

/** 字段变更动作：add = 新增，modify = 修改，drop = 删除 */
export type MySqlColumnAction = 'add' | 'modify' | 'drop';

/** 表设计变更载荷中的字段定义 */
export interface MySqlColumnChange {
  action: MySqlColumnAction;
  name: string;
  data_type: string;
  is_nullable: boolean;
  default_value: string | null;
  comment: string;
  extra: string;
}

/** 表设计变更载荷（保存时提交给后端） */
export interface MySqlDesignChange {
  table: string;
  is_new: boolean;
  columns: MySqlColumnChange[];
  indexes: MySqlIndexInfo[];
  dropped_indexes: string[];
  foreign_keys: MySqlForeignKeyInfo[];
  dropped_foreign_keys: string[];
  engine: string | null;
  charset: string | null;
  comment: string | null;
}

/** 拉取指定表的设计信息（字段/索引/外键/表属性） */
export function mysqlTableDesignGet(connId: string, table: string): Promise<MySqlTableDesign> {
  return invoke<MySqlTableDesign>('mysql_table_design_get', { connId, table });
}

/** 保存表设计变更，返回后端生成的真实 DDL 文本 */
export function mysqlTableDesignSave(
  connId: string,
  change: MySqlDesignChange,
): Promise<string> {
  return invoke<string>('mysql_table_design_save', { connId, change });
}
