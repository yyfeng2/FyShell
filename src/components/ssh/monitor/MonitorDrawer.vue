<script setup lang="ts">
/**
 * MonitorDrawer —— 服务器监控抽屉面板
 *
 * 参考 Xshell 服务器监控抽屉（右侧滑出）：
 * - CPU / 内存 / 网络 实时指标 + 自实现 SVG sparkline 趋势图
 * - Docker 容器管理面板：容器列表（名称/镜像/状态/操作）+ 启动/停止/重启（带确认）
 * - 采样间隔设置（1/3/5/10 秒），变更后 store 自动重启采集
 *
 * 数据流：组件不直接 invoke，统一走 stores/monitor（内部走 @/api/monitor 封装层）。
 * sparkline 依赖 store 暴露的聚合值（updatedAt tick）触发重算，
 * 高频原始样本存非响应式环形缓冲。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useMonitorStore, type DockerContainer } from '@/stores/monitor'

/** sparkline 视口尺寸（ preserveAspectRatio="none" 自动拉伸） */
const SPARK_W = 100
const SPARK_H = 24

const props = withDefaults(
  defineProps<{
    /** 抽屉开关（v-model） */
    modelValue: boolean
    /** 活动会话 id（null 时显示"请先选择活动会话"） */
    sessionId?: string | null
    /** 会话显示名 */
    sessionName?: string
  }>(),
  {
    sessionId: null,
    sessionName: '',
  },
)

const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const store = useMonitorStore()

/** 当前会话最新聚合值 */
const stats = computed(() => store.statsOf(props.sessionId ?? null))

/** sparkline 的响应式 tick：样本存非响应式缓冲，聚合值（updatedAt）变化时重算 */
const tick = computed(() => stats.value?.updatedAt ?? 0)

/** CPU 趋势序列（非响应式缓冲快照，由 tick 触发重算） */
const cpuSeries = computed(() => {
  void tick.value
  return store.samplesOf(props.sessionId ?? null).map((s) => s.cpu_percent)
})

/** 内存趋势序列（百分比） */
const memSeries = computed(() => {
  void tick.value
  return store.samplesOf(props.sessionId ?? null).map((s) =>
    s.mem_total_mb > 0 ? (s.mem_used_mb / s.mem_total_mb) * 100 : 0,
  )
})

/** 网络速率序列：累计计数器差分 / 采样间隔（计数器回绕取 0） */
function rateSeries(samples: ReturnType<typeof store.samplesOf>, key: 'net_rx_kb' | 'net_tx_kb'): number[] {
  const secs = Math.max(1, store.intervalSecs)
  return samples.map((s, i) => {
    if (i === 0) return 0
    return Math.max(0, (s[key] - samples[i - 1][key]) / secs)
  })
}

const rxSeries = computed(() => {
  void tick.value
  return rateSeries(store.samplesOf(props.sessionId ?? null), 'net_rx_kb')
})

const txSeries = computed(() => {
  void tick.value
  return rateSeries(store.samplesOf(props.sessionId ?? null), 'net_tx_kb')
})

/**
 * 生成 sparkline polyline 点集。
 * @param fixedMax 固定上限（百分比指标用 100）；不传时按数据峰值自适应
 */
function sparklinePath(values: number[], width: number, height: number, fixedMax?: number): string {
  if (values.length < 2) return ''
  const max = fixedMax ?? Math.max(...values)
  const range = (max - 0) || 1
  const stepX = width / (values.length - 1)
  return values
    .map((v, i) => {
      const x = (i * stepX).toFixed(1)
      const clamped = Math.min(Math.max(0, v), max)
      const y = (height - (clamped / range) * height).toFixed(1)
      return `${x},${y}`
    })
    .join(' ')
}

const cpuPath = computed(() => sparklinePath(cpuSeries.value, SPARK_W, SPARK_H, 100))
const memPath = computed(() => sparklinePath(memSeries.value, SPARK_W, SPARK_H, 100))
const rxPath = computed(() => sparklinePath(rxSeries.value, SPARK_W, SPARK_H))
const txPath = computed(() => sparklinePath(txSeries.value, SPARK_W, SPARK_H))

/** 网络速率格式化 */
function formatRate(kbPerSec: number): string {
  if (kbPerSec >= 1024) return `${(kbPerSec / 1024).toFixed(1)} MB/s`
  return `${kbPerSec.toFixed(1)} KB/s`
}

/** 采样间隔选项（秒） */
const intervalOptions = [1, 3, 5, 10]

