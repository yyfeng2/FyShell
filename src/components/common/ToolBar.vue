<script setup lang="ts">
/**
 * ToolBar —— 经典工具栏 + 快速连接（参考 Xshell）
 *
 * 单行分组式：图标按钮按"新建 | 连接 | 传输 | 视图"分组，组间竖分隔线；
 * 快速连接输入框固定在右端（180px），回车发起快速连接（经父级打开认证对话框）。
 * 所有按钮带 title 工具提示（Xshell 风格提示体系）。
 */
import { ref } from 'vue'

const emit = defineEmits<{
  (e: 'new-session'): void
  (e: 'new-folder'): void
  (e: 'connect'): void
  (e: 'disconnect'): void
  (e: 'search'): void
  (e: 'transfer'): void
  (e: 'sftp'): void
  (e: 'help'): void
  (e: 'quick-connect', host: string): void
}>()

const hostInput = ref('')

/** 回车发起快速连接 */
function submit(): void {
  const v = hostInput.value.trim()
  if (!v) return
  emit('quick-connect', v)
  hostInput.value = ''
}
</script>

<template>
  <div class="toolbar">
    <!-- 新建组 -->
    <v-btn icon="mdi-plus" size="20" variant="text" color="primary" title="新建会话 (Ctrl+N)" @click="emit('new-session')" />
    <v-btn icon="mdi-folder-plus-outline" size="20" variant="text" color="primary" title="新建文件夹" @click="emit('new-folder')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 连接组 -->
    <v-btn icon="mdi-lan-connect" size="20" variant="text" color="success" title="连接选中的会话" @click="emit('connect')" />
    <v-btn icon="mdi-lan-disconnect" size="20" variant="text" color="error" title="断开当前会话" @click="emit('disconnect')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 传输 / 视图组 -->
    <v-btn icon="mdi-magnify" size="20" variant="text" color="info" title="搜索会话" @click="emit('search')" />
    <v-btn icon="mdi-swap-vertical" size="20" variant="text" color="warning" title="传输队列" @click="emit('transfer')" />
    <v-btn icon="mdi-folder-swap-outline" size="20" variant="text" color="accent" title="SFTP 文件传输" @click="emit('sftp')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />

    <v-spacer />

    <!-- 快速连接：固定宽度输入框（Xshell 经典位置：工具栏右端） -->
    <div class="toolbar__quickconnect">
      <v-icon icon="mdi-lock-outline" size="13" class="mr-1" title="快速连接" />
      <input
        v-model="hostInput"
        class="toolbar__quick-input"
        type="text"
        placeholder="主机IP或会话名称"
        aria-label="快速连接：输入主机IP地址或会话名称"
        @keydown.enter="submit"
      />
    </div>

    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <v-btn icon="mdi-help-circle-outline" size="20" variant="text" title="帮助" @click="emit('help')" />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  min-height: 20px;
  padding: 0 4px;
  background: var(--fy-chrome-bg, #f0f2f5);
  user-select: none;
}

.toolbar__divider {
  height: 18px;
}

/* 图标按钮：24px 按钮 + 16px 图标（Vuetify 默认 20px 图标偏粗糙），细腻清晰 */
.toolbar :deep(.v-btn .v-icon) {
  font-size: 16px;
}

.toolbar__quickconnect {
  display: flex;
  align-items: center;
  height: 22px;
  /* 随窗口宽度自适应：窄窗口收缩，宽窗口不超过 180px */
  width: clamp(120px, 18vw, 180px);
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--fy-chrome-border, #d5d9de);
  border-radius: 3px;
  background: rgb(var(--v-theme-surface));
}

.toolbar__quick-input {
  flex: 1 1 auto;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: inherit;
  font-size: 14px;
  height: 100%;
}

.toolbar__quick-input:focus-visible {
  outline: 1px solid rgb(var(--v-theme-primary));
  outline-offset: 1px;
  border-radius: 2px;
}

.toolbar__quick-input::placeholder {
  color: rgb(var(--v-theme-on-surface) / 0.4);
}
</style>
