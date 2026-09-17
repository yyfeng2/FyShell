<template>
  <div class="compose-pane">
    <!-- 左侧：常用命令树侧栏（点击命令插入草稿） -->
    <div v-if="sidebarVisible" class="compose-pane__sidebar">
      <QuickCommandTree insert-mode @insert="onInsert" />
    </div>

    <!-- 右侧：多行命令草稿 + 发送 -->
    <div class="compose-pane__main">
      <!-- 工具栏 -->
      <div class="compose-pane__header">
        <v-btn
          size="x-small"
          variant="text"
          :icon="sidebarVisible ? 'mdi-chevron-left' : 'mdi-chevron-right'"
          @click="sidebarVisible = !sidebarVisible"
        />
        <v-icon size="small" class="mr-1">mdi-script-text-outline</v-icon>
        <span class="compose-pane__title">Compose Pane</span>
        <v-spacer />
        <v-tooltip text="清空草稿" location="bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn
              v-bind="activatorProps"
              icon="mdi-eraser"
              size="x-small"
              variant="text"
              :disabled="!draft"
              @click="draft = ''"
            />
          </template>
        </v-tooltip>
      </div>

      <!-- 多行命令草稿 -->
      <div class="compose-pane__editor">
        <textarea
          ref="draftEl"
          v-model="draft"
          class="compose-pane__textarea"
          aria-label="多行命令草稿"
          placeholder="输入多行命令草稿，可先编辑再发送…（Tab 缩进，回车保留缩进）"
          spellcheck="false"
          @keydown="onKeydown"
        />
      </div>

      <!-- 发送目标选择 -->
      <div class="compose-pane__footer">
        <v-radio-group v-model="target" inline density="compact" hide-details class="compose-pane__target">
          <v-radio value="current" label="当前会话" />
          <v-radio value="all" label="所有已连接" />
          <v-radio value="pick" label="指定会话" />
        </v-radio-group>

        <v-select
          v-if="target === 'pick'"
          v-model="pickedIds"
          :items="connectedSessions"
          item-title="name"
          item-value="id"
          label="选择目标会话（可多选）"
          density="compact"
          variant="outlined"
          hide-details
          multiple
          chips
          closable-chips
          class="compose-pane__pick"
        />

        <div class="compose-pane__actions">
          <span v-if="targetHint" class="compose-pane__hint">{{ targetHint }}</span>
          <v-checkbox v-model="clearAfterSend" label="发送后清空" density="compact" hide-details />
          <v-btn
            color="primary"
            prepend-icon="mdi-send-outline"
            :disabled="!canSend"
            :loading="sending"
            @click="send"
          >
            发送
          </v-btn>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import QuickCommandTree from './QuickCommandTree.vue'
import { useSessionStore } from '@/stores/session'
import { useTerminalStore } from '@/stores/terminal'
import { useUiStore } from '@/stores/ui'
import type { QuickCommand } from '@/api/types'

const emit = defineEmits<{
  /** 发送完成：携带草稿文本与目标会话（由父级做后续联动） */
  (e: 'sent', payload: { commandText: string; sessionIds: string[] }): void
}>()

const sessionStore = useSessionStore()
const terminalStore = useTerminalStore()
const uiStore = useUiStore()

// ---------- 草稿编辑 ----------
const draft = ref('')
const draftEl = ref<HTMLTextAreaElement | null>(null)
const sidebarVisible = ref(true)
const sending = ref(false)

/** 光标处插入文本（Tab 缩进 / 回车保持缩进 / 树命令插入共用） */
function insertAtCursor(text: string): void {
  const el = draftEl.value
  if (!el) {
    draft.value += text
    return
  }
  const start = el.selectionStart
  const end = el.selectionEnd
  draft.value = draft.value.slice(0, start) + text + draft.value.slice(end)
  void nextTick(() => {
    el.selectionStart = el.selectionEnd = start + text.length
    el.focus()
  })
}

/** Tab 插入缩进；回车保留上一行行首缩进（多行草稿编辑的 Xshell 风格） */
function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Tab') {
    e.preventDefault()
    insertAtCursor('  ')
    return
  }
  if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.altKey) {
    const el = draftEl.value
    if (!el) return
    e.preventDefault()
    const before = draft.value.slice(0, el.selectionStart)
    const lineStart = before.lastIndexOf('\n') + 1
    const indent = (before.slice(lineStart).match(/^[ \t]*/) ?? [''])[0]
    insertAtCursor(`\n${indent}`)
  }
}