function onInterval(value: unknown): void {
  void store.setIntervalSecs(Number(value))
}

/** 当前会话的容器列表 */
const containerList = computed(() => (props.sessionId ? store.containers[props.sessionId] ?? [] : []))

/** Docker 状态 → chip 颜色 */
function stateColor(state: string): string {
  switch (state) {
    case 'running':
      return 'success'
    case 'paused':
      return 'warning'
    case 'exited':
      return 'grey'
    default:
      return 'default'
  }
}

// —— 操作确认 ——
type DockerAction = 'start' | 'stop' | 'restart'
const confirming = ref(false)
const confirmTarget = ref<DockerContainer | null>(null)
const confirmAction = ref<DockerAction>('start')

const actionText: Record<DockerAction, string> = {
  start: '启动',
  stop: '停止',
  restart: '重启',
}
const confirmActionText = computed(() => actionText[confirmAction.value])

function askOperate(container: DockerContainer, action: DockerAction): void {
  confirmTarget.value = container
  confirmAction.value = action
  confirming.value = true
}

/** 确认后执行：store.operateDocker 完成后自动刷新列表 */
async function doOperate(): Promise<void> {
  const target = confirmTarget.value
  if (!target || !props.sessionId) return
  await store.operateDocker(props.sessionId, target.id, confirmAction.value)
  confirming.value = false
}

/** 某容器操作是否进行中（按钮 loading） */
function isOperating(container: DockerContainer): boolean {
  return !!store.operating[container.id]
}

// —— 采集生命周期：抽屉打开期间保持采集，引用计数由 store 管理 ——
watch(
  () => (props.modelValue && props.sessionId ? props.sessionId : null),
  (sid, prev) => {
    if (prev && prev !== sid) store.stopMonitor(prev)
    if (sid) {
      void store.startMonitor(sid, store.intervalSecs)
      void store.loadDocker(sid)
    }
  },
  { immediate: true },
)

// —— 抽屉打开期间支持 ESC 关闭 ——
function onEscKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') emit('update:modelValue', false)
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) window.addEventListener('keydown', onEscKeydown)
    else window.removeEventListener('keydown', onEscKeydown)
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onEscKeydown)
  // 卸载时释放本组件的引用（最后一个卸载才 monitor_stop）
  const sid = props.modelValue && props.sessionId ? props.sessionId : null
  if (sid) store.stopMonitor(sid)
})
</script>

