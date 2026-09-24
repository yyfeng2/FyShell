/**
 * MySQL Pinia store（契约 §5.2：commands/mysql.rs ↔ api/mysql.ts）
 *
 * 职责（对照 docs/ipc-contracts.md §5.1 MySQL 基础 / §5.2 MySQL 基础）：
 * - 连接管理：connect（mysql_connect 返回 connection_id）、disconnect、当前连接状态
 * - 已保存连接：以 settings 表（mysql_saved_connections 键）持久化最近使用过的连接配置，
 *   供 MySQL 工作台连接入口列表与工具条连接下拉读取；连接成功后自动回填（去重）
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
import { mysqlDbSwitch } from '@/api/mysqlDb'
import { settingsGet, settingsSet } from '@/api/settings'
import { vaultDecrypt, vaultEncrypt, vaultStatus, vaultUnlock } from '@/api/vault'
import { useUiStore } from './ui'
import type { MySqlConnection, MySqlQueryResult, MySqlTableInfo } from '@/api/types'
import { friendlyError as errText } from '@/utils/errors'

/** 对外复用契约类型（单一事实来源在 @/api/types） */
export type { MySqlConnection, MySqlQueryResult, MySqlTableInfo }

// ---------- 连接代际守卫（并发防陈旧响应） ----------
/**
 * 连接/切库统一并发守卫：任何改变「当前连接语义」的入口（connect / disconnect /
 * switchDb）先递增 connGen，异步请求发起前用 mysqlConnToken() 取快照，响应归来时
 * 用 mysqlConnTokenCurrent(token) 校验——false 表示连接/库已被切换，响应一律丢弃
 * 并抑制错误提示（此时报的错必然来自已被替换的连接/库，弹窗只会误导当前 UI）。
 *
 * 组件接入约定（优先收敛到 store，组件尽量少自己造并发控制）：
 * - 同步 ctx：在 store action 内部取 this.connId + token，await 后核对再写 state
 * - 查询级：grid 把 querySeq 作为 resultToken 传入 query()，store 仅当 token 仍为
 *   latestQueryToken 且连接未切换时才写 lastResult
 */
let connGen = 0
/** 最近一次被采纳的查询序号（resultToken 传入时登记；仅仍为最新者写 lastResult） */
let latestQueryToken = 0

/** 取当前连接代际快照（异步请求发起前调用） */
export function mysqlConnToken(): number {
  return connGen
}

/** 代际是否仍为当前（false = 连接/库已切换，响应应丢弃） */
export function mysqlConnTokenCurrent(token: number): boolean {
  return token === connGen
}

/** 登记连接上下文变更（connect / disconnect / switchDb 内部调用） */
function bumpConnGen(): void {
  connGen += 1
}

/** 过期响应哨兵（非用户可见错误：仅用于让调用方按失败短路、不落任何 UI 状态） */
class StaleResultError extends Error {
  constructor() {
    super('查询结果已过期，已丢弃')
    this.name = 'StaleResultError'
  }
}

/** 已保存连接的持久化键（后端 SQLite settings 表，key-value 文本） */
const SAVED_CONN_KEY = 'mysql_saved_connections'

/** 已保存的 MySQL 连接配置（连接入口列表与工具条连接下拉共用） */
export interface SavedMysqlConnection {
  id: string
  name: string
  host: string
  port: number
  username: string
  password: string
  schema: string | null
}

/** 生成连接配置 id（前端本地标识，仅用于保存列表） */
function uuid(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    const v = c === 'x' ? r : (r & 0x3) | 0x8
    return v.toString(16)
  })
}

/** 解析持久化的已保存连接 JSON（容错：坏条目跳过，坏格式返回空数组） */
function parseSaved(raw: string | null): SavedMysqlConnection[] {
  if (!raw) return []
  try {
    const list = JSON.parse(raw) as unknown
    if (!Array.isArray(list)) return []
    const out: SavedMysqlConnection[] = []
    for (const item of list) {
      if (!item || typeof item !== 'object') continue
      const r = item as Record<string, unknown>
      const host = typeof r.host === 'string' ? r.host : ''
      const port = Number(r.port)
      const username = typeof r.username === 'string' ? r.username : ''
      if (!host || !Number.isInteger(port) || !username) continue
      out.push({
        id: typeof r.id === 'string' && r.id ? r.id : uuid(),
        name: typeof r.name === 'string' && r.name ? r.name : `${username}@${host}`,
        host,
        port,
        username,
        password: typeof r.password === 'string' ? r.password : '',
        schema: typeof r.schema === 'string' && r.schema ? r.schema : null,
      })
    }
    return out
  } catch {
    return []
  }
}

