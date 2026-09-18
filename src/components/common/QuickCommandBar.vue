<script setup lang="ts">
/**
 * QuickCommandBar —— 快速命令栏（参考 Xshell）
 *
 * - 横向渲染根级快捷命令按钮，点击将命令写入当前活动会话（写入前自动补换行）
 * - 无命令时显示引导文案；右上角可折叠（状态由 ui store 持久化视觉开关）
 * - 栏体高 28px、浅灰 chrome 背景；按钮超宽 ellipsis、溢出横向滚动
 */
import { computed, onMounted } from 'vue'
import { useQuickCommandStore } from '@/stores/quickCommand'
import { useUiStore } from '@/stores/ui'
import { useTerminalStore } from '@/stores/terminal'

const props = withDefaults(
  defineProps<{
    /** 当前活动会话 ID（由父级传入；null 表示无活动会话） */
    sessionId?: string | null
  }>(),
  { sessionId: null },
)

const qcStore = useQuickCommandStore()
const ui = useUiStore()
const terminalStore = useTerminalStore()

/** 根级快捷命令（Xshell 快速命令栏仅展示根级命令，分组命令在树内管理） */
const rootCommands = computed(() => qcStore.nodes.filter((n) => n.kind === 'command'))

onMounted(() => {
  // 快捷命令列表未加载时拉取一次（store 内部有 loading 防重）
  if (qcStore.nodes.length === 0) void qcStore.load()
})

/** 点击命令按钮：写入当前活动会话（末尾自动补 \n 直接执行） */
async function sendCommand(text: string): Promise<void> {
  const id = props.sessionId
  if (!id || !terminalStore.isConnected(id)) {
    ui.toast('请先连接一个会话再发送快捷命令', 'warning')
    return
  }
  const payload = new TextEncoder().encode(text.endsWith('\n') ? text : `${text}\n`)
  try {
    await terminalStore.writeToSession(id, payload)
  } catch (e) {
    ui.toast(`发送失败：${String(e)}`, 'error')
  }
}
</script>

<template>
  <div v-if="ui.quickBarVisible" class="quick-bar">
    <!-- 命令按钮列表：溢出横向滚动 -->
    <div class="quick-bar__commands">
      <template v-if="rootCommands.length > 0">
        <button
          v-for="cmd in rootCommands"
          :key="cmd.id"
          class="quick-bar__cmd-btn"
          type="button"
          :title="`${cmd.name}\n${cmd.command_text}`"
          @click="sendCommand(cmd.command_text)"
        >
          {{ cmd.name }}
        </button>
      </template>
      <span v-else class="quick-bar__empty">暂无快捷命令（工具 → 快捷命令）</span>
    </div>

    <!-- 折叠开关 -->
    <v-btn
      icon="mdi-chevron-down"
      size="18"
      variant="text"
      title="隐藏快速命令栏（查看菜单可重新打开）"
      class="quick-bar__toggle"
      @click="ui.quickBarVisible = false"
    />
  </div>
</template>

<style scoped>
.quick-bar {
  display: flex;
  align-items: center;
  min-height: 28px;
  padding: 0 4px;
  border-top: 1px solid var(--fy-chrome-border, #d5d9de);
  background: var(--fy-chrome-bg, #f0f2f5);
  user-select: none;
}

.quick-bar__commands {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1 1 auto;
  min-width: 0;
  overflow-x: auto;
  padding: 2px 0;
  scrollbar-width: thin;
}

.quick-bar__cmd-btn {
  flex: none;
  max-width: 160px;
  height: 26px;
  padding: 0 12px;
  font-size: 14px;
  line-height: 24px;
  color: rgb(var(--v-theme-primary, 46 111 219));
  background: rgba(var(--v-theme-primary), 0.08);
  border: 1px solid rgba(var(--v-theme-primary), 0.25);
  border-radius: 3px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.quick-bar__cmd-btn:hover {
  background: rgba(var(--v-theme-primary), 0.16);
}

.quick-bar__cmd-btn:focus-visible {
  outline: 1px solid rgb(var(--v-theme-primary));
  outline-offset: 1px;
}

.quick-bar__empty {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.55);
  padding: 0 8px;
  white-space: nowrap;
}

.quick-bar__toggle {
  flex: none;
}
</style>
