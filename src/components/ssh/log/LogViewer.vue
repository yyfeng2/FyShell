<template>
  <div class="log-viewer">
    <!-- 工具栏：日志记录开关 + 搜索 + 刷新 -->
    <div class="log-viewer__toolbar">
      <v-switch
        :model-value="logStore.isEnabled(sessionId)"
        label="记录会话日志"
        density="compact"
        hide-details
        color="primary"
        class="log-viewer__switch"
        @update:model-value="toggle"
      />
      <v-text-field
        v-model="keyword"
        density="compact"
        variant="outlined"
        hide-details
        clearable
        aria-label="搜索日志"
        placeholder="搜索日志…"
        prepend-inner-icon="mdi-magnify"
        class="log-viewer__search"
      />
      <v-btn icon="mdi-refresh" size="18" variant="text" density="comfortable" title="刷新日志列表" :loading="loadingDates" @click="refresh" />
    </div>
    <v-divider />

    <v-alert v-if="error" type="error" variant="tonal" density="compact" class="ma-2" closable @click:close="error = null">
      {{ error }}
    </v-alert>

    <!-- 内容加载顶部细条（inactive 时高度塌陷为 0，不占布局） -->
    <v-progress-linear :active="loadingContent" :indeterminate="loadingContent" height="2" color="primary" />

    <!-- 主体：左日期列表 + 右日志内容 -->
    <div class="log-viewer__body">
      <div class="log-viewer__dates">
        <div
          v-for="d in dates"
          :key="d"
          class="log-viewer__date"
          :class="{ 'log-viewer__date--active': d === activeDate }"
          @click="selectDate(d)"
        >
          <v-icon size="12" class="mr-1">mdi-calendar-blank-outline</v-icon>
          {{ d }}
        </div>
        <EmptyState v-if="dates.length === 0" size="compact" icon="mdi-history" title="暂无日志" desc="连接会话后日志将在此记录" />
      </div>

      <div ref="contentRef" class="log-viewer__content" @scroll="onContentScroll">
        <!-- 等宽字体 pre 块，ANSI SGR 转为带颜色的 HTML -->
        <pre class="log-viewer__text" v-html="highlightedHtml" />
        <EmptyState v-if="dateContent === '' && dates.length > 0" size="compact" icon="mdi-calendar-blank" title="该日期暂无内容" desc="所选日期没有匹配的日志记录" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * LogViewer —— 会话日志查看器（契约 5.2 / 架构 §3.2 会话日志）
 *
 * 功能：
 * - 日期列表 + 日志内容展示（等宽字体 pre 块、可搜索、彩色 ANSI）
 * - 开关会话日志记录（session_log_toggle）
 *
 * ANSI 解析：内置轻量 SGR 解析器（16 色 + 256 色 + 粗体/斜体/下划线），
 * 把日志文本转为带颜色的 HTML，不依赖 xterm 渲染，保证可搜索可复制。
 * 状态经 @/stores/log 缓存：日期列表与已读内容不重复走 IPC。
 */
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useTheme } from 'vuetify'
import { useLogStore } from '@/stores/log'
import { useUiStore } from '@/stores/ui'
import EmptyState from '@/components/common/EmptyState.vue'
import { friendlyError as errText } from '@/utils/errors'

const props = defineProps<{
  /** 会话 ID（对应 logs/<session_id>/日期.log） */
  sessionId: string
}>()

const logStore = useLogStore()
const ui = useUiStore()

// ---------- 日期列表 ----------
const dates = computed(() => logStore.cachedDates(props.sessionId))
const activeDate = ref<string | null>(null)
const error = ref<string | null>(null)
/** 日期列表加载中 */
const loadingDates = ref(false)
/** 日志内容加载中 */
const loadingContent = ref(false)
/** 当前展示日期的日志内容（ANSI 转义前原文） */
const dateContent = ref('')

// ---------- 内容区自动滚动 ----------
/** 日志内容滚动容器 */
const contentRef = ref<HTMLElement | null>(null)
/** 用户是否位于底部附近（在该状态下方自动跟随滚动到底） */
const followBottom = ref(true)

function onContentScroll(): void {
  const el = contentRef.value
  if (!el) return
  followBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 40
}

function scrollToBottomIfNeeded(): void {
  if (!followBottom.value) return
  const el = contentRef.value
  if (el) el.scrollTop = el.scrollHeight
}

async function loadDates(): Promise<void> {
  activeDate.value = null
  dateContent.value = ''
  loadingDates.value = true
  try {
    const list = await logStore.loadDates(props.sessionId)
    // 默认选中最近一天（列表按日期正序返回）
    if (dates.value.length > 0) {
      await selectDate(dates.value[dates.value.length - 1])
    }
  } catch (e) {
    error.value = `加载日志日期列表失败：${errText(e)}`
  } finally {
    loadingDates.value = false
  }
}