/** 树侧栏点击命令：插入草稿（草稿非空且末尾无换行时先补换行） */
function onInsert(command: QuickCommand): void {
  const cur = draft.value
  const text = command.command_text ?? ''
  if (!text) return
  draft.value = cur && !cur.endsWith('\n') ? `${cur}\n${text}` : cur + text
}

// ---------- 发送目标选择 ----------
type ComposeTarget = 'current' | 'all' | 'pick'

const target = ref<ComposeTarget>('current')
const pickedIds = ref<string[]>([])
const clearAfterSend = ref(false)

/** 所有已连接会话（会话列表来自 useSessionStore，连接状态来自 useTerminalStore） */
const connectedSessions = computed(() =>
  sessionStore.allConfigs.filter((cfg) => terminalStore.isConnected(cfg.id)),
)

/** 当前会话：活动标签第一个窗格对应的会话 */
const currentSessionId = computed<string | null>(
  () => terminalStore.activeTab?.panes[0]?.sessionId ?? null,
)

/** 目标提示：当前会话为空 / 指定会话未选时给出提示 */
const targetHint = computed<string>(() => {
  if (target.value === 'current') {
    const cfg = currentSessionId.value ? sessionStore.getSessionById(currentSessionId.value) : undefined
    return cfg ? `发送到：${cfg.name}` : '暂无活动会话'
  }
  if (target.value === 'all') {
    return `共 ${connectedSessions.value.length} 个已连接会话`
  }
  return pickedIds.value.length > 0 ? `已选 ${pickedIds.value.length} 个会话` : '未选择会话'
})

/** 是否可发送：草稿非空且目标会话非空 */
const canSend = computed<boolean>(() => {
  if (!draft.value.trim()) return false
  return targetSessionIds().length > 0
})

function targetSessionIds(): string[] {
  if (target.value === 'all') return connectedSessions.value.map((c) => c.id)
  if (target.value === 'current') {
    const id = currentSessionId.value
    return id && terminalStore.isConnected(id) ? [id] : []
  }
  return pickedIds.value.filter((id) => terminalStore.isConnected(id))
}

// ---------- 发送（writeToSession 按会话类型路由，逐会话发送，末尾自动补换行） ----------
async function send(): Promise<void> {
  const sessionIds = targetSessionIds()
  if (!draft.value.trim() || sessionIds.length === 0) return
  sending.value = true
  try {
    const text = draft.value.endsWith('\n') ? draft.value : `${draft.value}\n`
    const payload = new TextEncoder().encode(text)
    const results = await Promise.allSettled(
      sessionIds.map((id) => terminalStore.writeToSession(id, payload)),
    )
    const failed = sessionIds.filter((_, i) => results[i].status === 'rejected')
    if (failed.length === 0) {
      uiStore.toast(`已发送到 ${sessionIds.length} 个会话`, 'success')
    } else {
      uiStore.toast(
        `发送完成：成功 ${sessionIds.length - failed.length} 个，失败 ${failed.length} 个`,
        'warning',
      )
    }
    emit('sent', { commandText: draft.value, sessionIds })
    if (clearAfterSend.value) draft.value = ''
  } finally {
    sending.value = false
  }
}
</script>

<style scoped>
.compose-pane {
  display: flex;
  height: 100%;
  min-height: 0;
}

.compose-pane__sidebar {
  width: 240px;
  min-width: 240px;
  border-right: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  overflow: hidden;
}

.compose-pane__main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.compose-pane__header {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 12px 4px 4px;
}

.compose-pane__editor {
  flex: 1;
  min-height: 0;
  padding: 0 12px;
}

.compose-pane__textarea {
  width: 100%;
  height: 100%;
  resize: none;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.24);
  border-radius: 6px;
  background: rgba(var(--v-theme-surface), 0.4);
  color: rgb(var(--v-theme-on-surface));
  padding: 8px 10px;
  font-family: 'Consolas', 'JetBrains Mono', 'Courier New', monospace;
  font-size: 0.85rem;
  line-height: 1.5;
  outline: none;
}

.compose-pane__textarea:focus {
  border-color: rgba(var(--v-theme-primary), 0.8);
}

.compose-pane__footer {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 8px 12px;
}

.compose-pane__target {
  flex: none;
  margin: 0;
}

.compose-pane__pick {
  flex: 1;
  min-width: 180px;
  max-width: 320px;
}

.compose-pane__actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.compose-pane__hint {
  color: rgba(var(--v-theme-on-surface), 0.5);
  font-size: 0.75rem;
  white-space: nowrap;
}
</style>
