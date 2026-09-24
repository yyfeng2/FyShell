/**
 * 终端状态 Pinia store
 *
 * 职责：
 * - 标签列表：每个 Tab = 一个 SSH 会话（标签内可再分屏，窗格各自绑定会话）
 * - 打开终端：调 ssh_connect 并把输出流绑到窗格（输出按会话 ID 路由）
 * - 关闭终端：必须调 ssh_disconnect，让 Rust 侧同步清理 session/PTY（架构红线）
 * - resize 按会话 ID 路由（架构红线）
 *
 * 注意：高频输出数据不走响应式（Map + 回调），仅标签列表与状态用 ref；
 * 组件通过 bindPaneWriter 注册写入器，输出字节流直接推给对应 TerminalPane。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import {
  sshConnect,
  sshDisconnect,
  sshResize,
  sshWrite,
  listenSessionStatus,
} from '@/api/ssh'
import { localShellConnect, localShellDisconnect, localShellResize, localShellWrite } from '@/api/localShell'
import { telnetConnect, telnetDisconnect, telnetWrite } from '@/api/telnet'
import { serialConnect, serialDisconnect, serialWrite } from '@/api/serial'
import { debugLog } from '@/api/channels'

/** 会话传输类型：byte-stream 终端（本地/Telnet/串口）与 SSH 共用 TerminalPane 渲染，仅传输层命令不同 */
export type SessionType = 'ssh' | 'local' | 'telnet' | 'serial'

/** 会话连接状态（与后端 session-status 事件同名同构） */
export type SessionStatus =
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'hostkey-verify'

/** 终端窗格：一个窗格绑定一个会话（标签内当前恒为单窗格，结构保留便于后续扩展） */
export interface TerminalPaneState {
  paneId: string
  sessionId: string
}

/** 终端标签 */
export interface TerminalTab {
  tabId: string
  title: string
  /** Tab 着色（hex，来自会话配置 color） */
  color: string | null
  panes: TerminalPaneState[]
}

/** 打开终端所需的最小会话信息（来自会话树或本地/Telnet/串口新建入口） */
export interface TerminalSessionRef {
  id: string
  name: string
  color?: string | null
  /** 会话类型（默认 ssh）；本地终端/Telnet/串口为 byte-stream 终端，经各自命令读写 */
  sessionType?: SessionType
  /** Telnet 主机（sessionType = telnet 时必填） */
  host?: string
  /** Telnet 端口，默认 23 */
  port?: number
  /** 串口名（sessionType = serial 时必填，如 "COM3"） */
  serialPort?: string
  /** 串口波特率，默认 115200 */
  baudRate?: number
}

/** 生成唯一 ID（窗格/标签） */
function genId(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID()
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
}

/** 单会话输出历史缓冲上限（字节）：窗格重挂载时不丢输出 */
const OUTPUT_HISTORY_LIMIT = 128 * 1024