<template>
  <v-navigation-drawer
    :model-value="modelValue"
    location="right"
    temporary
    :width="420"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <!-- 头部 -->
    <div class="monitor-drawer__header">
      <v-icon icon="mdi-monitor-dashboard" size="18" class="mr-2" />
      <span class="monitor-drawer__title">服务器监控</span>
      <v-spacer />
      <v-btn
        icon="mdi-close"
        variant="text"
        density="comfortable"
        size="small"
        title="关闭"
        @click="emit('update:modelValue', false)"
      />
    </div>

    <!-- 未选择会话 -->
    <div v-if="!sessionId" class="monitor-drawer__empty">
      <v-icon icon="mdi-monitor-off" size="48" class="mb-4 monitor-drawer__empty-icon" />
      <p class="text-body-2 mb-0">请先选择活动会话</p>
    </div>

    <template v-else>
      <v-alert
        v-if="store.error"
        type="error"
        density="compact"
        variant="tonal"
        class="mx-4 mt-2"
        closable
        @click:close="store.error = null"
      >
        {{ store.error }}
      </v-alert>

      <!-- 实时指标 -->
      <div class="monitor-drawer__section">
        <div v-if="!stats" class="text-body-2 text-medium-emphasis">等待采样…</div>
        <template v-else>
          <!-- CPU -->
          <div class="gauge">
            <div class="gauge__head">
              <v-icon icon="mdi-cpu-64-bit" size="14" />
              <span class="gauge__label">CPU</span>
              <span class="gauge__value">{{ stats.cpuPercent.toFixed(1) }}%</span>
            </div>
            <v-progress-linear
              :model-value="stats.cpuPercent"
              height="5"
              rounded
              color="primary"
              class="gauge__bar"
            />
            <svg
              class="gauge__spark"
              :viewBox="`0 0 ${SPARK_W} ${SPARK_H}`"
              preserveAspectRatio="none"
            >
              <polyline :points="cpuPath" class="spark spark--cpu" />
            </svg>
          </div>

          <!-- 内存 -->
          <div class="gauge">
            <div class="gauge__head">
              <v-icon icon="mdi-memory" size="14" />
              <span class="gauge__label">内存</span>
              <span class="gauge__value">
                {{ stats.memUsedMb }} / {{ stats.memTotalMb }} MB（{{ stats.memPercent.toFixed(1) }}%）
              </span>
            </div>
            <v-progress-linear
              :model-value="stats.memPercent"
              height="5"
              rounded
              color="success"
              class="gauge__bar"
            />
            <svg
              class="gauge__spark"
              :viewBox="`0 0 ${SPARK_W} ${SPARK_H}`"
              preserveAspectRatio="none"
            >
              <polyline :points="memPath" class="spark spark--mem" />
            </svg>
          </div>

          <!-- 网络 -->
          <div class="gauge">
            <div class="gauge__head">
              <v-icon icon="mdi-swap-vertical" size="14" />
              <span class="gauge__label">网络</span>
              <span class="gauge__value gauge__value--rates">
                <v-icon icon="mdi-arrow-down" size="12" class="net-arrow net-arrow--rx" />
                {{ formatRate(stats.netRxRate) }}
                <v-icon icon="mdi-arrow-up" size="12" class="ml-2 net-arrow net-arrow--tx" />
                {{ formatRate(stats.netTxRate) }}
              </span>
            </div>
            <svg
              class="gauge__spark"
              :viewBox="`0 0 ${SPARK_W} ${SPARK_H}`"
              preserveAspectRatio="none"
            >
              <polyline :points="rxPath" class="spark spark--net" />
              <polyline :points="txPath" class="spark spark--net-tx" />
            </svg>
            <div class="gauge__legend">
              <span class="gauge__legend-item">
                <span class="gauge__legend-dot gauge__legend-dot--rx"></span>↓ 下行
              </span>
              <span class="gauge__legend-item">
                <span class="gauge__legend-dot gauge__legend-dot--tx"></span>↑ 上行
              </span>
            </div>
          </div>
        </template>
      </div>

      <!-- 采样间隔 -->
      <div class="monitor-drawer__section monitor-drawer__section--row">
        <span class="monitor-drawer__label">采样间隔</span>
        <v-btn-toggle
          :model-value="store.intervalSecs"
          density="compact"
          mandatory
          @update:model-value="onInterval"
        >
          <v-btn v-for="s in intervalOptions" :key="s" :value="s" class="text-body-2">
            {{ s }} 秒
          </v-btn>
        </v-btn-toggle>
      </div>

      <!-- Docker 容器管理 -->
      <div class="monitor-drawer__section">
        <div class="docker-head">
          <v-icon icon="mdi-docker" size="16" />
          <span class="monitor-drawer__label">Docker 容器</span>
          <v-spacer />
          <v-btn
            icon="mdi-refresh"
            variant="text"
            size="small"
            density="comfortable"
            title="刷新容器列表"
            :loading="store.dockerLoading"
            @click="store.loadDocker(sessionId)"
          />
        </div>

        <v-alert
          v-if="store.dockerError"
          type="error"
          density="compact"
          variant="tonal"
          class="mb-2"
          closable
          @click:close="store.dockerError = null"
        >
          {{ store.dockerError }}
        </v-alert>

        <div v-if="store.dockerLoading && containerList.length === 0" class="docker-empty">
          <v-progress-circular indeterminate size="16" width="2" class="mr-2" />
          加载中…
        </div>
        <div v-else-if="containerList.length === 0" class="docker-empty">
          未发现运行中的容器
        </div>

        <div v-else class="docker-list">
          <div v-for="c in containerList" :key="c.id" class="docker-row">
            <div class="docker-row__main">
              <div class="docker-row__name" :title="c.names">{{ c.names }}</div>
              <div class="docker-row__image" :title="c.image">{{ c.image }}</div>
              <div class="docker-row__status">
                <v-chip :color="stateColor(c.state)" size="x-small" label class="mr-2">
                  {{ c.state || '未知' }}
                </v-chip>
                <span class="docker-row__status-text">{{ c.status }}</span>
              </div>
            </div>
            <div class="docker-row__actions">
              <v-btn
                icon="mdi-play"
                size="x-small"
                variant="text"
                title="启动容器"
                :disabled="c.state === 'running' || isOperating(c)"
                @click="askOperate(c, 'start')"
              />
              <v-btn
                icon="mdi-restart"
                size="x-small"
                variant="text"
                title="重启容器"
                :disabled="c.state === 'exited' || isOperating(c)"
                @click="askOperate(c, 'restart')"
              />
              <v-btn
                icon="mdi-stop"
                size="x-small"
                variant="text"
                color="error"
                title="停止容器"
                :disabled="(c.state !== 'running' && c.state !== 'paused') || isOperating(c)"
                @click="askOperate(c, 'stop')"
              />
            </div>
          </div>
        </div>
      </div>
    </template>

    <!-- Docker 操作二次确认 -->
    <v-dialog v-model="confirming" max-width="400">
      <v-card>
        <v-card-title class="text-h6">确认操作</v-card-title>
        <v-card-text>
          确认对容器
          <code class="docker-confirm-name">{{ confirmTarget?.names }}</code>
          执行「{{ confirmActionText }}」操作吗？
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="confirming = false">取消</v-btn>
          <v-btn
            color="primary"
            variant="flat"
            :loading="!!(confirmTarget && store.operating[confirmTarget.id])"
            @click="doOperate"
          >
            确认
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-navigation-drawer>
</template>

