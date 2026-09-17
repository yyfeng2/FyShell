/**
 * Vuetify 3 全局配置
 *
 * 设计要点（对照 plans/new-version-architecture.md §3.2 技术红线）：
 * - 深色主题全覆盖：dark 为默认基调，light 同步维护，支持 dark-light-auto（跟随系统）切换
 * - 主题色（primary 等）暂用 Vuetify 默认，留待后续品牌化调整
 * - UI 密度优先：compact 模式，配合全局紧凑行高样式
 */
import { createVuetify } from 'vuetify'
import type { VuetifyOptions } from 'vuetify'
import 'vuetify/styles'
import '@mdi/font/css/materialdesignicons.css'

/**
 * 系统主题的媒体查询监听器（延迟创建，避免 SSR/导入期访问 window）
 */
let mediaQuery: MediaQueryList | null = null

/** 主题持久化键（与 ui store 保持一致） */
const THEME_KEY = 'fyshell.theme'

/** 主题是否已被用户手动锁定（锁定后不再跟随系统 prefers-color-scheme） */
let manualLocked = false

/**
 * 将 Vuetify 主题与系统深浅色偏好同步（dark-light-auto）
 *
 * - 初始化时按系统当前偏好设定主题
 * - 监听 prefers-color-scheme 变化，实时切换
 *
 * @param vuetify createVuetify 返回的实例
 */
export function syncThemeWithSystem(vuetify: ReturnType<typeof createVuetify>): void {
  if (typeof window === 'undefined' || !window.matchMedia) return

  mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')

  const apply = (matches: boolean) => {
    // 用户手动锁定后不再跟随系统，尊重手动选择
    if (manualLocked) return
    vuetify.theme.global.name.value = matches ? 'dark' : 'light'
  }

  apply(mediaQuery.matches)

  // Safari < 14 不支持 addEventListener，做降级处理
  if (typeof mediaQuery.addEventListener === 'function') {
    mediaQuery.addEventListener('change', (e) => apply(e.matches))
  } else {
    mediaQuery.addListener((e) => apply(e.matches))
  }
}

/** 用户手动切换主题后调用：锁定主题，停止跟随系统偏好 */
export function lockThemeToManual(): void {
  manualLocked = true
}

/** 是否仍处于"跟随系统"状态（未手动锁定） */
export function isThemeFollowingSystem(): boolean {
  return !manualLocked
}

/**
 * 初始化主题（应用根组件挂载时调用一次）
 * - 若 localStorage 已存有用户手动选择的主题：应用之并锁定
 * - 否则：跟随系统偏好（dark-light-auto）
 */
export function applyInitialTheme(
  vuetify: ReturnType<typeof createVuetify>,
): void {
  const saved =
    typeof localStorage !== 'undefined' ? localStorage.getItem(THEME_KEY) : null
  if (saved === 'light' || saved === 'dark') {
    manualLocked = true
    vuetify.theme.global.name.value = saved
  } else {
    manualLocked = false
    syncThemeWithSystem(vuetify)
  }
}

const options: Partial<VuetifyOptions> = {
  // 经典浅灰为默认基调（Xshell 传统桌面风格），深色主题保留可切换
  theme: {
    defaultTheme: 'light',
    themes: {
      dark: {
        dark: true,
        colors: {
          background: '#1A1C1E',
          surface: '#232527',
          'surface-variant': '#3A3D40',
          // 经典蓝主色调，两主题保持一致（Xshell 经典蓝）
          primary: '#2E6FDB',
          secondary: '#43BFA3',
          accent: '#7C6CFF',
          error: '#FF5C5C',
          warning: '#FFB74D',
          info: '#4FC3F7',
          success: '#66BB6A',
        },
      },
      light: {
        dark: false,
        colors: {
          // 经典浅灰色板：浅灰 chrome 分层 + 白色内容区（Xshell 传统风格）
          background: '#F5F6F7',
          surface: '#FFFFFF',
          'surface-variant': '#E8EAED',
          primary: '#2E6FDB',
          secondary: '#2FA68C',
          accent: '#6A5AE0',
          error: '#D32F2F',
          warning: '#ED6C02',
          info: '#0288D1',
          success: '#2E7D32',
        },
      },
    },
  },
  // 信息密度优先：全局紧凑显示
  defaults: {
    VBtn: { density: 'compact' },
    VTextField: { density: 'compact', variant: 'outlined' },
    VSelect: { density: 'compact', variant: 'outlined' },
    VList: { density: 'compact' },
    VDataTable: { density: 'compact' },
    VTabs: { density: 'compact' },
    VChip: { density: 'compact' },
  },
}

export const vuetify = createVuetify(options)
