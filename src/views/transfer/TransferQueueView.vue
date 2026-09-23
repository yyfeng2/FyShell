<template>
  <v-card class="transfer-queue" flat>
    <v-card-title class="transfer-queue__title">
      <span>传输队列</span>
      <v-chip v-if="store.activeCount > 0" size="small" color="primary" class="ml-2">
        {{ store.activeCount }} 个进行中
      </v-chip>
      <v-spacer />
      <v-btn variant="text" size="small" :loading="refreshing" @click="refresh">刷新</v-btn>
      <v-btn variant="text" size="small" :disabled="!store.hasFinished" @click="clearFinished">
        清除已完成记录
      </v-btn>
    </v-card-title>
    <v-divider />

    <!-- 表头 -->
    <div v-if="store.tasks.length" class="transfer-queue__head">
      <span class="transfer-queue__cell transfer-queue__cell--name">文件名</span>
      <span class="transfer-queue__cell">方向</span>
      <span class="transfer-queue__cell">进度</span>
      <span class="transfer-queue__cell">速度</span>
      <span class="transfer-queue__cell">状态</span>
      <span class="transfer-queue__cell">错误信息</span>
      <span class="transfer-queue__cell transfer-queue__cell--action"></span>
    </div>

    <!-- 任务列表 -->
    <div v-if="!store.tasks.length" class="transfer-queue__empty">暂无传输任务</div>
    <div v-else ref="listEl" class="transfer-queue__list">
      <v-virtual-scroll
        :items="store.tasks"
        :height="listHeight"
        :item-height="ROW_HEIGHT"
      >
      <template #default="{ item }">
        <div class="transfer-queue__row">
          <span
            class="transfer-queue__cell transfer-queue__cell--name"
            :title="`${asTask(item).local_path} → ${asTask(item).remote_path}`"
          >
            <v-icon
              class="transfer-queue__icon"
              :class="`transfer-queue__icon--${asTask(item).kind}`"
              :icon="asTask(item).kind === 'Upload' ? 'mdi-upload' : 'mdi-download'"
              size="16"
            />
            {{ displayName(asTask(item)) }}
          </span>
          <span class="transfer-queue__cell">
            <v-chip size="x-small" :color="asTask(item).kind === 'Upload' ? 'primary' : 'secondary'">
              {{ asTask(item).kind === 'Upload' ? '上传' : '下载' }}
            </v-chip>
          </span>
          <span class="transfer-queue__cell">
            <div class="transfer-queue__progress">
              <v-progress-linear
                :model-value="percent(asTask(item))"
                :indeterminate="asTask(item).status === 'Running' && asTask(item).total_bytes <= 0"
                :color="statusColor(asTask(item).status)"
                height="6"
                rounded
              />
              <span class="transfer-queue__bytes">{{ bytesText(asTask(item)) }}</span>
            </div>
          </span>
          <span class="transfer-queue__cell">{{ speedText(asTask(item)) }}</span>
          <span class="transfer-queue__cell">
            <v-chip size="x-small" :color="statusColor(asTask(item).status)">{{ statusText(asTask(item).status) }}</v-chip>
          </span>
          <span class="transfer-queue__cell transfer-queue__cell--error" :title="asTask(item).error ?? ''">
            {{ asTask(item).error || '—' }}
          </span>
          <span class="transfer-queue__cell transfer-queue__cell--action">
            <v-btn
              v-if="asTask(item).status === 'Queued' || asTask(item).status === 'Running'"
              variant="text"
              size="x-small"
              color="error"
              @click="cancel(asTask(item).id)"
            >
              取消
            </v-btn>
          </span>
        </div>
      </template>
    </v-virtual-scroll>
    </div>
  </v-card>

  <v-snackbar v-model="cancelSnackbar" timeout="3000" location="bottom" color="error">
    {{ cancelError }}
  </v-snackbar>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import {
  baseName,
  formatSize,
} from '@/components/sftp/file-utils'
import { useTransferStore, type TransferStatus, type TransferTask } from '@/stores/transfer'

const ROW_HEIGHT = 56

/** VVirtualScroll 槽的 item 类型为 unknown，模板中统一断言为 TransferTask */
const asTask = (it: unknown): TransferTask => it as TransferTask

const store = useTransferStore()

/** 刷新按钮 loading 状态 */
const refreshing = ref(false)
/** 取消失败提示 */
const cancelError = ref('')
const cancelSnackbar = ref(false)

onMounted(() => {
  void store.start()
  void store.refresh()
})

