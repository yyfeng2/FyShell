<script setup lang="ts">
/**
 * MonitorMiniBar —— 当前活动会话的监控迷你条
 *
 * 参考 Xshell 服务器监控条：紧凑单行（约 24px），可放置在状态栏上方或 Tab 头：
 * - CPU / 内存 百分比条 + 网络上下行速率数字
 * - 数据由 monitor store 提供（仅聚合值响应式，高频样本存非响应式缓冲）
 * - 点击右侧图表按钮发出 open 事件，由父级打开监控抽屉
 *
 * 挂载即按引用计数开始采集（store.startMonitor），卸载时最后一个引用才 monitor_stop。
 */
import { computed, onBeforeUnmount, watch } from 'vue'
import { useMonitorStore } from '@/stores/monitor'

const props = withDefaults(
  defineProps<{
    /** 活动会话 id（null 时显示"无活动会话"占位） */
    sessionId?: string | null
    /** 会话显示名（悬浮提示） */
    sessionName?: string
  }>(),
  {
    sessionId: null,
    sessionName: '',
  },
)

const emit = defineEmits<{ (e: 'open'): void }>()

const store = useMonitorStore()

/** 当前会话的最新聚合值 */
const stats = computed(() => store.statsOf(props.sessionId ?? null))

/** CPU 条颜色：随占用率分级 */
const cpuColor = computed(() => {
  const v = stats.value?.cpuPercent ?? 0
  if (v > 90) return 'error'
  if (v > 70) return 'warning'
  return 'success'
})

/** 内存条颜色：随占用率分级 */
const memColor = computed(() => {
  const v = stats.value?.memPercent ?? 0
  if (v > 90) return 'error'
  if (v > 80) return 'warning'
  return 'success'
})

/** 网络速率格式化：< 1024 KB/s 显示 KB/s，否则 MB/s */
function formatRate(kbPerSec: number): string {
  if (kbPerSec >= 1024) return `${(kbPerSec / 1024).toFixed(1)} MB/s`
  return `${kbPerSec.toFixed(1)} KB/s`
}

// 采集生命周期：切换会话/卸载时由 store 引用计数决定是否真正 monitor_stop
watch(
  () => props.sessionId ?? null,
  (sid, prev) => {
    if (prev && prev !== sid) store.stopMonitor(prev)
    if (sid) void store.startMonitor(sid, store.intervalSecs)
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  // 卸载时释放本组件的引用（最后一个卸载才 monitor_stop）
  store.stopMonitor(props.sessionId ?? null)
})
</script>

<template>
  <div class="monitor-mini" :title="sessionName || sessionId || ''">
    <template v-if="sessionId && stats">
      <!-- CPU -->
      <div class="monitor-mini__item">
        <v-icon icon="mdi-cpu-64-bit" size="12" />
        <v-progress-linear
          :model-value="stats.cpuPercent"
          height="4"
          rounded
          :color="cpuColor"
          class="monitor-mini__bar"
        />
        <span class="monitor-mini__value">{{ stats.cpuPercent.toFixed(0) }}%</span>
      </div>

      <!-- 内存 -->
      <div class="monitor-mini__item">
        <v-icon icon="mdi-memory" size="12" />
        <v-progress-linear
          :model-value="stats.memPercent"
          height="4"
          rounded
          :color="memColor"
          class="monitor-mini__bar"
        />
        <span class="monitor-mini__value">{{ stats.memPercent.toFixed(0) }}%</span>
      </div>

      <!-- 网络速率 -->
      <div class="monitor-mini__item">
        <v-icon icon="mdi-arrow-down" size="12" class="monitor-mini__net-icon" />
        <span class="monitor-mini__value monitor-mini__rate">
          {{ formatRate(stats.netRxRate) }}
        </span>
        <v-icon icon="mdi-arrow-up" size="12" class="monitor-mini__net-icon" />
        <span class="monitor-mini__value monitor-mini__rate">
          {{ formatRate(stats.netTxRate) }}
        </span>
      </div>
    </template>

    <!-- 无活动会话 -->
    <span v-else class="monitor-mini__idle">
      <v-icon icon="mdi-chart-line" size="12" class="mr-1" />
      无活动会话
    </span>

    <!-- 打开监控抽屉 -->
    <v-btn
      icon="mdi-chart-areaspline"
      size="x-small"
      variant="text"
      density="comfortable"
      title="打开监控面板"
      @click="emit('open')"
    />
  </div>
</template>

<style scoped>
.monitor-mini {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 24px;
  min-height: 24px;
  padding: 0 6px;
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface) / 0.75);
  user-select: none;
}

.monitor-mini__item {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}

.monitor-mini__bar {
  width: 56px;
  flex-shrink: 0;
}

.monitor-mini__value {
  white-space: nowrap;
  font-family: var(--fy-font);
  font-size: 12px;
  line-height: 1;
}

.monitor-mini__rate {
  min-width: 64px;
  text-align: left;
}

.monitor-mini__net-icon {
  color: rgb(var(--v-theme-on-surface) / 0.6);
}

.monitor-mini__idle {
  display: inline-flex;
  align-items: center;
  white-space: nowrap;
  opacity: 0.6;
}
</style>
