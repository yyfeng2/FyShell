/**
 * Vuetify 3 全局配置
 *
 * 设计要点（对照 plans/new-version-architecture.md §3.2 技术红线）：
 * - 经典浅灰（light）为默认基调，dark 同步维护；light/dark/auto 三态主题模式
 *   的唯一来源在 stores/settings.ts（SQLite theme_mode），实际颜色经 GlobalDialog
 *   统一写入 vuetify.theme.global.name（本文件不再持有任何主题监听逻辑）
 * - 主题色（primary 等）暂用 Vuetify 默认，留待后续品牌化调整
 * - UI 密度优先：compact 模式，配合全局紧凑行高样式
 */
import { createVuetify } from 'vuetify'
import type { VuetifyOptions } from 'vuetify'
import 'vuetify/styles'
import '@mdi/font/css/materialdesignicons.css'

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
