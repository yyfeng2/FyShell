<script setup lang="ts">
/**
 * ShortcutListDialog —— 快捷键速查对话框（帮助菜单"快捷键列表…"入口）
 *
 * 按作用域分两组：全局快捷键与终端内快捷键（终端内 Ctrl+C 无选中时是中断
 * 信号、Ctrl+Shift 组合始终复制粘贴——两套并存，新手最需要告知的部分）。
 * 纯展示组件，无业务逻辑。
 */
defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

interface ShortcutItem {
  keys: string
  desc: string
}

/** 全局快捷键（workspaceView registerShortcut 注册，终端聚焦外也生效） */
const GLOBAL_SHORTCUTS: ShortcutItem[] = [
  { keys: 'Ctrl+T', desc: '新建会话' },
  { keys: 'Ctrl+W', desc: '关闭当前标签' },
  { keys: 'Ctrl+Tab', desc: '切换到下一个标签' },
  { keys: 'Alt+1 ~ Alt+9', desc: '切换到第 1~9 个标签' },
]

/** 终端内快捷键（xterm 键拦截器实现，仅终端聚焦时生效；OS 标准组 Ctrl+C/X 无选中放行=中断） */
const TERMINAL_SHORTCUTS: ShortcutItem[] = [
  { keys: 'Ctrl+C', desc: '复制选中内容（无选中时发送中断信号）' },
  { keys: 'Ctrl+X', desc: '剪切选中内容' },
  { keys: 'Ctrl+V', desc: '粘贴剪贴板' },
  { keys: 'Ctrl+A', desc: '全选' },
  { keys: 'Ctrl+Shift+C', desc: '复制选中内容' },
  { keys: 'Ctrl+Shift+V', desc: '粘贴剪贴板' },
  { keys: 'Ctrl+Shift+A', desc: '全选' },
]
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    width="360"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-keyboard-outline" size="small" class="mr-2" />
        快捷键列表
        <v-spacer />
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="emit('update:modelValue', false)"
        />
      </v-card-title>
      <v-divider />
      <v-card-text>
        <div class="shortcut-list__group">全局</div>
        <div v-for="s in GLOBAL_SHORTCUTS" :key="s.keys" class="shortcut-list__row">
          <span class="shortcut-list__keys">{{ s.keys }}</span>
          <span class="shortcut-list__desc">{{ s.desc }}</span>
        </div>
        <div class="shortcut-list__group mt-3">终端内</div>
        <div v-for="s in TERMINAL_SHORTCUTS" :key="s.keys" class="shortcut-list__row">
          <span class="shortcut-list__keys">{{ s.keys }}</span>
          <span class="shortcut-list__desc">{{ s.desc }}</span>
        </div>
      </v-card-text>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.shortcut-list__group {
  font-size: 13px;
  color: rgb(var(--v-theme-on-surface) / 0.6);
  margin-bottom: 4px;
}

.shortcut-list__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 24px;
}

.shortcut-list__keys {
  font-family: var(--fy-mono);
  font-size: 13px;
  color: rgb(var(--v-theme-on-surface));
}

.shortcut-list__desc {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.75);
}
</style>