async function selectDate(date: string): Promise<void> {
  activeDate.value = date
  loadingContent.value = true
  try {
    dateContent.value = await logStore.read(props.sessionId, date)
  } catch (e) {
    dateContent.value = ''
    error.value = `读取日志失败：${errText(e)}`
  } finally {
    loadingContent.value = false
  }
}

function refresh(): void {
  logStore.invalidate(props.sessionId)
  void loadDates()
}

// ---------- 日志记录开关 ----------
async function toggle(enabled: boolean | null): Promise<void> {
  const value = enabled ?? false
  try {
    await logStore.toggle(props.sessionId, value)
    ui.toast(value ? '已开启会话日志记录' : '已关闭会话日志记录', 'info')
  } catch (e) {
    ui.toast(`切换日志记录失败：${errText(e)}`, 'error')
  }
}

// ---------- 搜索 ----------
const keyword = ref('')

/** ANSI 转义后的 HTML + 搜索高亮 */
const highlightedHtml = computed(() => {
  const html = ansiToHtml(dateContent.value)
  return highlightHtml(html, keyword.value.trim())
})

// ---------- ANSI SGR 解析 ----------

/** 深色主题 16 色 ANSI 调色板（与 useXterm 终端主题一致） */
const PALETTE_16_DARK = [
  '#000000', '#cd313c', '#0dbc79', '#e5e510', '#2472c8', '#bc3fbc', '#11a8cd', '#d4d4d4',
  '#666666', '#f14c4c', '#23d18b', '#f5f543', '#3b8eea', '#d67df6', '#29d2cd', '#ffffff',
] as const

/** 亮色主题 16 色 ANSI 调色板（加深前景/亮色，保证在浅色 surface-variant 背景上可读） */
const PALETTE_16_LIGHT = [
  '#000000', '#c53030', '#008d5d', '#9a7a00', '#1d5fb5', '#a240a2', '#0f7e99', '#e7e7e7',
  '#555555', '#c52626', '#047a52', '#8a6d00', '#1a5cb8', '#8f3b8f', '#0d7189', '#ffffff',
] as const

/** 依据当前主题选择 ANSI 调色板 */
const logTheme = useTheme()
const isDark = computed(() => logTheme.global.current.value.dark)
const activePalette16 = computed(() => (isDark.value ? PALETTE_16_DARK : PALETTE_16_LIGHT))

function hex2(v: number): string {
  return v.toString(16).padStart(2, '0')
}

/** 256 色调色板：0-15 基本色、16-231 6x6x6 色立方、232-255 灰阶 */
function palette256(n: number): string {
  if (n < 16) return activePalette16.value[n] ?? (isDark.value ? '#d4d4d4' : '#333333')
  if (n < 232) {
    const idx = n - 16
    const levels = [0, 95, 135, 175, 215, 255]
    const r = levels[Math.floor(idx / 36)]
    const g = levels[Math.floor((idx % 36) / 6)]
    const b = levels[idx % 6]
    return `#${hex2(r)}${hex2(g)}${hex2(b)}`
  }
  const gray = 8 + (n - 232) * 10
  return `#${hex2(gray)}${hex2(gray)}${hex2(gray)}`
}

interface SgrState {
  fg: string | null
  bg: string | null
  bold: boolean
  italic: boolean
  underline: boolean
}