// transfer-status 监听在组件卸载时 unlisten（store 内引用计数）
onUnmounted(() => store.stop())

function displayName(task: TransferTask): string {
  return task.kind === 'Upload' ? baseName(task.local_path) : baseName(task.remote_path)
}

function percent(task: TransferTask): number {
  if (task.status === 'Completed') return 100
  if (task.total_bytes <= 0) return 0
  return Math.min(100, (task.transferred_bytes / task.total_bytes) * 100)
}

function statusText(status: TransferStatus): string {
  switch (status) {
    case 'Queued':
      return '排队中'
    case 'Running':
      return '进行中'
    case 'Completed':
      return '已完成'
    case 'Failed':
      return '失败'
    case 'Cancelled':
      return '已取消'
    default:
      return status
  }
}

function statusColor(status: TransferStatus): string {
  switch (status) {
    case 'Running':
      return 'primary'
    case 'Completed':
      return 'success'
    case 'Failed':
      return 'error'
    case 'Cancelled':
      return 'warning'
    default:
      return 'grey'
  }
}

function speedText(task: TransferTask): string {
  if (task.status !== 'Running') return '—'
  const speed = store.speedOf(task.id)
  if (speed <= 0) return '—'
  return `${formatSize(Math.round(speed))}/s`
}

function bytesText(task: TransferTask): string {
  // Running 且总字节未知（总长<=0）时走 indeterminate，字节数显示“—/已传输 x B”
  if (task.status === 'Running' && task.total_bytes <= 0) {
    return task.transferred_bytes > 0 ? `已传输 ${formatSize(task.transferred_bytes)}` : '—'
  }
  return `${formatSize(task.transferred_bytes)} / ${formatSize(task.total_bytes)}`
}

async function cancel(id: string): Promise<void> {
  try {
    await store.cancel(id)
  } catch (e) {
    // 取消失败不再静默：保留提示
    cancelError.value = '取消失败' + (e instanceof Error ? `：${e.message}` : '')
    cancelSnackbar.value = true
  }
}

async function clearFinished(): Promise<void> {
  await store.clear()
}

async function refresh(): Promise<void> {
  refreshing.value = true
  try {
    await store.refresh()
  } finally {
    refreshing.value = false
  }
}

// ---------- 虚拟滚动高度（随容器自适应，消除硬编码 96 的窄屏裁剪） ----------

const listEl = ref<HTMLElement | null>(null)
const listHeight = ref(400)
let resizeObserver: ResizeObserver | null = null

function attachResizeObserver(): void {
  if (typeof ResizeObserver === 'undefined' || !listEl.value) return
  resizeObserver?.disconnect()
  // 观察列表容器自身高度，直接作为虚拟滚动高度，无需减去标题/表头
  resizeObserver = new ResizeObserver((observed) => {
    for (const entry of observed) {
      listHeight.value = Math.max(160, Math.floor(entry.contentRect.height))
    }
  })
  resizeObserver.observe(listEl.value)
}

onMounted(() => {
  attachResizeObserver()
})

// listEl 位于 tasks.length > 0 的 v-else 分支：首次挂载队列为空时不渲染、ref 为 null，
// onMounted 无法 attach；任务异步加载进来后才赋值，须在此补挂 observer，否则高度恒为 400
watch(listEl, (el) => {
  if (el) attachResizeObserver()
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
})
</script>

<style scoped>
.transfer-queue {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: rgb(var(--v-theme-surface));
}

.transfer-queue__title {
  display: flex;
  align-items: center;
  gap: 4px;
}

.transfer-queue__head,
.transfer-queue__row {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) 72px minmax(140px, 1fr) 96px 88px minmax(0, 1fr) 64px;
  gap: 8px;
  align-items: center;
  padding: 0 8px;
}

.transfer-queue__head {
  height: 32px;
  font-size: 13px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.7);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  user-select: none;
}

.transfer-queue__row {
  height: 56px;
  font-size: 13px;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.transfer-queue__cell {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.transfer-queue__cell--name {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.transfer-queue__cell--action {
  text-align: right;
}

.transfer-queue__icon {
  flex: none;
}

.transfer-queue__icon--upload {
  color: rgb(var(--v-theme-primary));
}

.transfer-queue__icon--download {
  color: rgb(var(--v-theme-secondary));
}

.transfer-queue__progress {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.transfer-queue__bytes {
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.6);
  flex: none;
}

.transfer-queue__list {
  flex: 1 1 auto;
  min-height: 0;
}

.transfer-queue__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.5);
}
</style>