<style scoped>
.monitor-drawer__header {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 44px;
  padding: 0 12px;
  border-bottom: 1px solid rgb(var(--v-theme-surface-variant, 32 33 35));
  user-select: none;
}

.monitor-drawer__title {
  font-size: 14px;
  font-weight: 600;
}

.monitor-drawer__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 64px 24px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
}

.monitor-drawer__empty-icon {
  color: rgb(var(--v-theme-on-surface) / 0.35);
}

.monitor-drawer__section {
  padding: 12px;
  border-bottom: 1px solid rgb(var(--v-theme-surface-variant, 32 33 35) / 0.5);
}

.monitor-drawer__section--row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.monitor-drawer__label {
  font-size: 12px;
  font-weight: 600;
  color: rgb(var(--v-theme-on-surface) / 0.75);
  white-space: nowrap;
}

/* 指标行 */
.gauge {
  margin-bottom: 16px;
}

.gauge:last-child {
  margin-bottom: 0;
}

.gauge__head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}

.gauge__label {
  font-size: 12px;
  font-weight: 600;
  color: rgb(var(--v-theme-on-surface) / 0.75);
}

.gauge__value {
  margin-left: auto;
  font-family: 'Cascadia Mono', Consolas, 'Courier New', monospace;
  font-size: 12px;
}

.gauge__value--rates {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.gauge__bar {
  margin-bottom: 6px;
}

.gauge__spark {
  display: block;
  width: 100%;
  height: 28px;
  opacity: 0.9;
}

.spark {
  fill: none;
  stroke-width: 1.5;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.spark--cpu {
  stroke: rgb(var(--v-theme-primary));
}

.spark--mem {
  stroke: rgb(var(--v-theme-success));
}

.spark--net {
  stroke: rgb(var(--v-theme-primary));
}

.spark--net-tx {
  stroke: rgb(var(--v-theme-error));
  opacity: 0.85;
}

/* 网络 ↑/↓ 图标颜色与对应曲线一致 */
.net-arrow--rx {
  color: rgb(var(--v-theme-primary));
}

.net-arrow--tx {
  color: rgb(var(--v-theme-error));
}

/* 网络曲线图例 */
.gauge__legend {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 4px;
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface) / 0.6);
  user-select: none;
}

.gauge__legend-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.gauge__legend-dot {
  width: 10px;
  height: 3px;
  border-radius: 1px;
}

.gauge__legend-dot--rx {
  background: rgb(var(--v-theme-primary));
}

.gauge__legend-dot--tx {
  background: rgb(var(--v-theme-error));
}

/* Docker 面板 */
.docker-head {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
}

.docker-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.docker-row {
  display: flex;
  align-items: flex-start;
  gap: 4px;
  padding: 8px;
  border-radius: 6px;
  background: rgb(var(--v-theme-surface-variant, 32 33 35) / 0.25);
}

.docker-row__main {
  flex: 1;
  min-width: 0;
}

.docker-row__name {
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.docker-row__image {
  font-family: 'Cascadia Mono', Consolas, 'Courier New', monospace;
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface) / 0.6);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin: 2px 0 4px;
}

.docker-row__status {
  display: flex;
  align-items: center;
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface) / 0.75);
}

.docker-row__status-text {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.docker-row__actions {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

.docker-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px 0;
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
}

.docker-confirm-name {
  word-break: break-all;
}
</style>
