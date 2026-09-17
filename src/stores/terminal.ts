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
  listenSessionStatus,
} from '@/api/ssh'
import { debugLog } from '@/api/channels'

/** 分屏方向 */
export type SplitDirection = 'horizontal' | 'vertical'

/** 会话连接状态（与后端 session-status 事件同名同构） */
export type SessionStatus =
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'hostkey-verify'

/** 分屏窗格：一个窗格绑定一个 SSH 会话 */
export interface TerminalPaneState {
  paneId: string
  sessionId: string
}

/** 终端标签：一个 Tab 内可再分屏（配合 SplitLayout 组件） */
export interface TerminalTab {
  tabId: string
  title: string
  /** Tab 着色（hex，来自会话配置 color） */
  color: string | null
  panes: TerminalPaneState[]
  splitDirection: SplitDirection
  activePaneId: string
}

/** 打开终端所需的最小会话信息（来自会话树） */
export interface TerminalSessionRef {
  id: string
  name: string
  color?: string | null
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

  /** 活动标签（computed） */
  const activeTab = computed<TerminalTab | null>(
    () => tabs.value.find((t) => t.tabId === activeTabId.value) ?? null,
  )

  // ---------- 非响应式（高频路径） ----------
  /** 输出写入器：key = session_id，value = 该会话所有窗格的写入回调 */
  const outputWriters = new Map<string, Set<(data: Uint8Array) => void>>()
  /** 无窗格注册期间的输出缓冲（key = session_id），注册时一次性 flush */
  const outputHistory = new Map<string, Uint8Array[]>()
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
      if (set!.size === 0) outputWriters.delete(sessionId)
    }
  }

  /** 清理单个会话的前端侧缓存（写入器/历史/状态） */
  function cleanupSession(sessionId: string): void {
    outputWriters.delete(sessionId)
    outputHistory.delete(sessionId)
    delete sessionStatus.value[sessionId]
    delete sessionError.value[sessionId]
  }

  /** 建立连接：调 ssh_connect，输出走 Channel 回调（connId 为连接路由键，未传时用 session.id） */
  async function connectSession(session: TerminalSessionRef, connId?: string): Promise<void> {
    const key = connId ?? session.id
    ensureStatusListener()
    sessionStatus.value[key] = 'connecting'
    debugLog(`ssh_connect start: ${key}`)
    let firstChunk = true
    try {
      // 配置按 session.id 加载，连接注册/输出/状态按 key 路由
      await sshConnect(session.id, (data) => {
        // api 层回调数据可能为 number[]，归一化后推给写入器
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)
        if (firstChunk) {
          firstChunk = false
          debugLog(`first output chunk: ${key} ${bytes.byteLength} bytes`)
        }
        pushOutput(key, bytes)
      }, key)
      debugLog(`ssh_connect resolved (connected): ${key}`)
      sessionStatus.value[key] = 'connected'
    } catch (e) {
      debugLog(`ssh_connect failed: ${key} ${e instanceof Error ? e.message : String(e)}`)
      sessionStatus.value[key] = 'disconnected'
      sessionError.value[key] =
        e instanceof Error ? e.message : String(e)
      throw e
    }
  }

  /** 判断会话是否被其他窗格使用（关闭时避免误断开共享会话） */
  function sessionUsedElsewhere(sessionId: string, excludePaneId?: string): boolean {
    for (const tab of tabs.value) {
      for (const pane of tab.panes) {
        if (pane.sessionId === sessionId && pane.paneId !== excludePaneId) {
          return true
        }
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
      splitDirection: 'horizontal',
      activePaneId: paneId,
    }
    tabs.value.push(tab)
    activeTabId.value = tabId
    debugLog(`store openTerminal: new tab ${tabId} conn=${key}`)

    try {
      // 以连接键路由：多标签同会话各自独立连接，互不影响
      await connectSession({ ...session, id: key })
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

  /**
   * 标签内分屏：在指定标签中再打开一个会话窗格
   * SplitLayout 组件按 panes + splitDirection 渲染
   */
  async function splitPane(
    tabId: string,
    session: TerminalSessionRef,
    direction?: SplitDirection,
  ): Promise<TerminalTab | null> {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    if (!tab) return null
    // 同一标签内不允许重复绑定同一会话
    if (tab.panes.some((p) => p.sessionId === session.id)) return null

    const paneId = genId()
    tab.panes.push({ paneId, sessionId: session.id })
    tab.splitDirection = direction ?? tab.splitDirection
    tab.activePaneId = paneId

    try {
      // 已连接的会话直接共享（不重复 ssh_connect，避免重建断开其他窗格）
      if (!isConnected(session.id)) {
        await connectSession(session)
      }
    } catch (e) {
      // 连接失败：回滚新建窗格
      tab.panes = tab.panes.filter((p) => p.paneId !== paneId)
      if (tab.activePaneId === paneId) {
        tab.activePaneId = tab.panes.at(-1)?.paneId ?? ''
      }
      throw e
    }
    return tab
  }

  /** 切换分屏方向（垂直/水平） */
  function setSplitDirection(tabId: string, direction: SplitDirection): void {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    if (tab) tab.splitDirection = direction
  }

  /** 激活标签 */
  function activateTab(tabId: string): void {
    activeTabId.value = tabId
  }

  /** 激活标签内窗格 */
  function setActivePane(tabId: string, paneId: string): void {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    if (tab?.panes.some((p) => p.paneId === paneId)) {
      tab.activePaneId = paneId
    }
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

  /**
   * 关闭单个窗格：最后一个窗格等价于关闭标签；
   * 会话若被其他窗格共享则保留连接
   */
  async function closePane(tabId: string, paneId: string): Promise<void> {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    const pane = tab?.panes.find((p) => p.paneId === paneId)
    if (!tab || !pane) return
    if (tab.panes.length <= 1) {
      await closeTerminal(tabId)
      return
    }

    if (!sessionUsedElsewhere(pane.sessionId, pane.paneId)) {
      // 架构红线：关闭终端必须调 ssh_disconnect，让 Rust 侧同步清理 session/PTY
      await sshDisconnect(pane.sessionId).catch((e) => {
        console.warn('[terminal] ssh_disconnect 失败:', e)
      })
      cleanupSession(pane.sessionId)
    }
    tab.panes = tab.panes.filter((p) => p.paneId !== paneId)
    if (tab.activePaneId === paneId) {
      tab.activePaneId = tab.panes[0]!.paneId
    }
  }

  /** 按 sessionId 关闭终端（供工作区本地 Tab 系统的关闭回调使用）：
   *  找到包含该会话的 store 标签并完整关闭（断开 + 清理）；
   *  store 中无标签时（如连接尚未建立）直接断开并清理状态 */
  async function closeBySessionId(sessionId: string): Promise<void> {
    const tab = tabs.value.find((t) => t.panes.some((p) => p.sessionId === sessionId))
    if (!tab) {
      await sshDisconnect(sessionId).catch((e) => {
        console.warn('[terminal] ssh_disconnect 失败:', e)
      })
      cleanupSession(sessionId)
      return
    }
    await closeTerminal(tab.tabId)
  }

  /**
   * 关闭终端标签：对该标签内所有未共享会话调 ssh_disconnect
   * （架构红线：标签关闭时 Rust 侧同步清理 SSH session/PTY）
   */
  async function closeTerminal(tabId: string): Promise<void> {
    const tab = tabs.value.find((t) => t.tabId === tabId)
    if (!tab) return

    const sessionIds = new Set(tab.panes.map((p) => p.sessionId))
    for (const sessionId of sessionIds) {
      if (!sessionUsedElsewhere(sessionId)) {
        await sshDisconnect(sessionId).catch((e) => {
          console.warn('[terminal] ssh_disconnect 失败:', e)
        })
        cleanupSession(sessionId)
      }
    }

    tabs.value = tabs.value.filter((t) => t.tabId !== tabId)
    if (activeTabId.value === tabId) {
      activeTabId.value = tabs.value.at(-1)?.tabId ?? null
    }
  }

  return {
    // 状态
    tabs,
    activeTabId,
    activeTab,
    sessionStatus,
    sessionError,
    // getter
    isConnected,
    // 动作
    openTerminal,
    splitPane,
    closeBySessionId,
    closePane,
    closeTerminal,
    activateTab,
    setActivePane,
    setSplitDirection,
    activateNextTab,
    activateTabIndex,
    bindPaneWriter,
  }
})
