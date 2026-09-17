/**
 * MySQL Pinia store（契约 §5.2：commands/mysql.rs ↔ api/mysql.ts）
 *
 * 职责（对照 docs/ipc-contracts.md §5.1 MySQL 基础 / §5.2 MySQL 基础）：
 * - 连接管理：connect（mysql_connect 返回 connection_id）、disconnect、当前连接状态
 * - 表列表加载（mysqlListTables）、查询执行状态
 * - 危险 SQL 二次确认流程：mysql_execute 对危险 SQL（DROP/TRUNCATE/ALTER 开头或
 *   DELETE/UPDATE 无 WHERE）未确认时后端返回带提示的错误，前端捕获该错误后弹确认框，
 *   用户确认后带 confirmed 重发
 *
 * ⚠️ 契约偏差备注（2026-09-17，已解决）：
 * mysql_execute 的 confirmed 可选参数已由主会话补齐（Rust 侧 + api 层均支持），
 * executeSql 的 confirmed 参数会透传到后端。
 *
 * 本 store 不直接 invoke，统一走 @/api/mysql 封装层。
 */
import { defineStore } from 'pinia'
import {
  mysqlBegin,
  mysqlCommit,
  mysqlConnect,
  mysqlDisconnect,
  mysqlExecute,
  mysqlListTables,
  mysqlQuery,
  mysqlRollback,
} from '@/api/mysql'
import type { MySqlConnection, MySqlQueryResult, MySqlTableInfo } from '@/api/types'

/** 对外复用契约类型（单一事实来源在 @/api/types） */
export type { MySqlConnection, MySqlQueryResult, MySqlTableInfo }

/** 执行结果：needsConfirm=true 表示危险 SQL 待二次确认（未向后端发起执行） */
export interface MysqlExecuteOutcome {
  needsConfirm: boolean
  affected: number
}

/** 危险 SQL 判定：DROP/TRUNCATE/ALTER 开头，或 DELETE/UPDATE 无 WHERE */
const DANGEROUS_START_RE = /^\s*(DROP|TRUNCATE|ALTER)\b/i
const DANGEROUS_NO_WHERE_RE = /^\s*(DELETE|UPDATE)\b/i
const WHERE_RE = /\bWHERE\b/i

/** 后端"危险 SQL 未确认"错误的宽松匹配（Rust 侧提示措辞可能变化，取关键词兜底） */
const CONFIRM_HINT_RE = /confirm|确认|危险|dangerous/i

/** 危险 SQL 判定（供组件预检复用） */
export function isDangerousSql(sql: string): boolean {
  const s = sql.trim()
  if (!s) return false
  if (DANGEROUS_START_RE.test(s)) return true
  return DANGEROUS_NO_WHERE_RE.test(s) && !WHERE_RE.test(s)
}

