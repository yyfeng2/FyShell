<script lang="ts">
/**
 * 撰写栏输入历史（模块级）：所有终端 Tab 的 ComposeBar 实例共享（对齐 Xshell 撰写栏行为），
 * localStorage 持久化（上限 100 条，连续重复折叠），重启后可继续上下键回溯。
 */
const COMPOSE_HISTORY_KEY = 'fy-compose-history'
const COMPOSE_HISTORY_MAX = 100

const composeHistory = ref<string[]>([])
let composeHistoryLoaded = false

/** 惰性加载持久化的历史（localStorage 不可用/内容损坏时静默降级为空） */
function loadComposeHistory(): void {
  if (composeHistoryLoaded) return
  composeHistoryLoaded = true
  try {
    const raw = localStorage.getItem(COMPOSE_HISTORY_KEY)
    const parsed: unknown = raw ? JSON.parse(raw) : null
    if (Array.isArray(parsed)) {
      composeHistory.value = parsed.filter((v): v is string => typeof v === 'string')
    }
  } catch {
    // 存储不可用或内容损坏：历史仅内存保留
  }
}

/** 记录一条历史：连续重复折叠，超上限截断并落盘 */
function recordComposeHistory(text: string): void {
  loadComposeHistory()
  if (composeHistory.value[composeHistory.value.length - 1] !== text) {
    composeHistory.value.push(text)
    if (composeHistory.value.length > COMPOSE_HISTORY_MAX) {
      composeHistory.value = composeHistory.value.slice(-COMPOSE_HISTORY_MAX)
    }
  }
  try {
    localStorage.setItem(COMPOSE_HISTORY_KEY, JSON.stringify(composeHistory.value))
  } catch {
    // 存储写入失败忽略
  }
}
</script>

<script setup lang="ts">
/**
 * ComposeBar —— 撰写栏（Xshell 风格，SSH 终端底部）
 *
 * - 单行命令输入 + 发送目标下拉 + 发送按钮，回车即发送（末尾自动补 \n 直接执行）
 * - 发送目标按 FyShell 架构映射 Xshell 撰写栏：到当前会话 / 全部会话（所有已连接终端）/
 *   到可见标签（所有打开的终端标签）；
 *   Xshell 的"当前标签组/全部Xshell"在单窗口、无标签组架构下无对应概念，未提供
 */
import { computed, ref } from 'vue'
import { useTerminalStore } from '@/stores/terminal'
import { useUiStore } from '@/stores/ui'

type ComposeTarget = 'current' | 'all' | 'visible'

const props = withDefaults(
  defineProps<{
    /** 所属终端 Tab 对应会话 ID（"到当前会话"目标） */
    sessionId?: string | null
  }>(),
  { sessionId: null },
)

const terminalStore = useTerminalStore()
const ui = useUiStore()

const draft = ref('')
const sending = ref(false)
const target = ref<ComposeTarget>('current')
/** 历史回溯游标：-1 = 不在回溯中，否则指向 composeHistory 下标 */
const historyIndex = ref(-1)

/** 发送目标（Xshell 撰写栏菜单风格，带加速字母） */
const TARGETS: { value: ComposeTarget; title: string; accel: string }[] = [
  { value: 'current', title: '到当前会话', accel: 'C' },
  { value: 'all', title: '全部会话', accel: 'A' },
  { value: 'visible', title: '到可见标签', accel: 'V' },
]

const targetTitle = computed(() => TARGETS.find((t) => t.value === target.value)?.title ?? '')

/** 所有打开终端标签的会话 ID（标签 → 窗格展开） */
function openSessionIds(): string[] {
  return terminalStore.tabs.flatMap((t) => t.panes.map((p) => p.sessionId))
}

/** 目标会话列表：按选中目标解析（统一过滤未连接） */
function targetSessionIds(): string[] {
  if (target.value === 'current') {
    const id = props.sessionId
    return id && terminalStore.isConnected(id) ? [id] : []
  }
  return openSessionIds().filter((id) => terminalStore.isConnected(id))
}

