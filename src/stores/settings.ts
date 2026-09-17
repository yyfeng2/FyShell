/**
 * 高功能设置 Pinia store
 *
 * 职责：
 * - 持有全部设置项（外观/终端/SFTP），启动时从后端 SQLite 加载（settings_get_all）、
 *   变更时写回（settings_set，key-value 文本，重启后仍保留）
 * - 主题模式（浅色/深色/跟随系统）："跟随系统"用 matchMedia 监听系统深浅色偏好，
 *   经 ui store（→ GlobalDialog → Vuetify useTheme）实时切换 Vuetify theme
 * - 终端设置项供 useXterm 读取：新终端按当前设置创建，已打开终端运行时实时生效
 */
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { settingsGetAll, settingsSet } from '@/api/settings'
import { useUiStore } from '@/stores/ui'

/** 主题模式：浅色 / 深色 / 跟随系统 */
export type ThemeMode = 'light' | 'dark' | 'auto'

/** 设置项 key（SQLite settings 表，snake_case） */
export const SETTING_KEYS = {
  themeMode: 'theme_mode',
  terminalFontSize: 'terminal_font_size',
  terminalFontFamily: 'terminal_font_family',
  terminalScrollback: 'terminal_scrollback',
  terminalCursorBlink: 'terminal_cursor_blink',
  sftpDownloadDir: 'sftp_download_dir',
} as const

/** 设置项默认值（与 useXterm 原始默认保持一致，加载失败时同样生效） */
const DEFAULTS = {
  /** 跟随系统 = 现有 dark-light-auto 行为（vuetify.ts applyInitialTheme 一致语义） */
  theme_mode: 'auto' as ThemeMode,
  terminal_font_size: 14,
  terminal_font_family: '"Cascadia Mono", Consolas, "Microsoft YaHei", monospace',
  terminal_scrollback: 10000,
  terminal_cursor_blink: true,
  sftp_download_dir: '',
}

