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
import { ref, type Ref } from 'vue'
import { settingsGetAll, settingsSet } from '@/api/settings'
import { useUiStore } from '@/stores/ui'

/** 主题模式：浅色 / 深色 / 跟随系统 */
export type ThemeMode = 'light' | 'dark' | 'auto'

/** 终端字体样式：常规 / 粗体 / 斜体（xterm fontWeight，斜体经容器类触发） */
export type FontFamilyStyle = 'normal' | 'bold' | 'italic'

/** 设置项 key（SQLite settings 表，snake_case） */
export const SETTING_KEYS = {
  themeMode: 'theme_mode',
  terminalFontSize: 'terminal_font_size',
  terminalFontFamily: 'terminal_font_family',
  terminalFontStyle: 'terminal_font_style',
  terminalScrollback: 'terminal_scrollback',
  terminalCursorBlink: 'terminal_cursor_blink',
  sftpDownloadDir: 'sftp_download_dir',
  mouseMiddleButton: 'mouse_middle_button',
  mouseRightButton: 'mouse_right_button',
  mouseCtrlClickMoveCursor: 'mouse_ctrl_click_move_cursor',
  mouseUrlHyperlink: 'mouse_url_hyperlink',
  mouseUrlPrefixes: 'mouse_url_prefixes',
  mouseCtrlClickOpenHyperlink: 'mouse_ctrl_click_open_hyperlink',
  selectionWordSeparators: 'selection_word_separators',
  selectionShiftDoubleClick: 'selection_shift_double_click',
  selectionAutoCopy: 'selection_auto_copy',
  selectionSoftTabs: 'selection_soft_tabs',
  selectionCopyIncludeNewline: 'selection_copy_include_newline',
  selectionCopyTrimWhitespace: 'selection_copy_trim_whitespace',
  selectionCopyNonblankOnly: 'selection_copy_nonblank_only',
  shortcutNewSession: 'shortcut_new_session',
  shortcutCloseTab: 'shortcut_close_tab',
  shortcutNextTab: 'shortcut_next_tab',
  shortcutCopy: 'shortcut_copy',
  shortcutCut: 'shortcut_cut',
  shortcutPaste: 'shortcut_paste',
  shortcutSelectAll: 'shortcut_select_all',
} as const

/** 鼠标中/右键行为：没做什么 / 粘贴剪贴板内容 */
export type MouseButtonAction = 'nothing' | 'paste'

/** 可修改快捷键默认值（settings 键 → 键位组合，ui.shortcutOf 归一化格式） */
export const SHORTCUT_DEFAULTS = {
  shortcut_new_session: 'ctrl+t',
  shortcut_close_tab: 'ctrl+w',
  shortcut_next_tab: 'ctrl+tab',
  shortcut_copy: 'ctrl+c',
  shortcut_cut: 'ctrl+x',
  shortcut_paste: 'ctrl+v',
  shortcut_select_all: 'ctrl+a',
} as const

/** 键位组合显示格式化：ctrl+t → Ctrl+T（MenuBar 标注与设置对话框共用） */
export function formatShortcutCombo(combo: string): string {
  return combo
    .split('+')
    .map((p) => {
      if (p === 'ctrl') return 'Ctrl'
      if (p === 'alt') return 'Alt'
      if (p === 'shift') return 'Shift'
      if (p === 'tab') return 'Tab'
      return p.charAt(0).toUpperCase() + p.slice(1)
    })
    .join('+')
}