export const useMysqlStore = defineStore('mysql', {
  state: () => ({
    /** 当前连接 ID（mysql_connect 返回；null = 未连接） */
    connId: null as string | null,
    /** 当前连接的简要描述（host:port/user），供面板展示 */
    connLabel: '' as string,
    connecting: false,
    /** 连接失败/断开信息（v-alert 展示） */
    connError: '' as string,

    tables: [] as MySqlTableInfo[],
    tablesLoading: false,

    /** 最近一次 SELECT 的查询结果（含分页信息） */
    lastResult: null as MySqlQueryResult | null,
    queryLoading: false,
    /** 查询错误信息（v-alert/toast 展示） */
    queryError: '' as string,

    executing: false,
    /** 写操作结果提示（如"受影响行数：N"） */
    executeMessage: '' as string,
    executeError: '' as string,

    /** 事务进行中标志（BEGIN 后为 true，COMMIT/ROLLBACK 后复位） */
    inTransaction: false,

    /**
     * 危险 SQL 二次确认：待确认的 SQL（null = 无待确认项）。
     * 组件据此弹出确认框，用户确认后调 confirmExecute()
     */
    pendingConfirm: null as { sql: string } | null,
  }),

  getters: {
    /** 是否已连接 */
    isConnected: (state): boolean => !!state.connId,
  },

  actions: {
    // ---------- 连接管理 ----------
    /** 建立连接并自动加载表列表；失败时抛出（组件可捕获展示） */
    async connect(config: MySqlConnection): Promise<void> {
      this.connecting = true
      this.connError = ''
      try {
        this.connId = await mysqlConnect(config)
        this.connLabel = `${config.host}:${config.port} / ${config.username}`
        // 连接成功后立即拉取表列表（失败不回滚连接，由面板展示错误）
        try {
          await this.loadTables()
        } catch (err) {
          this.queryError = errText(err)
        }
      } catch (err) {
        this.connId = null
        this.connLabel = ''
        this.connError = errText(err)
        throw err
      } finally {
        this.connecting = false
      }
    },

    /** 关闭当前连接并清空会话级状态 */
    async disconnect(): Promise<void> {
      const id = this.connId
      if (!id) return
      try {
        await mysqlDisconnect(id)
      } finally {
        // 无论后端是否报错，本地一律复位（后端可能已清理）
        this.connId = null
        this.connLabel = ''
        this.tables = []
        this.lastResult = null
        this.pendingConfirm = null
        this.inTransaction = false
        this.queryError = ''
        this.executeMessage = ''
        this.executeError = ''
      }
    },

    // ---------- 表列表 ----------
    /** 拉取当前库的全部表信息（表名/行数/引擎/注释） */
    async loadTables(): Promise<void> {
      if (!this.connId) throw new Error('未连接 MySQL')
      this.tablesLoading = true
      try {
        this.tables = await mysqlListTables(this.connId)
      } finally {
        this.tablesLoading = false
      }
    },

    // ---------- 查询 ----------
    /**
     * 只读查询（SELECT 类语句，分页）。结果写入 lastResult。
     * page 从 1 开始；pageSize 为每页行数。
     */
    async query(sql: string, page: number, pageSize: number): Promise<MySqlQueryResult> {
      if (!this.connId) throw new Error('未连接 MySQL')
      this.queryLoading = true
      this.queryError = ''
      try {
        const result = await mysqlQuery(this.connId, sql, page, pageSize)
        this.lastResult = result
        return result
      } catch (err) {
        this.queryError = errText(err)
        throw err
      } finally {
        this.queryLoading = false
      }
    },

    /**
     * 写操作（INSERT/UPDATE/DELETE/DDL），返回受影响行数。
     *
     * 危险 SQL 二次确认流程：
     * 1. 前端预检 isDangerousSql 命中且未确认 → 不发请求，置 pendingConfirm 并返回 needsConfirm
     * 2. 后端兜底：未确认的危险 SQL 后端会 reject 带提示的错误，此处捕获同样置 pendingConfirm
     * 3. 组件弹确认框，用户确认后调 confirmExecute()（带 confirmed 重发）
     */
    async executeSql(sql: string, confirmed = false): Promise<MysqlExecuteOutcome> {
      if (!this.connId) throw new Error('未连接 MySQL')
      // 前端预检：危险 SQL 且未确认，直接进入确认流程（省一次往返）
      if (!confirmed && isDangerousSql(sql)) {
        this.pendingConfirm = { sql }
        return { needsConfirm: true, affected: 0 }
      }
      this.executing = true
      this.executeError = ''
      this.executeMessage = ''
      try {
        // 危险 SQL 确认后重发时透传 confirmed=true（api 层已支持）
        const affected = await mysqlExecute(this.connId, sql, confirmed)
        this.executeMessage = `执行成功，受影响行数：${affected}`
        // 写操作可能改变表结构/行数，刷新表列表兜底
        void this.loadTables().catch(() => undefined)
        return { needsConfirm: false, affected }
      } catch (err) {
        const msg = errText(err)
        this.executeError = msg
        // 后端兜底：错误提示指向危险 SQL 未确认时进入确认流程
        if (!confirmed && CONFIRM_HINT_RE.test(msg)) {
          this.pendingConfirm = { sql }
          return { needsConfirm: true, affected: 0 }
        }
        throw err
      } finally {
        this.executing = false
      }
    },

    /** 用户在确认框中点击"确认"：带 confirmed 重发危险 SQL */
    async confirmExecute(): Promise<MysqlExecuteOutcome> {
      const pending = this.pendingConfirm
      if (!pending) return { needsConfirm: false, affected: 0 }
      this.pendingConfirm = null
      // TODO(主会话补 api 层 confirmed 参数后)：mysqlExecute(connId, sql, true)
      return this.executeSql(pending.sql, true)
    },

    /** 用户在确认框中点击"取消"：放弃本次危险操作 */
    cancelConfirm(): void {
      this.pendingConfirm = null
    },

    // ---------- 事务 ----------
    /** 开启事务（BEGIN） */
    async beginTransaction(): Promise<void> {
      if (!this.connId) throw new Error('未连接 MySQL')
      await mysqlBegin(this.connId)
      this.inTransaction = true
    },

    /** 提交事务（COMMIT） */
    async commit(): Promise<void> {
      if (!this.connId) throw new Error('未连接 MySQL')
      await mysqlCommit(this.connId)
      this.inTransaction = false
    },

    /** 回滚事务（ROLLBACK） */
    async rollback(): Promise<void> {
      if (!this.connId) throw new Error('未连接 MySQL')
      await mysqlRollback(this.connId)
      this.inTransaction = false
    },
  },
})

/** 错误归一化：Rust 侧 AppError 以字符串形式 reject */
function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}