export const useTerminalStore = defineStore('terminal', () => {
  // ---------- 响应式状态 ----------
  /** 标签列表（每个 Tab = 一个 SSH 会话） */
  const tabs = ref<TerminalTab[]>([])
  /** 活动标签 */
  const activeTabId = ref<string | null>(null)
  /** 各会话连接状态（key = session_id） */
  const sessionStatus = ref<Record<string, SessionStatus>>({})
  /** 各会话连接失败信息（key = session_id） */
  const sessionError = ref<Record<string, string>>({})
  /** 连接成功闪光（key = session_id）：连接成功瞬间标签闪绿，1.5s 后恢复原色 */
  const sessionFlash = ref<Record<string, boolean>>({})

  /** 活动标签（computed） */
  const activeTab = computed<TerminalTab | null>(
    () => tabs.value.find((t) => t.tabId === activeTabId.value) ?? null,
  )

  // ---------- 非响应式（高频路径） ----------
  /** 输出写入器：key = session_id，value = 该会话所有窗格的写入回调 */
  const outputWriters = new Map<string, Set<(data: Uint8Array) => void>>()
  /** 无窗格注册期间的输出缓冲（key = session_id），注册时一次性 flush */
  const outputHistory = new Map<string, Uint8Array[]>()
  /** 各会话传输类型（key = 连接路由键），byte-stream 终端读写按类型路由到各自命令 */
  const sessionTypes = new Map<string, SessionType>()
  /** session-status 监听句柄（应用生命周期，store 内注册一次） */
  let unlistenStatus: (() => void) | null = null
  let statusListenerPending = false

  /** 判断会话是否处于 connected 状态 */
  function isConnected(sessionId: string): boolean {
    return sessionStatus.value[sessionId] === 'connected'
  }

  /** 注册 session-status 监听（幂等）。store 级监听与应用同生命周期，无需随组件卸载 */
  function ensureStatusListener(): void {
    if (unlistenStatus || statusListenerPending) return
    statusListenerPending = true
    void listenSessionStatus((payload) => {
      sessionStatus.value[payload.id] = payload.status
      if (payload.status === 'connected') {
        delete sessionError.value[payload.id]
      }
    })
      .then((unlisten) => {
        unlistenStatus = unlisten
        statusListenerPending = false
      })
      .catch(() => {
        statusListenerPending = false
      })
  }

  /** 输出分发：写入该会话全部窗格，并追加到历史缓冲 */
  function pushOutput(sessionId: string, data: Uint8Array): void {
    const writers = outputWriters.get(sessionId)
    if (writers) {
      debugLog(`pushOutput: ${sessionId} ${data.byteLength}B -> ${writers.size} writer(s)`)
      for (const writer of writers) {
        try {
          writer(data)
        } catch {
          // 单个窗格写入失败不影响其他窗格
        }
      }
      return
    }
    // 无写入器注册期间（连接建立到窗格挂载之间的空窗期）：缓冲输出
    let history = outputHistory.get(sessionId)
    if (!history) {
      history = []
      outputHistory.set(sessionId, history)
    }
    history.push(data)
    let total = history.reduce((sum, chunk) => sum + chunk.byteLength, 0)
    while (total > OUTPUT_HISTORY_LIMIT && history.length > 1) {
      const dropped = history.shift()
      total -= dropped?.byteLength ?? 0
    }
  }

  /** 窗格注册输出写入器，返回注销函数；注册时先补发历史缓冲 */
  function bindPaneWriter(
    sessionId: string,
    writer: (data: Uint8Array) => void,
  ): () => void {
    debugLog(`bindPaneWriter: ${sessionId}`)
    let set = outputWriters.get(sessionId)
    if (!set) {
      set = new Set()
      outputWriters.set(sessionId, set)
    }
    set.add(writer)
    // 补发注册前的输出（连接建立到窗格挂载之间的空窗期）
    const history = outputHistory.get(sessionId)
    if (history) {
      for (const data of history) {
        try {
          writer(data)
        } catch {
          // 忽略
        }
      }
    }
    outputHistory.delete(sessionId)
    return () => {
      set.delete(writer)
      // 防误删：若期间发生过 cleanupSession + 同一 connId 重新 bindPaneWriter，
      // outputWriters 中的 Set 已是新实例，旧闭包把 captured set 清空后不得再删 Map 项
      // （否则新窗格收不到任何输出 → 终端黑屏）。仅在仍是当前注册实例时清理
      if (outputWriters.get(sessionId) === set && set.size === 0) {
        outputWriters.delete(sessionId)
      }
    }
  }

  /** 清理单个会话的前端侧缓存（写入器/历史/状态/类型） */
  function cleanupSession(sessionId: string): void {
    outputWriters.delete(sessionId)
    outputHistory.delete(sessionId)
    sessionTypes.delete(sessionId)
    delete sessionStatus.value[sessionId]
    delete sessionError.value[sessionId]
    delete sessionFlash.value[sessionId]
  }

  /** 查询会话传输类型（byte-stream 终端读写路由用，默认 ssh） */
  function sessionTypeOf(sessionId: string): SessionType {
    return sessionTypes.get(sessionId) ?? 'ssh'
  }

  /**
   * 统一键盘输入写入：按会话传输类型路由到各自命令
   * （SSH → ssh_write，本地/Telnet/串口 → 各自 byte-stream 命令）
   */
  function writeTerminal(sessionId: string, data: Uint8Array): void {
    const type = sessionTypeOf(sessionId)
    if (type === 'local') {
      void localShellWrite(sessionId, data).catch(() => {
        // 会话已断开时写入失败静默忽略
      })
    } else if (type === 'telnet') {
      void telnetWrite(sessionId, data).catch(() => {
        // 忽略
      })
    } else if (type === 'serial') {
      void serialWrite(sessionId, data).catch(() => {
        // 忽略
      })
    } else {
      void sshWrite(sessionId, data).catch(() => {
        // 忽略
      })
    }
  }

  /**
   * 程序化写入（快捷命令/批量发送/终端定位等）：与 writeTerminal 同路由，
   * 但保留 Promise 让调用方可感知失败（键盘输入用 writeTerminal，程序化用本入口）
   */
  function writeToSession(sessionId: string, data: Uint8Array): Promise<void> {
    const type = sessionTypeOf(sessionId)
    if (type === 'local') return localShellWrite(sessionId, data)
    if (type === 'telnet') return telnetWrite(sessionId, data)
    if (type === 'serial') return serialWrite(sessionId, data)
    return sshWrite(sessionId, data)
  }

  /**
   * 统一终端尺寸变更：按会话传输类型路由（仅 connected 时发送）。
   * Telnet（最小实现不协商 NAWS）与串口（无尺寸概念）忽略 resize。
   */
  function resizeTerminal(sessionId: string, cols: number, rows: number): void {
    if (!isConnected(sessionId)) return
    const type = sessionTypeOf(sessionId)
    if (type === 'local') {
      void localShellResize(sessionId, cols, rows).catch(() => {
        // resize 失败静默忽略，下轮尺寸变化会重试
      })
    } else if (type === 'ssh') {
      void sshResize(sessionId, cols, rows).catch(() => {
        // 忽略
      })
    }
  }

  /**
   * 统一断开：按会话传输类型路由到各自 disconnect 命令
   * （架构红线：关闭终端必须断开，让 Rust 侧同步清理 session/PTY）
   */
  function disconnectSession(sessionId: string): Promise<void> {
    const type = sessionTypeOf(sessionId)
    if (type === 'local') return localShellDisconnect(sessionId)
    if (type === 'telnet') return telnetDisconnect(sessionId)
    if (type === 'serial') return serialDisconnect(sessionId)
    return sshDisconnect(sessionId)
  }

  /** 建立连接：按会话类型分发到各自命令（SSH 查持久化配置；本地/Telnet/串口为 byte-stream 终端），输出走 Channel 回调（connId 为连接路由键，未传时用 session.id） */
  async function connectSession(session: TerminalSessionRef, connId?: string): Promise<void> {
    const key = connId ?? session.id
    ensureStatusListener()
    sessionStatus.value[key] = 'connecting'
    const type = session.sessionType ?? 'ssh'
    // 记录传输类型：write/resize/断开均按它路由到各自命令
    sessionTypes.set(key, type)
    debugLog(`${type} connect start: ${key}`)
    let firstChunk = true
    try {
      // 输出回调：api 层回调数据可能为 number[]，归一化后推给写入器
      const onOutput = (data: unknown) => {
        // api 层回调可能为 number[]，先归一化再推给写入器
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data as ArrayLike<number>)
        if (firstChunk) {
          firstChunk = false
          debugLog(`first output chunk: ${key} ${bytes.byteLength} bytes`)
        }
        pushOutput(key, bytes)
      }
      if (type === 'local') {
        // 本地终端：无持久会话，连接路由键即标识
        await localShellConnect(key, onOutput)
      } else if (type === 'telnet') {
        await telnetConnect(key, session.host ?? '', session.port ?? null, onOutput)
      } else if (type === 'serial') {
        await serialConnect(key, session.serialPort ?? '', session.baudRate ?? null, onOutput)
      } else {
        // 配置按 session.id 加载，连接注册/输出/状态按 key 路由
        await sshConnect(session.id, onOutput, key)
      }
      debugLog(`${type} connect resolved (connected): ${key}`)
      // M2 守卫：连接期间 tab 已关闭（cleanupSession 已删除状态条目 → undefined）则不回写
      // 已销毁会话的状态，并清理刚建成但已无人接收的后端连接句柄（防已死会话复活/句柄泄漏）
      if (sessionStatus.value[key] === undefined) {
        debugLog(`${type} connect resolved after session closed, cleanup handle: ${key}`)
        void disconnectSession(key).catch(() => undefined)
        return
      }
      sessionStatus.value[key] = 'connected'
      // 连接成功峰值反馈：标签短暂闪绿（setTimeout 恢复原色）
      sessionFlash.value[key] = true
      setTimeout(() => delete sessionFlash.value[key], 1500)
    } catch (e) {
      debugLog(`${type} connect failed: ${key} ${e instanceof Error ? e.message : String(e)}`)
      // M2 守卫：会话已销毁则不回写错误状态（避免复活已死会话的状态条目）
      if (sessionStatus.value[key] !== undefined) {
        sessionStatus.value[key] = 'disconnected'
        sessionError.value[key] =
          e instanceof Error ? e.message : String(e)
      }
      throw e
    }
  }

  /** 判断会话是否被其他标签使用（关闭时避免误断开共享会话） */
  function sessionUsedElsewhere(sessionId: string): boolean {
    for (const tab of tabs.value) {
      for (const pane of tab.panes) {
        if (pane.sessionId === sessionId) return true
      }
    }
    return false
  }

  /**
   * 打开终端：建立连接并把输出流绑到窗格。
   * Xshell 多标签行为：同一会话可重复开 Tab，每个 Tab 一条独立连接
   * （connId 为连接路由键，未传时退回会话 id）
   */
  async function openTerminal(session: TerminalSessionRef, connId?: string): Promise<TerminalTab> {
    const key = connId ?? session.id
    const tabId = genId()
    const paneId = genId()
    const tab: TerminalTab = {
      tabId,
      title: session.name,
      color: session.color ?? null,
      panes: [{ paneId, sessionId: key }],
    }
    tabs.value.push(tab)
    activeTabId.value = tabId
    debugLog(`store openTerminal: new tab ${tabId} conn=${key}`)

    try {
      // 路由键 = connId（每标签独立连接）；会话配置按 session.id 加载
      await connectSession(session, key)
    } catch (e) {
      // 连接失败：回滚新建标签，错误继续抛给调用方展示
      tabs.value = tabs.value.filter((t) => t.tabId !== tabId)
      if (activeTabId.value === tabId) {
        activeTabId.value = tabs.value.at(-1)?.tabId ?? null
      }
      cleanupSession(key)
      throw e
    }
    return tab
  }

  /** 激活标签 */
  function activateTab(tabId: string): void {
    activeTabId.value = tabId
  }

  /** 激活相对偏移的标签（Ctrl+Tab / Ctrl+Shift+Tab 快捷键入口） */
  function activateNextTab(offset: number): void {
    if (tabs.value.length === 0) return
    const index = tabs.value.findIndex((t) => t.tabId === activeTabId.value)
    const next = ((index + offset) % tabs.value.length + tabs.value.length) % tabs.value.length
    activeTabId.value = tabs.value[next]!.tabId
  }

  /** 激活指定下标的标签（Alt+1~9 快捷键入口） */
  function activateTabIndex(index: number): void {
    const tab = tabs.value[index]
    if (tab) activeTabId.value = tab.tabId
  }

  /** 按 sessionId 关闭终端（供工作区本地 Tab 系统的关闭回调使用）：
   *  找到包含该会话的 store 标签并完整关闭（断开 + 清理）；
   *  store 中无标签时（如连接尚未建立）直接断开并清理状态 */
  async function closeBySessionId(sessionId: string): Promise<void> {
    const tab = tabs.value.find((t) => t.panes.some((p) => p.sessionId === sessionId))
    if (!tab) {
      await disconnectSession(sessionId).catch((e) => {
        console.warn('[terminal] disconnect 失败:', e)
      })
      cleanupSession(sessionId)
      return
    }
    await closeTerminal(tab.tabId)
  }

  /** 按连接键重命名终端标签标题（供工作区本地 Tab 系统的重命名回调使用，与 closeBySessionId 同键路由）：
   *  找到包含该会话的 store 标签并更新标题；store 中无标签时忽略 */
  function renameBySessionId(sessionId: string, title: string): void {
    const tab = tabs.value.find((t) => t.panes.some((p) => p.sessionId === sessionId))
    if (tab) tab.title = title
  }

  /**
   * 关闭终端标签：对该标签内所有未共享会话调各自 disconnect
   * （架构红线：标签关闭时 Rust 侧同步清理 session/PTY）
   */
  async function closeTerminal(tabId: string): Promise<void> {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    if (!tab) return

    const sessionIds = new Set(tab.panes.map((p) => p.sessionId))
    // 先移除标签再判定共享：正被关闭的 tab 仍在 tabs.value 时，
    // sessionUsedElsewhere 恒为 true，disconnect 与状态清理会被完全跳过
    // （曾导致 sessionStatus 残留 connected、后端连接不断开）
    tabs.value = tabs.value.filter((t) => t.tabId !== tabId)
    if (activeTabId.value === tabId) {
      activeTabId.value = tabs.value.at(-1)?.tabId ?? null
    }

    for (const sessionId of sessionIds) {
      if (!sessionUsedElsewhere(sessionId)) {
        await disconnectSession(sessionId).catch((e) => {
          console.warn('[terminal] disconnect 失败:', e)
        })
        cleanupSession(sessionId)
      }
    }
  }

  return {
    // 状态
    tabs,
    activeTabId,
    activeTab,
    sessionStatus,
    sessionError,
    sessionFlash,
    // getter
    isConnected,
    sessionTypeOf,
    // 动作
    openTerminal,
    connectSession,
    closeBySessionId,
    renameBySessionId,
    closeTerminal,
    activateTab,
    activateNextTab,
    activateTabIndex,
    bindPaneWriter,
    writeTerminal,
    writeToSession,

    resizeTerminal,
  }
})
