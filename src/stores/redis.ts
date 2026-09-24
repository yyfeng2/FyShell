/**
 * Redis Pinia store（契约：commands/redis.rs ↔ api/redis.ts）
 *
 * 职责（镜像 src/stores/mysql.ts 的连接管理 + 已保存连接模式，Redis 语义化调整）：
 * - 连接管理：connect（redis_connect 返回 connection_id，config.db 作为默认库随连接选择）、
 *   disconnect、当前连接状态（单例：连了就一个）
 * - 已保存连接：以 settings 表（redis_saved_connections 键）持久化最近使用过的连接配置，
 *   供 Redis 工作台连接入口列表与工具条连接下拉读取；连接成功后自动回填（去重）
 * - 当前库 index + 键列表加载（redisKeys(connId, pattern)）、任意命令执行（redisExec）
 *
 * 与 Redis 工作台组件智能体的接口契约（勿改状态名/action 名，工作台组件按此对接）：
 * - 单例连接：connId 非空即已连接；connect 前先断开旧连接（切换语义）
 * - db 为当前库 index：连接时由 config.db 带入（后端 connect 已选库，无需额外 select_db 往返），
 *   工作台切库走 selectDb(n)；disconnect 复位为 0
 * - pattern 为键过滤 pattern（默认 "*"），loadKeys 以 pattern || '*' 拉取
 * - keys 为当前库匹配 pattern 的键列表；keysLoading 供加载中展示
 * - 错误处理对齐 mysql store：Rust 侧 AppError 以字符串 reject，errText 归一化
 *
 * 本 store 不直接 invoke，统一走 @/api/redis 封装层。
 */
import { defineStore } from 'pinia'
import {
  redisConnect,
  redisDisconnect,
  redisExec,
  redisKeys,
  redisSelectDb,
} from '@/api/redis'
import { settingsGet, settingsSet } from '@/api/settings'
import { vaultDecrypt, vaultEncrypt, vaultStatus, vaultUnlock } from '@/api/vault'
import { useUiStore } from './ui'
import type { RedisConnection, RedisExecResult } from '@/api/types'
import { friendlyError as errText } from '@/utils/errors'

/** 对外复用契约类型（单一事实来源在 @/api/types） */
export type { RedisConnection, RedisExecResult }

// ---------- 连接代际守卫（连接/切库并发防陈旧响应，镜像 mysql store） ----------
/**
 * 任何改变「当前连接语义」的入口（connect / disconnect / selectDb）先递增 connGen；
 * loadKeys / selectDb 捕获发起时的 token，响应归来时校验仍为当前才写 state，过期写
 * 作静默丢弃（不覆盖 keys/db、不弹陈旧的错误）。快速连点切库时仅最后一次回写。
 */
let connGen = 0

function bumpConnGen(): void {
  connGen += 1
}

/** 已保存连接的持久化键（后端 SQLite settings 表，key-value 文本） */
const SAVED_CONN_KEY = 'redis_saved_connections'

