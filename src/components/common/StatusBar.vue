<script setup lang="ts">
/**
 * StatusBar —— 底部状态栏
 *
 * 通过 props 传入状态（亦可配合 store 数据源），同时提供默认插槽供扩展：
 * - 连接状态（connecting/connected/disconnected/hostkey-verify，色点区分）
 * - 当前路径（等宽字体）
 * - 传输状态（进行中任务数）
 *
 * 运行状态全项目统一定义：已连接/连接成功=绿色、断开（含异常断开）=红色、未连接=灰色。
 *
 * 布局约定：单行紧凑（26px），深色主题友好。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 连接状态（与后端 session-status 事件一致） */
    connectionStatus?: 'connecting' | 'connected' | 'disconnected' | 'hostkey-verify' | null
    /** 当前连接显示名 */
    connectionName?: string
    /** 当前路径（终端 cwd / SFTP 远程路径） */
    currentPath?: string
    /** 进行中的传输任务数（Queued + Running） */
    transferCount?: number
    /** 当前会话主机地址（Xshell 状态栏风格分段展示） */
    hostIp?: string
    /** 当前会话编码 */
    encoding?: string
    /** 当前打开的标签数 */
    sessionCount?: number
  }>(),
  {
    connectionStatus: null,
    connectionName: '',
    currentPath: '',
    transferCount: 0,
    hostIp: '',
    encoding: '',
    sessionCount: 0,
  },
)

/** 连接状态文案（已连接绿 / 断开红 / 未连接灰） */
const statusText = computed(() => {
  switch (props.connectionStatus) {
    case 'connected':
      return '已连接'
    case 'connecting':
      return '连接中…'
    case 'hostkey-verify':
      return '主机验证'
    case 'disconnected':
      return '断开'
    default:
      return '未连接'
  }
})

/** 连接状态色点颜色 */
const statusColor = computed(() => {
  switch (props.connectionStatus) {
    case 'connected':
      return 'success'
    case 'connecting':
    case 'hostkey-verify':
      return 'warning'
    case 'disconnected':
      return 'error'
    default:
      return 'grey'
  }
})

/** 传输状态文案 */
const transferText = computed(() => {
  if (props.transferCount > 0) {
    return `传输：${props.transferCount} 个任务`
  }
  return '传输：空闲'
})
</script>

<template>
  <div class="status-bar">
    <!-- 左侧：连接状态 -->
    <div class="status-bar__section">
      <span class="status-bar__dot" :class="{ 'status-bar__dot--on': !!statusColor }">
        <v-icon
          :icon="statusColor ? 'mdi-circle' : 'mdi-circle-outline'"
          :color="statusColor"
          size="10"
        />
      </span>
      <span class="status-bar__text">{{ statusText }}</span>
      <span v-if="connectionName" class="status-bar__text status-bar__name">
        {{ connectionName }}
      </span>
    </div>

    <!-- 主机地址（Xshell 分段） -->
    <v-divider vertical inset class="status-bar__sep" />
    <div class="status-bar__section" :title="hostIp || undefined">
      <v-icon icon="mdi-server" size="12" class="mr-1" />
      <span class="status-bar__text status-bar__mono">{{ hostIp || '—' }}</span>
    </div>

    <!-- 编码（Xshell 分段） -->
    <v-divider vertical inset class="status-bar__sep" />
    <div class="status-bar__section">
      <span class="status-bar__text status-bar__mono">{{ encoding || 'UTF-8' }}</span>
    </div>

    <!-- 中部：当前路径（等宽字体） -->
    <v-divider vertical inset class="status-bar__sep" />
    <div v-if="currentPath" class="status-bar__path" :title="currentPath">
      <v-icon icon="mdi-folder-outline" size="12" class="mr-1" />
      <span class="status-bar__path-text">{{ currentPath }}</span>
    </div>

    <v-spacer />

    <!-- 右侧：传输状态 + 标签数 + 扩展项 -->
    <div class="status-bar__section">
      <span class="status-bar__text" :class="{ 'status-bar__text--active': transferCount > 0 }">
        <v-icon icon="mdi-swap-vertical" size="12" class="mr-1" />
        {{ transferText }}
      </span>
      <v-divider vertical inset class="status-bar__sep" />
      <span class="status-bar__text" :title="`已打开 ${sessionCount} 个标签`">
        <v-icon icon="mdi-tab" size="12" class="mr-1" />
        标签：{{ sessionCount }}
      </span>
      <slot />
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 26px;
  min-height: 26px;
  padding: 0 10px;
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.75);
  background: var(--fy-chrome-bg);
  border-top: 1px solid var(--fy-chrome-border);
  user-select: none;
}

.status-bar__section {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.status-bar__dot {
  display: inline-flex;
  align-items: center;
}

.status-bar__text {
  white-space: nowrap;
}

.status-bar__text--active {
  color: rgb(var(--v-theme-primary, 46 111 219));
}

/* 分段竖分隔线（Xshell 状态栏风格） */
.status-bar__sep {
  height: 14px;
  align-self: center;
}

/* 等宽小字（IP/编码等字段）：--fy-mono 真等宽，纵向可对齐 */
.status-bar__mono {
  font-family: var(--fy-mono);
  font-size: 14px;
}

.status-bar__name {
  font-weight: 400;
  color: rgb(var(--v-theme-on-surface));
}

.status-bar__path {
  display: flex;
  align-items: center;
  min-width: 0;
  max-width: 480px;
  overflow: hidden;
}

.status-bar__path-text {
  font-family: var(--fy-font);
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
</style>