export const useSettingsStore = defineStore('settings', () => {
  const ui = useUiStore()

  // ---------------- 设置项状态 ----------------

  /** 主题模式（浅色/深色/跟随系统） */
  const themeMode = ref<ThemeMode>(DEFAULTS.theme_mode)
  /** 终端默认字体大小（px） */
  const terminalFontSize = ref(DEFAULTS.terminal_font_size)
  /** 终端字体家族（xterm fontFamily CSS 列表） */
  const terminalFontFamily = ref(DEFAULTS.terminal_font_family)
  /** 滚动缓冲行数 */
  const terminalScrollback = ref(DEFAULTS.terminal_scrollback)
  /** 光标闪烁 */
  const terminalCursorBlink = ref(DEFAULTS.terminal_cursor_blink)
  /** SFTP 默认下载目录（空 = 使用系统下载目录） */
  const sftpDownloadDir = ref(DEFAULTS.sftp_download_dir)
  /** 是否已完成首次后端加载 */
  const loaded = ref(false)

  // ---------------- 后端加载（启动时） ----------------

  let loadPromise: Promise<void> | null = null

  /** 启动时从后端加载设置（幂等：仅在首次调用时发起请求） */
  function ensureLoaded(): Promise<void> {
    if (loadPromise) return loadPromise
    loadPromise = (async () => {
      try {
        const map = await settingsGetAll()
        const savedMode = map[SETTING_KEYS.themeMode]
        if (savedMode === 'light' || savedMode === 'dark' || savedMode === 'auto') {
          themeMode.value = savedMode
        }
        const fontSize = Number(map[SETTING_KEYS.terminalFontSize])
        if (Number.isFinite(fontSize) && fontSize > 0) {
          terminalFontSize.value = Math.round(fontSize)
        }
        const family = map[SETTING_KEYS.terminalFontFamily]
        if (family !== undefined && family !== '') terminalFontFamily.value = family
        const scrollback = Number(map[SETTING_KEYS.terminalScrollback])
        if (Number.isFinite(scrollback) && scrollback >= 0) {
          terminalScrollback.value = Math.round(scrollback)
        }
        const blink = map[SETTING_KEYS.terminalCursorBlink]
        if (blink !== undefined) terminalCursorBlink.value = blink === 'true'
        const downloadDir = map[SETTING_KEYS.sftpDownloadDir]
        if (downloadDir !== undefined) sftpDownloadDir.value = downloadDir
        loaded.value = true
        // 加载完成后应用主题模式（SQLite 优先于 localStorage 的启动缓存）
        applyThemeMode()
      } catch {
        // 加载失败不阻塞 UI：保持默认值，后续变更时按 key 逐条落库
      }
    })()
    return loadPromise
  }

  // ---------------- 主题模式（matchMedia 跟随系统） ----------------

  /** 系统主题媒体查询监听器（幂等创建，注册一次全程生效） */
  let systemThemeMedia: MediaQueryList | null = null

  /** 注册 prefers-color-scheme 监听；仅在"跟随系统"模式下响应，手动选择不受影响 */
  function ensureSystemThemeWatch(): void {
    if (systemThemeMedia || typeof window === 'undefined' || !window.matchMedia) return
    systemThemeMedia = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = (e: MediaQueryListEvent): void => {
      if (themeMode.value !== 'auto') return
      ui.setTheme(e.matches ? 'dark' : 'light')
    }
    // Safari < 14 不支持 addEventListener，做降级处理
    if (typeof systemThemeMedia.addEventListener === 'function') {
      systemThemeMedia.addEventListener('change', onChange)
    } else {
      systemThemeMedia.addListener(onChange)
    }
  }

  /** 按当前主题模式应用主题（ui.setTheme → GlobalDialog → Vuetify 全局主题） */
  function applyThemeMode(): void {
    if (themeMode.value === 'auto') {
      ensureSystemThemeWatch()
      // 立即按系统当前偏好应用一次（不等待下一次 change 事件）
      if (systemThemeMedia) {
        ui.setTheme(systemThemeMedia.matches ? 'dark' : 'light')
      }
    } else {
      ui.setTheme(themeMode.value)
    }
  }

  /** 设置主题模式：立即生效并持久化 */
  function setThemeMode(mode: ThemeMode): void {
    themeMode.value = mode
    applyThemeMode()
    persist(SETTING_KEYS.themeMode, mode)
  }

  // ---------------- 通用写回 ----------------

  /** 按 key 持久化到后端（失败静默：不阻塞 UI，下次修改会重试覆盖） */
  function persist(key: string, value: string): void {
    void settingsSet(key, value).catch(() => {
      /* 持久化失败不影响 UI */
    })
  }

  /** 终端默认字体大小（px），变更实时生效到已打开终端（useXterm 联动） */
  function setTerminalFontSize(size: number): void {
    if (!Number.isFinite(size) || size <= 0) return
    const v = Math.round(size)
    terminalFontSize.value = v
    persist(SETTING_KEYS.terminalFontSize, String(v))
  }

  /** 终端字体家族 */
  function setTerminalFontFamily(family: string): void {
    terminalFontFamily.value = family
    persist(SETTING_KEYS.terminalFontFamily, family)
  }

  /** 滚动缓冲行数 */
  function setTerminalScrollback(rows: number): void {
    if (!Number.isFinite(rows) || rows < 0) return
    const v = Math.round(rows)
    terminalScrollback.value = v
    persist(SETTING_KEYS.terminalScrollback, String(v))
  }

  /** 光标闪烁开关 */
  function setTerminalCursorBlink(blink: boolean): void {
    terminalCursorBlink.value = blink
    persist(SETTING_KEYS.terminalCursorBlink, blink ? 'true' : 'false')
  }

  /** SFTP 默认下载目录 */
  function setSftpDownloadDir(dir: string): void {
    sftpDownloadDir.value = dir
    persist(SETTING_KEYS.sftpDownloadDir, dir)
  }

  return {
    // 状态
    themeMode,
    terminalFontSize,
    terminalFontFamily,
    terminalScrollback,
    terminalCursorBlink,
    sftpDownloadDir,
    loaded,
    // 动作
    ensureLoaded,
    setThemeMode,
    setTerminalFontSize,
    setTerminalFontFamily,
    setTerminalScrollback,
    setTerminalCursorBlink,
    setSftpDownloadDir,
  }
})
