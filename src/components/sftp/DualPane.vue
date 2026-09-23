<template>
  <div class="dual-pane">
    <!-- 本地栏 -->
    <div class="dual-pane__col">
      <div class="dual-pane__title">本地</div>
      <FilePane
        ref="localPane"
        side="local"
        :session-id="sessionId"
        :path="localPath"
        @update:path="setLocalPath"
        @transfer-request="(entries) => handleTransfer(entries, 'local')"
        @drop-files="(payload) => onDropFiles(payload, 'local')"
      />
    </div>

    <v-divider vertical />

    <!-- 远程栏（需先选中活动会话） -->
    <div class="dual-pane__col">
      <div class="dual-pane__title">
        远程<span v-if="sessionName">（{{ sessionName }}）</span>
      </div>
      <FilePane
        ref="remotePane"
        side="remote"
        :session-id="sessionId"
        :disabled="!sessionId"
        :path="remotePath"
        @update:path="setRemotePath"
        @transfer-request="(entries) => handleTransfer(entries, 'remote')"
        @drop-files="(payload) => onDropFiles(payload, 'remote')"
      />
    </div>

    <!-- 入队结果反馈：上收避免与子窗格底部 snackbar 重叠遮挡 -->
    <v-snackbar v-model="snackbar" timeout="3000" location="top">{{ notice }}</v-snackbar>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { homeDir } from '@tauri-apps/api/path'
import FilePane from './FilePane.vue'
import { joinPath, type FileEntry, type PaneSide } from './file-utils'
import { onSessionTransfersDone, useTransferStore } from '@/stores/transfer'
import { friendlyError as errText } from '@/utils/errors'

const props = withDefaults(
  defineProps<{
    /** 活动会话 ID（远程栏必填，未选中时远程窗格提示选择会话） */
    sessionId?: string
    /** 活动会话显示名（标题展示用） */
    sessionName?: string
    /** 本地路径（v-model:localPath，可选绑定） */
    localPath?: string
    /** 远程路径（v-model:remotePath，可选） */
    remotePath?: string
  }>(),
  {
    sessionId: '',
    sessionName: '',
    localPath: '',
    remotePath: '',
  },
)

const emit = defineEmits<{
  /** 终端路径联动：远程路径变化时通知父级，父级可同步到终端 */
  (e: 'remote-path-change', path: string): void
  (e: 'update:localPath', path: string): void
  (e: 'update:remotePath', path: string): void
}>()

const transfer = useTransferStore()

const notice = ref('')
const snackbar = ref(false)

function notify(message: string): void {
  notice.value = message
  snackbar.value = true
}

// ---------- 路径状态（props 未绑定时内部自管） ----------

const localPathInternal = ref('C:\\')
const remotePathInternal = ref('/')

const localPath = computed(() => props.localPath || localPathInternal.value)
const remotePath = computed(() => props.remotePath || remotePathInternal.value)

/** 挂载时尝试从系统取用户主目录作为本地默认路径；失败回退 C:\（避免不存在的盘报错） */
onMounted(async () => {
  try {
    if (!props.localPath) {
      const home = await homeDir()
      if (home) localPathInternal.value = home
    }
  } catch {
    // 取主目录失败，保持 C:\ 回退
  }
})

function setLocalPath(path: string): void {
  if (!path) return
  localPathInternal.value = path
  emit('update:localPath', path)
}

function setRemotePath(path: string): void {
  if (!path) return
  remotePathInternal.value = path
  emit('update:remotePath', path)
}

// ---------- 终端路径联动（通知） ----------

/** 远程路径变化即通知父级（父级可将命令 cwd 同步给终端） */
watch(
  () => remotePath.value,
  (path) => {
    if (path) emit('remote-path-change', path)
  },
)

/** 外部（如终端）请求远程跳转到指定路径 */
function jumpRemotePath(path: string): void {
  setRemotePath(path)
}

defineExpose({ jumpRemotePath })

// ---------- 拖拽传输与"传输到对侧" ----------

/** FilePane 的 drop-files：同侧忽略，跨侧则入队传输 */
function onDropFiles(payload: { from: PaneSide; entries: FileEntry[] }, toSide: PaneSide): void {
  if (payload.from === toSide) return
  void handleTransfer(payload.entries, payload.from)
}

/**
 * 入队传输：
 * - 本地 → 远程：逐个 sftpUpload 入队（上传）
 * - 远程 → 本地：逐个 sftpDownload 入队（下载）
 * 目标路径按两侧当前路径 + 文件名对齐（参考 Xftp 同名对齐）
 */
async function handleTransfer(entries: FileEntry[], from: PaneSide): Promise<void> {
  if (!entries.length) return
  if (!props.sessionId) {
    notify('请先选择活动会话')
    return
  }
  const to = from === 'local' ? 'remote' : 'local'
  try {
    for (const entry of entries) {
      const sourcePath =
        from === 'local'
          ? joinPath('local', localPath.value, entry.name)
          : joinPath('remote', remotePath.value, entry.name)
      const targetPath =
        to === 'remote'
          ? joinPath('remote', remotePath.value, entry.name)
          : joinPath('local', localPath.value, entry.name)
      if (from === 'local') {
        await transfer.enqueueUpload(props.sessionId, sourcePath, targetPath)
      } else {
        await transfer.enqueueDownload(props.sessionId, sourcePath, targetPath)
      }
    }
    notify(`已入队 ${entries.length} 个${to === 'remote' ? '上传' : '下载'}任务，可在传输队列查看进度`)
  } catch (e) {
    notify(errText(e))
  }
}

// ---------- 传输完成后对侧列表自动刷新 ----------

const localPane = ref<InstanceType<typeof FilePane> | null>(null)
const remotePane = ref<InstanceType<typeof FilePane> | null>(null)

/** 待刷新方向集合：同一批多个任务完成合并为一次刷新 */
const pendingKinds = new Set<string>()
let refreshTimer: ReturnType<typeof setTimeout> | null = null
let unlistenDone: (() => void) | null = null

onMounted(() => {
  unlistenDone = onSessionTransfersDone((task) => {
    if (!props.sessionId || task.session_id !== props.sessionId) return
    pendingKinds.add(task.kind)
    // 防抖：批量任务陆续完成时合并为一次刷新
    if (refreshTimer) clearTimeout(refreshTimer)
    refreshTimer = setTimeout(() => {
      refreshTimer = null
      if (pendingKinds.has('Upload')) remotePane.value?.refresh()
      if (pendingKinds.has('Download')) localPane.value?.refresh()
      pendingKinds.clear()
    }, 500)
  })
})

onUnmounted(() => {
  if (refreshTimer) clearTimeout(refreshTimer)
  unlistenDone?.()
})
</script>

<style scoped>
.dual-pane {
  display: flex;
  height: 100%;
  min-height: 0;
  background: rgb(var(--v-theme-background));
}

.dual-pane__col {
  display: flex;
  flex-direction: column;
  flex: 1 1 50%;
  min-width: 0;
  min-height: 0;
}

.dual-pane__title {
  padding: 6px 8px;
  font-size: 12px;
  font-weight: 400;
  color: rgba(var(--v-theme-on-surface), 0.85);
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  background: rgb(var(--v-theme-surface));
  user-select: none;
  /* 长会话名不撑破列宽 */
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
</style>