/** 已保存的 Redis 连接配置（连接入口列表与工具条连接下拉共用；用户名/密码可空） */
export interface SavedRedisConnection {
  id: string
  name: string
  host: string
  port: number
  username: string | null
  password: string | null
  db: number
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
function parseSaved(raw: string | null): SavedRedisConnection[] {
  if (!raw) return []
  try {
    const list = JSON.parse(raw) as unknown
    if (!Array.isArray(list)) return []
    const out: SavedRedisConnection[] = []
    for (const item of list) {
      if (!item || typeof item !== 'object') continue
      const r = item as Record<string, unknown>
      const host = typeof r.host === 'string' ? r.host : ''
      const port = Number(r.port)
      if (!host || !Number.isInteger(port)) continue
      const username = typeof r.username === 'string' && r.username ? r.username : null
      out.push({
        id: typeof r.id === 'string' && r.id ? r.id : uuid(),
        name:
          typeof r.name === 'string' && r.name ? r.name : `${username ?? ''}@${host}`,
        host,
        port,
        username,
        password: typeof r.password === 'string' && r.password ? r.password : null,
        db: typeof r.db === 'number' && Number.isInteger(r.db) && r.db >= 0 ? r.db : 0,
      })
    }
    return out
  } catch {
    return []
  }
}

export const useRedisStore = defineStore('redis', {
  state: () => ({
    /** 当前连接 ID（redis_connect 返回；null = 未连接）；Redis 为单例连接，连了就一个 */
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
    /** 已保存的 Redis 连接（settings 表 redis_saved_connections 键，重启后仍保留） */
    savedConnections: [] as SavedRedisConnection[],
    /** 最近一次 connect 的入参（连接成功后回填保存列表/连接去重用；镜像 mysql store，附加字段） */
    lastConfig: null as RedisConnection | null,

    /** 已存连接被主密码保护且未解锁（密码密文不可读，列表为空，由 UI 引导解锁） */
    vaultLocked: false as boolean,

    /** 当前库 index（连接时由 config.db 带入，selectDb 切换） */
    db: 0 as number,
    /** 当前库匹配 pattern 的键列表 */
    keys: [] as string[],
    keysLoading: false,
    /** 键列表过滤 pattern（默认 "*"；空串按 * 处理） */
    pattern: '*' as string,
  }),

  getters: {
    /** 是否已连接 */
    isConnected: (state): boolean => !!state.connId,
  },

  actions: {
    // ---------- 连接管理 ----------
    /** 建立连接并自动加载键列表；失败时抛出（组件可捕获展示）。config.db 作为默认库随连接选择 */
    async connect(config: RedisConnection): Promise<void> {
      // 连接上下文即将变更：在途旧连接/旧库的异步响应（键列表等）即刻过期
      bumpConnGen()
      // 已连接时先断开旧连接（切换语义，与 connectSaved 一致）：
      // 避免直接覆盖 connId 导致后端旧连接池/独占连接滞留注册表
      if (this.connId) await this.disconnect()
      this.connecting = true
      this.connError = ''
      this.lastConfig = config
      // 后端 redis_connect 已携带 db 参数选择默认库，前端先把 db 记入当前状态，
      // 避免对同一配置再做一次 select_db 往返
      this.db = config.db ?? 0
      try {
        this.connId = await redisConnect(config)
        this.connLabel = `${config.host}:${config.port} / ${config.username ?? ''}`
        this.dropped = false
        // 连接成功后立即拉取键列表（失败不回滚连接，由面板展示错误）
        try {
          await this.loadKeys()
        } catch (err) {
          this.connError = errText(err)
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
      // 连接已断开：在途旧请求（键列表）的响应一律过期丢弃
      bumpConnGen()
      try {
        await redisDisconnect(id)
      } finally {
        // 无论后端是否报错，本地一律复位（后端可能已清理）
        this.connId = null
        this.connLabel = ''
        this.activeSavedId = null
        this.keys = []
        this.db = 0
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
     * 保存连接配置（name 为显示名，空则自动生成）：按 host+port+username 去重，
     * 已存在则更新密码/默认库。返回该条目的 id（供组件回填 activeSavedId）
     */
    saveConnection(name: string, config: RedisConnection): string {
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
        existing.db = config.db
        id = existing.id
      } else {
        id = uuid()
        this.savedConnections.push({
          id,
          name: name.trim() || `${config.username ?? ''}@${config.host}`,
          host: config.host,
          port: config.port,
          username: config.username,
          password: config.password,
          db: config.db,
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
     * 原地更新已保存连接（编辑连接入口：按 id 覆盖提供的字段，db 等未提供项保留）。
     * 返回是否命中 id（false = 无此条目）
     */
    async updateConnection(
      id: string,
      config: Partial<Omit<SavedRedisConnection, 'id'>>,
    ): Promise<boolean> {
      const existing = this.savedConnections.find((c) => c.id === id)
      if (!existing) return false
      if (config.name !== undefined) existing.name = config.name
      if (config.host !== undefined) existing.host = config.host
      if (config.port !== undefined) existing.port = config.port
      if (config.username !== undefined) existing.username = config.username
      if (config.password !== undefined) existing.password = config.password
      if (config.db !== undefined) existing.db = config.db
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
      const { host, port, username, password, db } = cfg
      if (this.connId) await this.disconnect()
      this.activeSavedId = id
      await this.connect({ host, port, username, password, db })
    },

    // ---------- 键列表 ----------
    /** 拉取当前库匹配 pattern 的键列表（pattern 空串按 "*" 处理）；连接已切换的滞留响应丢弃 */
    async loadKeys(): Promise<void> {
      if (!this.connId) throw new Error('未连接 Redis')
      const token = connGen
      this.keysLoading = true
      try {
        const keys = await redisKeys(this.connId, this.pattern || '*')
        if (token !== connGen) return // 连接/库已切换：丢弃陈旧键列表
        this.keys = keys
      } finally {
        this.keysLoading = false
      }
    },

    /** 刷新当前键列表（工作台刷新按钮；pattern 沿用当前值） */
    async refreshKeys(): Promise<void> {
      await this.loadKeys()
    },

    // ---------- 库与命令 ----------
    /**
     * 切换当前库（select n），成功后刷新键列表。返回是否本次生效。
     * 并发快速连点切库时仅最后一次生效：过期切换不覆盖 db、不刷新键列表、
     * 失败静默丢弃（旧库已断导致的必然失败不弹错）。
     */
    async selectDb(n: number): Promise<boolean> {
      if (!this.connId) throw new Error('未连接 Redis')
      const token = ++connGen
      try {
        await redisSelectDb(this.connId, n)
        if (token !== connGen) return false // 更新的切库已接管
        this.db = n
        await this.loadKeys()
        return true
      } catch (err) {
        if (token !== connGen) return false // 过期切库失败静默
        throw err
      }
    },

    /** 任意 Redis 命令执行（args 为命令+参数）；错误归一化为字符串抛出 */
    async exec(args: string[]): Promise<RedisExecResult> {
      if (!this.connId) throw new Error('未连接 Redis')
      try {
        return await redisExec(this.connId, args)
      } catch (err) {
        throw errText(err)
      }
    },
  },
})
