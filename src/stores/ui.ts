/**
 * 全局 UI 状态 store
 *
 * 负责：
 * - 全局确认弹层队列（confirm 返回 Promise<boolean>，供危险操作二次确认）
 * - 全局 toast 提示队列
 * - 主题（深色/浅色）运行时两态值：持久化唯一来源是 settings store 的
 *   theme_mode（SQLite），本 store 只持有 GlobalDialog 写入 Vuetify 用的当前色
 * - 左导航折叠 / 自动隐藏状态
 * - 全局快捷键注册辅助（Ctrl+T / Ctrl+Tab / Alt+1~9 等）
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'

/** 确认弹层请求对象 */
export interface DialogRequest {
  id: number
  title: string
  message: string
  confirmText: string
  cancelText: string
  /** 危险操作：确认按钮显示为红色 */
  danger: boolean
  resolve: (ok: boolean) => void
}

/** toast 通知对象 */
export interface ToastItem {
  id: number
  message: string
  color?: string
  timeout: number
}

/** 快捷键处理函数 */
export type ShortcutHandler = (e: KeyboardEvent) => void

let uid = 0

function genId(): number {
  uid += 1
  return uid
}

export const useUiStore = defineStore('ui', () => {
  // ---------------- 全局弹层队列 ----------------

  /** 待展示的确认弹层队列（GlobalDialog 逐个消费） */
  const dialogs = ref<DialogRequest[]>([])

  /**
   * 全局确认弹层：useUiStore().confirm({ title, message }) 返回 Promise<boolean>
   * 危险操作（删除、DROP 等）二次确认统一走这里
   */
  function confirm(opts: {
    title: string
    message: string
    confirmText?: string
    cancelText?: string
    danger?: boolean
  }): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      dialogs.value.push({
        id: genId(),
        title: opts.title,
        message: opts.message,
        confirmText: opts.confirmText ?? '确认',
        cancelText: opts.cancelText ?? '取消',
        danger: opts.danger ?? false,
        resolve,
      })
    })
  }

  /** GlobalDialog 在用户点击按钮后回填结果并结束 Promise */
  function resolveDialog(id: number, ok: boolean): void {
    const idx = dialogs.value.findIndex((d) => d.id === id)
    if (idx >= 0) {
      const [req] = dialogs.value.splice(idx, 1)
      req.resolve(ok)
    }
  }

  // ---------------- Toast 提示 ----------------

  const toasts = ref<ToastItem[]>([])

  /** 全局 toast 提示；color 可传 'success' | 'error' | 'warning' 等 */
  function toast(message: string, color?: string, timeout = 3000): void {
    const item: ToastItem = { id: genId(), message, color, timeout }
    toasts.value.push(item)
    if (timeout > 0) {
      window.setTimeout(() => dismissToast(item.id), timeout)
    }
  }

  /** 手动移除某条 toast（点击关闭按钮时） */
  function dismissToast(id: number): void {
    const idx = toasts.value.findIndex((t) => t.id === id)
    if (idx >= 0) toasts.value.splice(idx, 1)
  }

  // ---------------- 主题 ----------------

  /**
   * 当前主题实际颜色（仅运行时两态载体，不做持久化）。
   * 权威来源：settings store 的 theme_mode（light/dark/auto，SQLite 持久化），
   * 启动后由 settings.ensureLoaded → applyThemeMode → setTheme 覆盖本值。
   * 初始 'light' 与 Vuetify defaultTheme 一致，避免首帧闪烁差异。
   */
  const theme = ref<'dark' | 'light'>('light')

  function setTheme(value: 'dark' | 'light'): void {
    theme.value = value
  }

  function toggleTheme(): void {
    setTheme(theme.value === 'dark' ? 'light' : 'dark')
  }

  // ---------------- 左导航折叠 ----------------

  /** 左导航是否折叠（折叠 = 收起；自动隐藏模式下鼠标悬停左缘可临时弹出） */
  const navCollapsed = ref(false)

  /** 自动隐藏模式：开启后导航悬浮于内容之上，鼠标移开即收起 */
  const navAutoHide = ref(false)

  /** 快速命令栏可见性（查看菜单切换） */
  const quickBarVisible = ref(true)

  /** 撰写栏可见性（查看菜单切换） */
  const composeBarVisible = ref(true)

  // ---------------- 菜单命令分发（终端内键位映射触发） ----------------

  /** 待分发的菜单命令（seq 递增保证同一命令连续触发也能被 watch 到） */
  const menuActionRequest = ref<{ action: string; seq: number }>({ action: '', seq: 0 })

  /** 终端内请求执行菜单命令（WorkspaceView watch 后调用 onMenuAction） */
  function requestMenuAction(action: string): void {
    menuActionRequest.value = { action, seq: menuActionRequest.value.seq + 1 }
  }

  // ---------------- 左导航宽度（可拖拽调整，持久化） ----------------

  const NAV_WIDTH_KEY = 'fyshell.navWidth'
  const NAV_WIDTH_MIN = 180
  const NAV_WIDTH_MAX = 480

  function loadNavWidth(): number {
    const saved = Number(localStorage.getItem(NAV_WIDTH_KEY))
    return Number.isFinite(saved) && saved >= NAV_WIDTH_MIN && saved <= NAV_WIDTH_MAX ? saved : 240
  }

  /** 左导航当前宽度（px），默认 240 */
  const navWidth = ref(loadNavWidth())

  /** 拖拽调整导航宽度：范围钳制到 180-480px 并持久化 */
  function setNavWidth(value: number): void {
    const v = Math.min(NAV_WIDTH_MAX, Math.max(NAV_WIDTH_MIN, Math.round(value)))
    navWidth.value = v
    localStorage.setItem(NAV_WIDTH_KEY, String(v))
  }

  function toggleNav(): void {
    navCollapsed.value = !navCollapsed.value
  }

  // ---------------- 全局快捷键注册辅助 ----------------

  /** 将键盘事件归一化为 "ctrl+t" / "alt+1" / "ctrl+tab" 形式 */
  function shortcutOf(e: KeyboardEvent): string {
    const parts: string[] = []
    if (e.ctrlKey || e.metaKey) parts.push('ctrl')
    if (e.altKey) parts.push('alt')
    if (e.shiftKey) parts.push('shift')
    const k = e.key.toLowerCase()
    // 修饰键本身不作为 key 位
    if (k !== 'control' && k !== 'alt' && k !== 'shift' && k !== 'meta') {
      parts.push(k)
    }
    return parts.join('+')
  }

  /** 快捷键 -> 处理函数集合 */
  const shortcutHandlers = ref(new Map<string, Set<ShortcutHandler>>())

  /**
   * 注册快捷键，返回注销函数（组件卸载时调用）。
   * 例：const off = ui.registerShortcut('ctrl+t', onNewTab)
   */
  function registerShortcut(keys: string | string[], handler: ShortcutHandler): () => void {
    const list = Array.isArray(keys) ? keys : [keys]
    for (const key of list) {
      let set = shortcutHandlers.value.get(key)
      if (!set) {
        set = new Set()
        shortcutHandlers.value.set(key, set)
      }
      set.add(handler)
    }
    return () => {
      for (const key of list) {
        const set = shortcutHandlers.value.get(key)
        if (set) {
          set.delete(handler)
          if (set.size === 0) shortcutHandlers.value.delete(key)
        }
      }
    }
  }

  /** window keydown 入口：命中已注册快捷键则拦截并分发 */
  function handleKeydown(e: KeyboardEvent): void {
    const key = shortcutOf(e)
    const set = shortcutHandlers.value.get(key)
    if (set && set.size > 0) {
      e.preventDefault()
      set.forEach((handler) => handler(e))
    }
  }

  return {
    dialogs,
    confirm,
    resolveDialog,
    toasts,
    toast,
    dismissToast,
    theme,
    setTheme,
    toggleTheme,
    navCollapsed,
    navAutoHide,
    toggleNav,
    navWidth,
    setNavWidth,
    quickBarVisible,
    composeBarVisible,
    menuActionRequest,
    requestMenuAction,
    shortcutOf,
    shortcutHandlers,
    registerShortcut,
    handleKeydown,
  }
})