function ansiStyle(state: SgrState): string {
  const parts: string[] = []
  if (state.bold) parts.push('font-weight:bold')
  if (state.italic) parts.push('font-style:italic')
  if (state.underline) parts.push('text-decoration:underline')
  if (state.fg) parts.push(`color:${state.fg}`)
  if (state.bg) parts.push(`background-color:${state.bg}`)
  return parts.join(';')
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/** 解析 38;5;n / 38;2;r;g;b 扩展色，返回颜色与消耗的参数个数 */
function parseExtended(params: number[], i: number): { color: string; extra: number } | null {
  const mode = params[i + 1]
  if (mode === 5) {
    const n = params[i + 2]
    if (Number.isFinite(n)) return { color: palette256(n), extra: 2 }
  } else if (mode === 2) {
    const r = params[i + 2]
    const g = params[i + 3]
    const b = params[i + 4]
    if (Number.isFinite(r) && Number.isFinite(g) && Number.isFinite(b)) {
      return { color: `#${hex2(r)}${hex2(g)}${hex2(b)}`, extra: 4 }
    }
  }
  return null
}

/** 应用一段 SGR 序列到状态 */
function applySgr(state: SgrState, codeStr: string): void {
  const params = codeStr === '' ? [0] : codeStr.split(';').map((c) => (c === '' ? 0 : parseInt(c, 10)))
  for (let i = 0; i < params.length; i++) {
    const code = params[i]
    if (code === 0) {
      state.fg = null
      state.bg = null
      state.bold = false
      state.italic = false
      state.underline = false
    } else if (code === 1) state.bold = true
    else if (code === 3) state.italic = true
    else if (code === 4) state.underline = true
    else if (code === 22) state.bold = false
    else if (code === 23) state.italic = false
    else if (code === 24) state.underline = false
    else if (code >= 30 && code <= 37) state.fg = activePalette16.value[code - 30]
    else if (code === 39) state.fg = null
    else if (code >= 40 && code <= 47) state.bg = activePalette16.value[code - 40]
    else if (code === 49) state.bg = null
    else if (code >= 90 && code <= 97) state.fg = activePalette16.value[code - 90 + 8]
    else if (code >= 100 && code <= 107) state.bg = activePalette16.value[code - 100 + 8]
    else if (code === 38 || code === 48) {
      const ext = parseExtended(params, i)
      if (ext) {
        if (code === 38) state.fg = ext.color
        else state.bg = ext.color
        i += ext.extra
      }
    }
  }
}

/**
 * ANSI 转义序列 → 带颜色的 HTML：
 * OSC（标题等）与非 SGR CSI（光标移动等）直接剔除，SGR（m 结尾）转颜色样式
 */
function ansiToHtml(raw: string): string {
  // 先剔除 OSC 与非 SGR CSI 序列，避免残留乱码
  const clean = raw
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, (m) => (m.endsWith('m') ? m : ''))

  const state: SgrState = { fg: null, bg: null, bold: false, italic: false, underline: false }
  const csiRe = /\x1b\[([0-9;?]*)m/g
  let out = ''
  let last = 0
  let m: RegExpExecArray | null
  const pushText = (text: string): void => {
    if (!text) return
    const style = ansiStyle(state)
    out += style ? `<span style="${style}">${escapeHtml(text)}</span>` : escapeHtml(text)
  }
  while ((m = csiRe.exec(clean))) {
    pushText(clean.slice(last, m.index))
    last = m.index + m[0].length
    applySgr(state, m[1])
  }
  pushText(clean.slice(last))
  return out
}

/** 搜索高亮：仅在 HTML 的非标签文本段内替换，避免破坏 span 结构 */
function highlightHtml(html: string, kw: string): string {
  if (!kw) return html
  const escaped = escapeHtml(kw).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const re = new RegExp(escaped, 'gi')
  return html.replace(/(<[^>]+>)|([^<]+)/g, (m, tag: string | undefined, text: string | undefined) => {
    if (tag) return m
    return (text ?? '').replace(re, (mm) => `<mark class="log-viewer__mark">${mm}</mark>`)
  })
}

// ---------- 生命周期 ----------
onMounted(() => {
  void loadDates()
})

// 会话切换：清空内容并重新加载日期列表
watch(
  () => props.sessionId,
  () => {
    void loadDates()
  }
)

// 内容更新且用户位于底部附近时自动跟随滚动到底（手动上翻时不强制拉底）
watch(dateContent, () => {
  void nextTick(scrollToBottomIfNeeded)
})
</script>

<style scoped>
.log-viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
  color: rgb(var(--v-theme-on-surface));
}

.log-viewer__toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  flex: 0 0 auto;
}

.log-viewer__switch {
  flex: 0 0 auto;
}

.log-viewer__search {
  max-width: 280px;
  flex: 1 1 auto;
}

.log-viewer__body {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

/* 左侧日期列表 */
.log-viewer__dates {
  flex: 0 0 140px;
  width: 140px;
  padding: 4px;
  border-right: 1px solid rgba(var(--v-theme-on-surface), 0.15);
  overflow-y: auto;
}

.log-viewer__date {
  display: flex;
  align-items: center;
  padding: 4px 8px;
  font-size: 12px;
  border-radius: 4px;
  cursor: pointer;
  white-space: nowrap;
  user-select: none;
}

.log-viewer__date:hover {
  background: rgba(var(--v-theme-on-surface), 0.08);
}

.log-viewer__date--active {
  background: rgba(var(--v-theme-primary), 0.15);
}

/* 右侧日志内容：等宽字体，深色背景 */
.log-viewer__content {
  flex: 1 1 auto;
  min-width: 0;
  overflow: auto;
  background: rgba(var(--v-theme-surface-variant), 0.35);
}

.log-viewer__text {
  margin: 0;
  padding: 8px;
  font-family: var(--fy-font);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

/* 搜索命中高亮（深色主题友好） */
.log-viewer__text :deep(.log-viewer__mark),
.log-viewer__mark {
  background: rgba(var(--v-theme-primary), 0.45);
  color: inherit;
  border-radius: 2px;
}
</style>
