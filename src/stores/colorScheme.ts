/**
 * 配色方案 store（Xshell 风格颜色方案管理）
 *
 * 方案列表持久化到 settings 表 color_schemes 键（JSON 数组，首次播种内置方案），
 * 当前应用方案名存 terminal_color_scheme 键。终端主题由 useXterm 联动应用。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { settingsGet, settingsSet } from '@/api/settings'
import { BUILTIN_SCHEMES, type ColorScheme } from '@/data/colorSchemes'

/** settings 表键（snake_case） */
const SCHEMES_KEY = 'color_schemes'
const APPLIED_KEY = 'terminal_color_scheme'

export const useColorSchemeStore = defineStore('colorScheme', () => {
  const schemes = ref<ColorScheme[]>([])
  const appliedName = ref('')
  const loaded = ref(false)

  /** 当前应用的方案（名称在列表中才命中） */
  const applied = computed(() => schemes.value.find((s) => s.name === appliedName.value) ?? null)

  /** 启动加载：无持久化时播种内置方案，默认应用"FyShell 默认"（保持升级前外观） */
  async function ensureLoaded(): Promise<void> {
    if (loaded.value) return
    loaded.value = true
    try {
      const raw = await settingsGet(SCHEMES_KEY)
      if (raw) {
        const parsed = JSON.parse(raw) as ColorScheme[]
        if (Array.isArray(parsed) && parsed.length > 0) {
          schemes.value = parsed
        } else {
          schemes.value = [...BUILTIN_SCHEMES]
        }
      } else {
        schemes.value = [...BUILTIN_SCHEMES]
        void settingsSet(SCHEMES_KEY, JSON.stringify(schemes.value))
      }
      appliedName.value = (await settingsGet(APPLIED_KEY)) ?? 'FyShell 默认'
    } catch (err) {
      console.error('[colorScheme] 加载配色方案失败:', err)
      schemes.value = [...BUILTIN_SCHEMES]
      appliedName.value = 'FyShell 默认'
    }
  }

  /** 按 key 持久化（失败静默：不阻塞 UI） */
  function persist(key: string, value: string): void {
    void settingsSet(key, value).catch(() => {
      /* 持久化失败不影响 UI */
    })
  }

  function persistSchemes(): void {
    persist(SCHEMES_KEY, JSON.stringify(schemes.value))
  }

  /** 切换应用方案（持久化，useXterm watch 联动实时生效） */
  function select(name: string): void {
    if (!schemes.value.some((s) => s.name === name)) return
    appliedName.value = name
    persist(APPLIED_KEY, name)
  }

  /** 新增/更新方案（同名覆盖，持久化） */
  function upsert(scheme: ColorScheme): void {
    const idx = schemes.value.findIndex((s) => s.name === scheme.name)
    if (idx >= 0) schemes.value.splice(idx, 1, scheme)
    else schemes.value.push(scheme)
    persistSchemes()
  }

  /** 删除方案；删除的是应用中方案时回退到首个剩余方案 */
  function remove(name: string): void {
    if (schemes.value.length <= 1) return
    schemes.value = schemes.value.filter((s) => s.name !== name)
    if (appliedName.value === name) {
      appliedName.value = schemes.value[0]?.name ?? ''
      persist(APPLIED_KEY, appliedName.value)
    }
    persistSchemes()
  }

  /** 名称去重：重名时追加序号（新建/另存为/导入共用） */
  function uniqueName(base: string): string {
    if (!schemes.value.some((s) => s.name === base)) return base
    let i = 2
    while (schemes.value.some((s) => s.name === `${base} ${i}`)) i++
    return `${base} ${i}`
  }

  return {
    schemes,
    appliedName,
    loaded,
    applied,
    ensureLoaded,
    select,
    upsert,
    remove,
    uniqueName,
  }
})
