<script setup lang="ts">
/**
 * ToolBar —— 经典工具栏 + 地址栏 + 快速连接（参考 Xshell）
 *
 * 单行分组式：图标按钮按"新建 | 连接 | 传输"分组，组间竖分隔线；
 * 地址栏显示当前活动会话地址，下拉列出全部会话可快速跳转（Xshell 地址栏惯例）；
 * 快速连接输入框固定在右端（180px），回车发起快速连接（经父级打开认证对话框）。
 * 所有按钮带 title 工具提示（Xshell 风格提示体系）。
 */
import { ref } from 'vue'

/** 地址栏会话条目（id 为树节点 id，父级经节点连接路由跳转） */
interface AddressBarSession {
  id: string
  label: string
}

defineProps<{
  /** 当前活动标签的地址文本（会话显示 user@host:port，其他显示标签标题） */
  address: string
  /** 可快速跳转的会话列表（会话树叶子，不含文件夹/分区） */
  sessions: AddressBarSession[]
}>()

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
  (e: 'goto-session', id: string): void
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
    <v-btn icon="mdi-plus" size="20" variant="text" title="新建会话 (Ctrl+T)" @click="emit('new-session')" />
    <v-btn icon="mdi-folder-plus-outline" size="20" variant="text" title="新建文件夹" @click="emit('new-folder')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 连接组：连接/断开保留语义色（全工具栏唯一的彩色点缀） -->
    <v-btn icon="mdi-lan-connect" size="20" variant="text" color="success" title="连接选中的会话" @click="emit('connect')" />
    <v-btn icon="mdi-lan-disconnect" size="20" variant="text" color="error" title="断开当前会话" @click="emit('disconnect')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 传输 / 视图组 -->
    <v-btn icon="mdi-magnify" size="20" variant="text" title="搜索会话" @click="emit('search')" />
    <v-btn icon="mdi-swap-vertical" size="20" variant="text" title="传输队列" @click="emit('transfer')" />
    <v-btn icon="mdi-folder-swap-outline" size="20" variant="text" title="SFTP 文件传输" @click="emit('sftp')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />

    <v-spacer />

    <!-- 地址栏：显示当前活动会话地址，下拉可快速跳转（Xshell 地址栏惯例） -->
    <v-menu :close-on-content-click="true">
      <template #activator="{ props: act }">
        <button class="toolbar__address" v-bind="act" title="当前会话地址，下拉切换会话">
          <v-icon icon="mdi-map-marker" size="13" class="toolbar__address-icon" />
          <span class="toolbar__address-text">{{ address || '未连接' }}</span>
          <v-icon icon="mdi-chevron-down" size="13" class="toolbar__address-icon" />
        </button>
      </template>
      <v-list density="compact" class="toolbar__address-list" max-height="320">
        <v-list-item
          v-for="s in sessions"
          :key="s.id"
          :value="s.id"
          @click="emit('goto-session', s.id)"
        >
          <v-list-item-title>{{ s.label }}</v-list-item-title>
        </v-list-item>
        <v-list-item v-if="!sessions.length" disabled>
          <v-list-item-title>暂无会话</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

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
  min-height: 26px; /* 与 MenuBar/StatusBar 26px 节奏一致（原 20px 比相邻条带矮 6px） */
  padding: 0 4px;
  background: var(--fy-chrome-bg);
  user-select: none;
}

.toolbar__divider {
  height: 18px;
}

/* 覆盖 Vuetify 竖向 inset divider 默认 margin-block 8px：工具栏行高收紧到内容高 */
.toolbar :deep(.v-divider--inset) {
  margin-block: 0;
}

/* 图标按钮：20px 按钮 + 16px 图标（Vuetify 默认 20px 图标偏粗糙），细腻清晰 */
.toolbar :deep(.v-btn .v-icon) {
  font-size: 16px;
}

/* 地址栏：与快速连接输入框同基线（26px、同边框圆角），Xshell 地址栏惯例 */
.toolbar__address {
  display: flex;
  align-items: center;
  height: 26px;
  /* 随窗口宽度自适应：窄窗口收缩，宽窗口不超过 240px */
  width: clamp(140px, 22vw, 240px);
  flex: none;
  padding: 0 8px;
  margin-right: 6px;
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
  background: rgb(var(--v-theme-surface));
  color: inherit;
  cursor: pointer;
  font-size: 13px;
}

.toolbar__address:hover {
  background: rgb(var(--v-theme-on-surface) / 0.04);
}

.toolbar__address-icon {
  flex: none;
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

.toolbar__address-text {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
  padding: 0 4px;
}

/* 地址栏下拉：行距与 MenuBar 菜单一致（紧凑档） */
.toolbar :deep(.v-list-item) {
  min-height: 0;
  padding-top: 2px;
  padding-bottom: 2px;
}

.toolbar :deep(.v-list-item-title) {
  font-size: 13px;
  line-height: 1.3;
}

.toolbar__quickconnect {
  display: flex;
  align-items: center;
  height: 26px; /* 与全局单行控件基线一致（字段/搜索框 26px，原 22px） */
  /* 随窗口宽度自适应：窄窗口收缩，宽窗口不超过 180px */
  width: clamp(120px, 18vw, 180px);
  flex: none;
  padding: 0 6px;
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
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
  border-radius: 4px;
}

.toolbar__quick-input::placeholder {
  color: rgb(var(--v-theme-on-surface) / 0.4);
}
</style>