/** 设置项默认值（与 useXterm 原始默认保持一致，加载失败时同样生效） */
const DEFAULTS = {
  /** 跟随系统：经 matchMedia 实时同步系统深浅色偏好（唯一监听，见 ensureSystemThemeWatch） */
  theme_mode: 'auto' as ThemeMode,
  terminal_font_size: 14,
  /** 系统默认：xterm.js 官方默认等宽字体栈（Windows 命中 Consolas，跨平台降级 Menlo/monospace） */
  terminal_font_family: 'Consolas, "Liberation Mono", Menlo, Courier, monospace',
  terminal_font_style: 'normal' as FontFamilyStyle,
  terminal_scrollback: 10000,
  terminal_cursor_blink: true,
  sftp_download_dir: '',
  mouse_middle_button: 'nothing' as MouseButtonAction,
  mouse_right_button: 'paste' as MouseButtonAction,
  mouse_ctrl_click_move_cursor: true,
  mouse_url_hyperlink: false,
  mouse_url_prefixes: 'http://|https://|ftp://|ssh://|telnet://|sftp://',
  mouse_ctrl_click_open_hyperlink: false,
  selection_word_separators: '\\:\\~\\-!@#$%^&*()-=+[]{}',
  selection_shift_double_click: false,
  selection_auto_copy: true,
  selection_soft_tabs: false,
  selection_copy_include_newline: true,
  selection_copy_trim_whitespace: false,
  selection_copy_nonblank_only: false,
  ...SHORTCUT_DEFAULTS,
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
  /** 终端字体样式（常规/粗体/斜体，xterm fontWeight，斜体经容器类触发） */
  const terminalFontStyle = ref<FontFamilyStyle>(DEFAULTS.terminal_font_style)
  /** 滚动缓冲行数 */
  const terminalScrollback = ref(DEFAULTS.terminal_scrollback)
  /** 光标闪烁 */
  const terminalCursorBlink = ref(DEFAULTS.terminal_cursor_blink)
  /** SFTP 默认下载目录（空 = 使用系统下载目录） */
  const sftpDownloadDir = ref(DEFAULTS.sftp_download_dir)
  /** 鼠标中键行为 */
  const mouseMiddleButton = ref<MouseButtonAction>(DEFAULTS.mouse_middle_button)
  /** 鼠标右键行为 */
  const mouseRightButton = ref<MouseButtonAction>(DEFAULTS.mouse_right_button)
  /** Ctrl+左单击移动终端光标 */
  const mouseCtrlClickMoveCursor = ref(DEFAULTS.mouse_ctrl_click_move_cursor)
  /** 使用 URL 超链接 */
  const mouseUrlHyperlink = ref(DEFAULTS.mouse_url_hyperlink)
  /** URL 前缀（| 分隔） */
  const mouseUrlPrefixes = ref(DEFAULTS.mouse_url_prefixes)
  /** [Ctrl+单击] 以打开超链接 */
  const mouseCtrlClickOpenHyperlink = ref(DEFAULTS.mouse_ctrl_click_open_hyperlink)
  /** 双击选择分隔符 */
  const selectionWordSeparators = ref(DEFAULTS.selection_word_separators)
  /** Shift+双击按分隔符而非空格选择 */
  const selectionShiftDoubleClick = ref(DEFAULTS.selection_shift_double_click)
  /** 选中文本自动复制到剪贴板 */
  const selectionAutoCopy = ref(DEFAULTS.selection_auto_copy)
  /** 复制时制表符转空格 */
  const selectionSoftTabs = ref(DEFAULTS.selection_soft_tabs)
  /** 复制包含最后一个换行字符 */
  const selectionCopyIncludeNewline = ref(DEFAULTS.selection_copy_include_newline)
  /** 复制时删除尾部空白 */
  const selectionCopyTrimWhitespace = ref(DEFAULTS.selection_copy_trim_whitespace)
  /** 复制时排除仅含空白的行 */
  const selectionCopyNonblankOnly = ref(DEFAULTS.selection_copy_nonblank_only)
  /** 快捷键设置（ui.shortcutOf 归一化格式，如 ctrl+t；设置对话框可修改） */
  const shortcutNewSession = ref(DEFAULTS.shortcut_new_session)
  const shortcutCloseTab = ref(DEFAULTS.shortcut_close_tab)
  const shortcutNextTab = ref(DEFAULTS.shortcut_next_tab)
  const shortcutCopy = ref(DEFAULTS.shortcut_copy)
  const shortcutCut = ref(DEFAULTS.shortcut_cut)
  const shortcutPaste = ref(DEFAULTS.shortcut_paste)
  const shortcutSelectAll = ref(DEFAULTS.shortcut_select_all)
  /** 快捷键设置索引：snake_case key → ref（后端加载与写回共用遍历） */
  const shortcutRefs: Record<string, Ref<string>> = {
    [SETTING_KEYS.shortcutNewSession]: shortcutNewSession,
    [SETTING_KEYS.shortcutCloseTab]: shortcutCloseTab,
    [SETTING_KEYS.shortcutNextTab]: shortcutNextTab,
    [SETTING_KEYS.shortcutCopy]: shortcutCopy,
    [SETTING_KEYS.shortcutCut]: shortcutCut,
    [SETTING_KEYS.shortcutPaste]: shortcutPaste,
    [SETTING_KEYS.shortcutSelectAll]: shortcutSelectAll,
  }
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
        const fontStyle = map[SETTING_KEYS.terminalFontStyle]
        if (fontStyle === 'normal' || fontStyle === 'bold' || fontStyle === 'italic') {
          terminalFontStyle.value = fontStyle
        }
        const scrollback = Number(map[SETTING_KEYS.terminalScrollback])
        if (Number.isFinite(scrollback) && scrollback >= 0) {
          terminalScrollback.value = Math.round(scrollback)
        }
        const blink = map[SETTING_KEYS.terminalCursorBlink]
        if (blink !== undefined) terminalCursorBlink.value = blink === 'true'
        const downloadDir = map[SETTING_KEYS.sftpDownloadDir]
        if (downloadDir !== undefined) sftpDownloadDir.value = downloadDir
        const middleBtn = map[SETTING_KEYS.mouseMiddleButton]
        if (middleBtn === 'nothing' || middleBtn === 'paste') {
          mouseMiddleButton.value = middleBtn
        }
        const rightBtn = map[SETTING_KEYS.mouseRightButton]
        if (rightBtn === 'nothing' || rightBtn === 'paste') {
          mouseRightButton.value = rightBtn
        }
        const ctrlMove = map[SETTING_KEYS.mouseCtrlClickMoveCursor]
        if (ctrlMove !== undefined) mouseCtrlClickMoveCursor.value = ctrlMove === 'true'
        const urlLink = map[SETTING_KEYS.mouseUrlHyperlink]
        if (urlLink !== undefined) mouseUrlHyperlink.value = urlLink === 'true'
        const urlPrefixes = map[SETTING_KEYS.mouseUrlPrefixes]
        if (urlPrefixes !== undefined && urlPrefixes !== '') mouseUrlPrefixes.value = urlPrefixes
        const ctrlOpen = map[SETTING_KEYS.mouseCtrlClickOpenHyperlink]
        if (ctrlOpen !== undefined) mouseCtrlClickOpenHyperlink.value = ctrlOpen === 'true'
        const wordSeps = map[SETTING_KEYS.selectionWordSeparators]
        if (wordSeps !== undefined) selectionWordSeparators.value = wordSeps
        const shiftDbl = map[SETTING_KEYS.selectionShiftDoubleClick]
        if (shiftDbl !== undefined) selectionShiftDoubleClick.value = shiftDbl === 'true'
        const autoCopy = map[SETTING_KEYS.selectionAutoCopy]
        if (autoCopy !== undefined) selectionAutoCopy.value = autoCopy === 'true'
        const softTabs = map[SETTING_KEYS.selectionSoftTabs]
        if (softTabs !== undefined) selectionSoftTabs.value = softTabs === 'true'
        const copyNewline = map[SETTING_KEYS.selectionCopyIncludeNewline]
        if (copyNewline !== undefined) selectionCopyIncludeNewline.value = copyNewline === 'true'
        const trimWs = map[SETTING_KEYS.selectionCopyTrimWhitespace]
        if (trimWs !== undefined) selectionCopyTrimWhitespace.value = trimWs === 'true'
        const nonblankOnly = map[SETTING_KEYS.selectionCopyNonblankOnly]
        if (nonblankOnly !== undefined) selectionCopyNonblankOnly.value = nonblankOnly === 'true'
        // 快捷键设置：非空即采用（ui.shortcutOf 归一化格式，设置对话框负责校验）
        for (const [key, refItem] of Object.entries(shortcutRefs)) {
          const v = map[key]
          if (v !== undefined && v !== '') refItem.value = v
        }
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

  /** 终端字体样式（常规/粗体/斜体），变更实时生效到已打开终端（useXterm 联动） */
  function setTerminalFontStyle(style: FontFamilyStyle): void {
    terminalFontStyle.value = style
    persist(SETTING_KEYS.terminalFontStyle, style)
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

  /** 鼠标中键行为 */
  function setMouseMiddleButton(action: MouseButtonAction): void {
    mouseMiddleButton.value = action
    persist(SETTING_KEYS.mouseMiddleButton, action)
  }

  /** 鼠标右键行为 */
  function setMouseRightButton(action: MouseButtonAction): void {
    mouseRightButton.value = action
    persist(SETTING_KEYS.mouseRightButton, action)
  }

  /** Ctrl+左单击移动终端光标 */
  function setMouseCtrlClickMoveCursor(enabled: boolean): void {
    mouseCtrlClickMoveCursor.value = enabled
    persist(SETTING_KEYS.mouseCtrlClickMoveCursor, enabled ? 'true' : 'false')
  }

  /** 使用 URL 超链接 */
  function setMouseUrlHyperlink(enabled: boolean): void {
    mouseUrlHyperlink.value = enabled
    persist(SETTING_KEYS.mouseUrlHyperlink, enabled ? 'true' : 'false')
  }

  /** URL 前缀 */
  function setMouseUrlPrefixes(prefixes: string): void {
    mouseUrlPrefixes.value = prefixes
    persist(SETTING_KEYS.mouseUrlPrefixes, prefixes)
  }

  /** [Ctrl+单击] 以打开超链接 */
  function setMouseCtrlClickOpenHyperlink(enabled: boolean): void {
    mouseCtrlClickOpenHyperlink.value = enabled
    persist(SETTING_KEYS.mouseCtrlClickOpenHyperlink, enabled ? 'true' : 'false')
  }

  /** 双击选择分隔符 */
  function setSelectionWordSeparators(seps: string): void {
    selectionWordSeparators.value = seps
    persist(SETTING_KEYS.selectionWordSeparators, seps)
  }

  /** Shift+双击按分隔符而非空格选择 */
  function setSelectionShiftDoubleClick(enabled: boolean): void {
    selectionShiftDoubleClick.value = enabled
    persist(SETTING_KEYS.selectionShiftDoubleClick, enabled ? 'true' : 'false')
  }

  /** 选中文本自动复制到剪贴板 */
  function setSelectionAutoCopy(enabled: boolean): void {
    selectionAutoCopy.value = enabled
    persist(SETTING_KEYS.selectionAutoCopy, enabled ? 'true' : 'false')
  }

  /** 复制时制表符转空格 */
  function setSelectionSoftTabs(enabled: boolean): void {
    selectionSoftTabs.value = enabled
    persist(SETTING_KEYS.selectionSoftTabs, enabled ? 'true' : 'false')
  }

  /** 复制包含最后一个换行字符 */
  function setSelectionCopyIncludeNewline(enabled: boolean): void {
    selectionCopyIncludeNewline.value = enabled
    persist(SETTING_KEYS.selectionCopyIncludeNewline, enabled ? 'true' : 'false')
  }

  /** 复制时删除尾部空白 */
  function setSelectionCopyTrimWhitespace(enabled: boolean): void {
    selectionCopyTrimWhitespace.value = enabled
    persist(SETTING_KEYS.selectionCopyTrimWhitespace, enabled ? 'true' : 'false')
  }

  /** 复制时排除仅含空白的行 */
  function setSelectionCopyNonblankOnly(enabled: boolean): void {
    selectionCopyNonblankOnly.value = enabled
    persist(SETTING_KEYS.selectionCopyNonblankOnly, enabled ? 'true' : 'false')
  }

  /** 读取快捷键设置（snake_case key → 归一化键位组合，未配置时返回默认值） */
  function shortcutValue(key: string): string {
    return shortcutRefs[key]?.value ?? ''
  }

  /** 修改快捷键设置（value 为 ui.shortcutOf 归一化格式，设置对话框负责校验），立即持久化 */
  function setShortcut(key: string, value: string): void {
    const refItem = shortcutRefs[key]
    if (!refItem) return
    refItem.value = value
    persist(key, value)
  }

  return {
    // 状态
    themeMode,
    terminalFontSize,
    terminalFontFamily,
    terminalFontStyle,
    terminalScrollback,
    terminalCursorBlink,
    sftpDownloadDir,
    mouseMiddleButton,
    mouseRightButton,
    mouseCtrlClickMoveCursor,
    mouseUrlHyperlink,
    mouseUrlPrefixes,
    mouseCtrlClickOpenHyperlink,
    selectionWordSeparators,
    selectionShiftDoubleClick,
    selectionAutoCopy,
    selectionSoftTabs,
    selectionCopyIncludeNewline,
    selectionCopyTrimWhitespace,
    selectionCopyNonblankOnly,
    shortcutNewSession,
    shortcutCloseTab,
    shortcutNextTab,
    shortcutCopy,
    shortcutCut,
    shortcutPaste,
    shortcutSelectAll,
    loaded,
    // 动作
    ensureLoaded,
    shortcutValue,
    setShortcut,
    setThemeMode,
    setTerminalFontSize,
    setTerminalFontFamily,
    setTerminalFontStyle,
    setTerminalScrollback,
    setTerminalCursorBlink,
    setSftpDownloadDir,
    setMouseMiddleButton,
    setMouseRightButton,
    setMouseCtrlClickMoveCursor,
    setMouseUrlHyperlink,
    setMouseUrlPrefixes,
    setMouseCtrlClickOpenHyperlink,
    setSelectionWordSeparators,
    setSelectionShiftDoubleClick,
    setSelectionAutoCopy,
    setSelectionSoftTabs,
    setSelectionCopyIncludeNewline,
    setSelectionCopyTrimWhitespace,
    setSelectionCopyNonblankOnly,
  }
})