/** 上键回溯历史：首次上翻到最新一条，已到最早一条则不再翻 */
function onHistoryUp(): void {
  loadComposeHistory()
  const history = composeHistory.value
  if (history.length === 0 || historyIndex.value === 0) return
  historyIndex.value = historyIndex.value < 0 ? history.length - 1 : historyIndex.value - 1
  draft.value = history[historyIndex.value] ?? ''
}

/** 下键回溯历史：越过最新一条则清空输入并退出回溯 */
function onHistoryDown(): void {
  if (historyIndex.value < 0) return
  const next = historyIndex.value + 1
  if (next >= composeHistory.value.length) {
    historyIndex.value = -1
    draft.value = ''
    return
  }
  historyIndex.value = next
  draft.value = composeHistory.value[next] ?? ''
}

/** 发送：逐会话写入（末尾自动补 \n 直接执行），成功后清空输入 */
async function send(): Promise<void> {
  const ids = targetSessionIds()
  const text = draft.value
  if (!text.trim() || ids.length === 0) return
  recordComposeHistory(text)
  historyIndex.value = -1
  sending.value = true
  try {
    const payload = new TextEncoder().encode(text.endsWith('\n') ? text : `${text}\n`)
    const results = await Promise.allSettled(ids.map((id) => terminalStore.writeToSession(id, payload)))
    const failed = ids.filter((_, i) => results[i].status === 'rejected')
    if (failed.length === 0) {
      ui.toast(ids.length === 1 ? '命令已发送' : `已发送到 ${ids.length} 个会话`, 'success')
    } else {
      ui.toast(`发送完成：成功 ${ids.length - failed.length} 个，失败 ${failed.length} 个`, 'warning')
    }
    draft.value = ''
  } finally {
    sending.value = false
  }
}
</script>

<template>
  <div v-if="ui.composeBarVisible" class="compose-bar">
    <!-- 发送目标下拉（Xshell 撰写栏菜单风格，选中项带勾选标记） -->
    <v-menu :close-on-content-click="true">
      <template #activator="{ props: act }">
        <button class="compose-bar__target" v-bind="act" :title="`发送目标：${targetTitle}`">
          {{ targetTitle }}
          <v-icon icon="mdi-chevron-down" size="12" />
        </button>
      </template>
      <v-list density="compact" class="compose-bar__target-list">
        <v-list-item v-for="t in TARGETS" :key="t.value" @click="target = t.value">
          <template #prepend>
            <v-icon v-if="target === t.value" icon="mdi-check" size="12" />
          </template>
          <v-list-item-title>{{ t.title }}({{ t.accel }})</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

    <!-- 单行命令输入：回车即发送 -->
    <input
      v-model="draft"
      class="compose-bar__input"
      type="text"
      placeholder="输入命令，回车发送…"
      spellcheck="false"
      aria-label="撰写栏命令输入"
      @keydown.enter="send"
      @keydown.up.prevent="onHistoryUp"
      @keydown.down.prevent="onHistoryDown"
    />

    <v-btn
      icon="mdi-send-outline"
      size="18"
      variant="text"
      :disabled="!draft.trim() || sending"
      title="发送"
      class="compose-bar__send"
      @click="send"
    />
  </div>
</template>

<style scoped>
.compose-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 28px;
  padding: 0 4px;
  border-top: 1px solid var(--fy-chrome-border);
  background: var(--fy-chrome-bg);
  user-select: none;
}

.compose-bar__target {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  max-width: 140px;
  height: 26px; /* 与 QuickCommandBar 按钮/全局字段基线一致（原 22px 与相邻栏失配） */
  padding: 0 8px;
  font-size: 14px;
  color: inherit;
  background: transparent;
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
  cursor: pointer;
  white-space: nowrap;
}

.compose-bar__target:hover {
  background: var(--fy-hover-bg);
}

.compose-bar__target-list {
  min-width: 140px;
}

.compose-bar__input {
  flex: 1 1 auto;
  min-width: 0;
  height: 26px; /* 与 target 按钮同高（原 22px） */
  padding: 0 8px;
  font-size: 14px;
  font-family: var(--fy-font);
  color: rgb(var(--v-theme-on-surface));
  background: rgb(var(--v-theme-surface));
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
  outline: none;
}

.compose-bar__input:focus {
  border-color: var(--fy-focus-color);
}
</style>