/** 执行结果：needsConfirm=true 表示危险 SQL 待二次确认（未向后端发起执行） */
export interface MysqlExecuteOutcome {
  needsConfirm: boolean
  affected: number
}

/** 危险 SQL 判定：DROP/TRUNCATE/ALTER 开头，或 DELETE/UPDATE 无 WHERE */
const DANGEROUS_START_RE = /^\s*(DROP|TRUNCATE|ALTER)\b/i
const DANGEROUS_NO_WHERE_RE = /^\s*(DELETE|UPDATE)\b/i
const WHERE_RE = /\bWHERE\b/i

/** 后端"危险 SQL 未确认"错误的匹配：取 commands/mysql.rs reject 文案的确切特征
 *  "请确认后以 confirmed=true 重新执行"。此前宽松匹配危险/confirm 等单词，
 *  会把普通错误里恰好含表名 dangerous_data / "确认" 等文本的报错误判为需二次确认 */
const CONFIRM_HINT_RE = /请确认后以 confirmed=true 重新执行/i

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
    /** 断开标记（含异常断开）：树节点断开红色标注的数据来源，重连成功后清除 */
    dropped: false,
    /** 当前连接对应的已保存连接 id（null = 未连接或非保存连接发起） */
    activeSavedId: null as string | null,
    /** 已保存的 MySQL 连接（settings 表 mysql_saved_connections 键，重启后仍保留） */
    savedConnections: [] as SavedMysqlConnection[],
    /** 最近一次 connect 的入参（连接成功后回填保存列表用） */
    lastConfig: null as MySqlConnection | null,

    /**
     * 已存连接被主密码保护且未解锁（密码密文不可读，列表为空，由 UI 引导解锁）
     */
    vaultLocked: false as boolean,

    tables: [] as MySqlTableInfo[],
    tablesLoading: false,

    /** 待打开的表（全库查找结果等入口请求）：非空时 MysqlDataGrid watch 选中并立即执行查询 */
    pendingOpenTable: '' as string,

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
      // 连接上下文即将变更：在途旧连接/旧库的异步响应（查询、表加载等）即刻过期
      bumpConnGen()
      // 已连接时先断开旧连接（切换语义，与 connectSaved 一致）：
      // 避免直接覆盖 connId 导致后端旧连接池/事务独占连接滞留注册表
      if (this.connId) await this.disconnect()
      this.connecting = true
      this.connError = ''
      this.lastConfig = config
      try {
        this.connId = await mysqlConnect(config)
        this.connLabel = `${config.host}:${config.port} / ${config.username}`
        this.dropped = false
        // 连接成功后立即拉取表列表（失败不回滚连接，由面板展示错误）
        try {
          await this.loadTables()
        } catch (err) {
          const msg = errText(err)
          this.queryError = msg
          useUiStore().toast(msg, 'error')
        }
      } catch (err) {
        this.connId = null
        this.connLabel = ''
        const msg = errText(err)
        this.connError = msg
        useUiStore().toast(msg, 'error')
        throw err
      } finally {
        this.connecting = false
      }
    },

    /** 关闭当前连接并清空会话级状态 */
    async disconnect(): Promise<void> {
      const id = this.connId
      if (!id) return
      // 连接已断开：在途旧请求（查询/表加载）的响应一律过期丢弃
      bumpConnGen()
      try {
        await mysqlDisconnect(id)
      } finally {
        // 无论后端是否报错，本地一律复位（后端可能已清理）
        this.connId = null
        this.connLabel = ''
        this.activeSavedId = null
        this.tables = []
        this.lastResult = null
        this.pendingConfirm = null
        this.inTransaction = false
        this.queryError = ''
        this.executeMessage = ''
        this.executeError = ''
        this.dropped = true
      }
    },

    /**
     * 从后端 settings 表加载已保存连接（失败静默：列表为空，可手动新建）。
     *
     * Vault 感知：设置了主密码时，密码字段为密文（v1$…），需先解锁保险库才能解密读取；
     * 未解锁则置 vaultLocked、列表置空（明文直读路径对未设主密码的旧数据保持兼容）。
     */
    async loadSavedConnections(): Promise<void> {
      try {
        const status = await vaultStatus()
        if (status.has_master_password && !status.unlocked) {
          // 主密码保护但未解锁：密文不可读，列表置空等待解锁引导
          this.vaultLocked = true
          this.savedConnections = []
          return
        }
        this.vaultLocked = false
        const raw = await settingsGet(SAVED_CONN_KEY)
        if (status.has_master_password && raw && raw.startsWith('v1$')) {
          // 加密存储：先解密再解析（密钥在 Rust 侧内存，此路径已解锁）
          this.savedConnections = parseSaved(await vaultDecrypt(raw))
        } else {
          // 明文存储：未设主密码，或设定前遗留的存量数据
          this.savedConnections = parseSaved(raw)
        }
      } catch {
        // 后端读取失败不阻塞 UI：列表保持为空，可在连接入口手动新建
      }
    },

    /**
     * 持久化已保存连接到 settings 表。返回是否成功落盘：
     * - true：已写入，或主密码未解锁被 toast 拦截（预期跳过本次）
     * - false：加密/落盘异常（内存列表已生效但未持久化，下次变更会重试覆盖）
     */
    async persistSavedConnections(): Promise<boolean> {
      try {
        const status = await vaultStatus()
        let raw: string
        if (status.has_master_password) {
          if (!status.unlocked) {
            useUiStore().toast('已存连接受主密码保护，请先解锁后再保存', 'warning')
            return true
          }
          raw = await vaultEncrypt(JSON.stringify(this.savedConnections))
        } else {
          raw = JSON.stringify(this.savedConnections)
        }
        await settingsSet(SAVED_CONN_KEY, raw)
        return true
      } catch {
        // 加密/落盘失败不抛出：返回 false 由调用方决定如何提示
        return false
      }
    },

    /**
     * 保存连接配置：按 host+port+username 去重，已存在则更新密码/默认库。
     * 返回该条目的 id（供组件回填 activeSavedId）
     */
    saveConnection(config: MySqlConnection): string {
      // 保险库锁定：已存连接不可写入（正常路径不会触发，编辑入口已被锁定态遮挡）
      if (this.vaultLocked) {
        useUiStore().toast('已存连接受主密码保护，请先解锁后再保存', 'warning')
        return ''
      }
      const existing = this.savedConnections.find(
        (c) => c.host === config.host && c.port === config.port && c.username === config.username,
      )
      let id: string
      if (existing) {
        existing.password = config.password
        existing.schema = config.schema
        id = existing.id
      } else {
        id = uuid()
        this.savedConnections.push({
          id,
          name: `${config.username}@${config.host}`,
          host: config.host,
          port: config.port,
          username: config.username,
          password: config.password,
          schema: config.schema,
        })
      }
      this.persistSavedConnections()
      return id
    },

    /**
     * 删除已保存连接（持久化列表）；删除的是当前连接时复位 activeSavedId。
     * 返回是否成功落盘：false 时内存已同步删除但未持久化（重启后可能复活），供调用方提示。
     */
    async removeConnection(id: string): Promise<boolean> {
      this.savedConnections = this.savedConnections.filter((c) => c.id !== id)
      if (this.activeSavedId === id) this.activeSavedId = null
      return this.persistSavedConnections()
    },

    /**
     * 原地更新已保存连接（编辑连接入口：按 id 覆盖提供的字段，schema 等未提供项保留）。
     * 返回是否命中 id（false = 无此条目）
     */
    async updateConnection(
      id: string,
      config: Partial<Omit<SavedMysqlConnection, 'id'>>,
    ): Promise<boolean> {
      const existing = this.savedConnections.find((c) => c.id === id)
      if (!existing) return false
      if (config.name !== undefined) existing.name = config.name
      if (config.host !== undefined) existing.host = config.host
      if (config.port !== undefined) existing.port = config.port
      if (config.username !== undefined) existing.username = config.username
      if (config.password !== undefined) existing.password = config.password
      if (config.schema !== undefined) existing.schema = config.schema
      return this.persistSavedConnections()
    },

    /**
     * 以主密码解锁保险库后加载已存连接（解锁对话框调用）。
     * 密码错误由后端返回，向上抛出供对话框提示。
     */
    async unlockVault(password: string): Promise<void> {
      await vaultUnlock(password)
      this.vaultLocked = false
      await this.loadSavedConnections()
    },

    /** 连接指定已保存配置：已连接时先断开（切换连接语义）；失败时抛出 */
    async connectSaved(id: string): Promise<void> {
      const cfg = this.savedConnections.find((c) => c.id === id)
      if (!cfg) return
      const { host, port, username, password, schema } = cfg
      if (this.connId) await this.disconnect()
      this.activeSavedId = id
      await this.connect({ host, port, username, password, schema })
    },

    // ---------- 表列表 ----------
    /** 拉取当前库的全部表信息（表名/行数/引擎/注释）；连接/库已切换的滞留响应丢弃 */
    async loadTables(): Promise<void> {
      if (!this.connId) throw new Error('未连接 MySQL')
      const token = connGen
      this.tablesLoading = true
      try {
        const tables = await mysqlListTables(this.connId)
        if (token !== connGen) return // 连接/库已切换：丢弃陈旧表列表
        this.tables = tables
      } finally {
        this.tablesLoading = false
      }
    },

    // ---------- 查询 ----------
    /**
     * 只读查询（SELECT 类语句，分页）。结果在仍为「当前」时写入 lastResult。
     * page 从 1 开始；pageSize 为每页行数。
     *
     * resultToken：调用方发起查询的序号（MysqlDataGrid 把 querySeq 传入）。
     * 仅当 resultToken 仍是最近一次登记（即本次查询未被更新的查询覆盖）且连接/库
     * 未切换时才写 lastResult 并正常返回；过期响应当作失败短路（不写 lastResult、
     * 不污染 queryError、不弹过期错误的 toast），由 grid 层按失败丢弃，防止慢的旧
     * 查询覆盖新结果导致表格显示过期数据 / 编辑解析错表。
     */
    async query(
      sql: string,
      page: number,
      pageSize: number,
      resultToken?: number,
    ): Promise<MySqlQueryResult> {
      if (!this.connId) throw new Error('未连接 MySQL')
      // 登记本次查询代际：仅最新者写 lastResult（按实际发起顺序裁决，非完成顺序）
      if (resultToken !== undefined) latestQueryToken = resultToken
      const ctxToken = connGen
      this.queryLoading = true
      this.queryError = ''
      // 本次响应是否已过期：连接/库已切换，或已有更新的查询在途/完成
      const stale = (): boolean =>
        ctxToken !== connGen || (resultToken !== undefined && resultToken !== latestQueryToken)
      try {
        const result = await mysqlQuery(this.connId, sql, page, pageSize)
        if (stale()) throw new StaleResultError()
        this.lastResult = result
        return result
      } catch (err) {
        // 过期响应（无论实际成败）：静默丢弃，不写 queryError、不弹 toast
        if (stale()) throw err
        const msg = errText(err)
        this.queryError = msg
        useUiStore().toast(msg, 'error')
        throw err
      } finally {
        this.queryLoading = false
      }
    },

    /**
     * 切换默认数据库（后端 mysql_db_switch 重建连接池并返回新 conn_id）。
     *
     * 统一入口（工作台树双击 / 查找结果打开 / 查询 Tab「指定数据库」 / 命令列 use /
     * 工作台库下拉均走本动作，不再直接调 mysqlDbSwitch + 手动替换 connId）：
     * - 入口即递增连接代际：在途旧查询 / 旧表加载 / 旧切库结果全部过期
     * - 并发快速连点（如树双击去重、命令列连发）：仅最后一次切换回写——过期切换
     *   不覆盖 connId、成功不回写表列表、失败不弹错（旧池已断导致的必然失败静默丢弃）
     * - 返回是否本次生效（false = 已被更新的切换接管，调用方勿再更新本地状态/toast）
     *
     * ⚠️ 行为说明：切库成功后清空 lastResult / tables / queryError（沿用 disconnect 的清空
     *   方向，仅保留连接本身）——旧库结果不停留、也不可再编辑；grid 侧另有 connId watch
     *   同步清空查询区/待提交集/预览管道等会话态，之后需用户重新查询才出该库结果。
     */
    async switchDb(name: string): Promise<boolean> {
      const oldId = this.connId
      if (!oldId || !name) return false
      // 先递增代际：从此刻起旧连接/旧库的在途响应全部过期
      const token = ++connGen
      try {
        const newId = await mysqlDbSwitch(oldId, name)
        if (token !== connGen) return false // 更新的切换已接管：放弃本次回写
        this.connId = newId
        this.tables = []
        this.lastResult = null // 旧库结果立即失效：杜绝其滞留期间以新 connId 对新库误发编辑/删除
        this.queryError = ''
        await this.loadTables()
        return true
      } catch (err) {
        if (token !== connGen) return false // 过期切换失败静默（旧池已断等）
        throw err
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
        useUiStore().toast(msg, 'error')
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
