<template>
  <v-dialog
    :model-value="modelValue"
    width="640"
    persistent
    @update:model-value="onDialogToggle"
  >
    <v-card class="backup-panel">
      <!-- 标题条 -->
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-database-export</v-icon>
        MySQL 备份与还原
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ TAB_TITLES[tab] }}
        </v-chip>
        <v-btn size="x-small" variant="text" icon="mdi-close" title="关闭" @click="close" />
      </v-card-title>
      <v-divider />

      <!-- 标签页：备份 / 还原 / 运行历史 -->
      <v-tabs v-model="tab" density="comfortable" color="primary">
        <v-tab value="backup" prepend-icon="mdi-content-save-outline" class="text-none">备份</v-tab>
        <v-tab value="restore" prepend-icon="mdi-database-import" class="text-none">还原</v-tab>
        <v-tab value="history" prepend-icon="mdi-history" class="text-none">运行历史</v-tab>
      </v-tabs>
      <v-divider />

      <!-- 备份标签页 -->
      <v-window v-model="tab" class="backup-panel__window">
        <v-window-item value="backup">
          <v-card-text>
            <!-- 表多选：空 = 全库 -->
            <div class="fy-field-row">
              <span class="fy-field-row__label">备份表</span>
              <v-select
                v-model="backupTables"
                :items="tableNames"
                density="compact"
                variant="outlined"
                multiple
                chips
                closable-chips
                clearable
              >
                <template #no-data>
                  <div class="px-4 py-2 text-body-2 text-medium-emphasis">
                    未加载表列表（可直接备份全库）
                  </div>
                </template>
              </v-select>
            </div>

            <!-- 保存路径 -->
            <div class="fy-field-row">
              <span class="fy-field-row__label">保存路径</span>
              <v-text-field
                :model-value="backupPath"
                density="compact"
                single-line
                readonly
                hide-details
                prepend-inner-icon="mdi-file-outline"
                placeholder="点击右侧按钮选择保存位置"
              >
                <template #append-inner>
                  <v-btn
                    size="x-small"
                    variant="text"
                    icon="mdi-folder-open-outline"
                    title="选择保存路径"
                    @click="pickBackupPath"
                  />
                </template>
              </v-text-field>
            </div>

            <!-- 备份选项 -->
            <div class="d-flex flex-wrap mt-3">
              <v-switch
                v-model="includeData"
                label="包含数据"
                density="compact"
                hide-details
                color="primary"
              />
              <v-switch
                v-model="includeCreate"
                label="附带 DROP + CREATE"
                density="compact"
                hide-details
                color="primary"
                class="ml-6"
              />
            </div>

            <v-alert
              v-if="!backupPath.trim()"
              type="info"
              variant="tonal"
              density="compact"
              class="mt-2"
            >
              请先选择备份文件的保存位置。
            </v-alert>

            <!-- 执行结果 -->
            <div v-if="backupResult" class="backup-panel__result mt-3">
              <v-icon size="large" color="success" class="mb-1">mdi-check-circle-outline</v-icon>
              <div class="text-subtitle-1">备份成功</div>
              <div class="text-body-2 text-medium-emphasis">
                共 {{ backupResult.rows_total.toLocaleString() }} 行 · 耗时
                {{ backupResult.duration_ms.toLocaleString() }} ms
              </div>
            </div>
          </v-card-text>
        </v-window-item>

        <!-- 还原标签页 -->
        <v-window-item value="restore">
          <v-card-text>
            <div class="fy-field-row">
              <span class="fy-field-row__label">备份文件</span>
              <v-text-field
                :model-value="restorePath"
                density="compact"
                single-line
                readonly
                hide-details
                prepend-inner-icon="mdi-file-outline"
                placeholder="点击右侧按钮选择备份文件"
              >
                <template #append-inner>
                  <v-btn
                    size="x-small"
                    variant="text"
                    icon="mdi-folder-open-outline"
                    title="选择文件"
                    @click="pickRestorePath"
                  />
                </template>
              </v-text-field>
            </div>

            <v-alert type="warning" variant="tonal" density="compact" class="mt-3">
              还原将执行备份文件中的全部 SQL（含 DROP/CREATE/INSERT），目标库中的现有数据可能被覆盖。
            </v-alert>

            <!-- 执行结果 -->
            <div v-if="restoreResult" class="backup-panel__result mt-3">
              <v-icon size="large" color="success" class="mb-1">mdi-check-circle-outline</v-icon>
              <div class="text-subtitle-1">还原成功</div>
              <div class="text-body-2 text-medium-emphasis">
                共 {{ restoreResult.rows_total.toLocaleString() }} 行 · 耗时
                {{ restoreResult.duration_ms.toLocaleString() }} ms
              </div>
            </div>
          </v-card-text>
        </v-window-item>

        <!-- 运行历史标签页 -->
        <v-window-item value="history">
          <v-card-text class="pt-4">
            <div v-if="historyLoading" class="backup-panel__empty">
              <v-progress-circular indeterminate size="28" width="2" />
              <div class="text-body-2 text-medium-emphasis mt-2">正在加载运行历史…</div>
            </div>
            <EmptyState v-else-if="history.length === 0" size="compact" icon="mdi-clipboard-text-outline" title="暂无备份记录" desc="执行备份后记录将在此保留" />
            <v-list v-else density="compact" class="backup-panel__history">
              <v-list-item v-for="run in history" :key="run.id">
                <template #title>
                  <div class="text-body-2 text-truncate">{{ run.file_path }}</div>
                </template>
                <template #subtitle>
                  {{ formatTime(run.created_at) }} · {{ run.rows_total.toLocaleString() }} 行 ·
                  {{ run.duration_ms.toLocaleString() }} ms
                </template>
                <template #prepend>
                  <v-icon
                    size="small"
                    :color="run.success ? 'success' : 'error'"
                    :icon="run.success ? 'mdi-check-circle' : 'mdi-close-circle'"
                  />
                </template>
              </v-list-item>
            </v-list>
          </v-card-text>
        </v-window-item>
      </v-window>

      <v-divider />
      <v-card-actions>
        <v-btn
          variant="text"
          prepend-icon="mdi-refresh"
          :loading="historyLoading"
          :disabled="tab !== 'history'"
          @click="loadHistory"
        >
          刷新历史
        </v-btn>
        <v-spacer />
        <v-btn variant="text" @click="close">关闭</v-btn>
        <!-- 备份标签页执行按钮 -->
        <v-btn
          v-if="tab === 'backup'"
          color="primary"
          prepend-icon="mdi-database-export"
          :loading="backupExecuting"
          @click="doBackup"
        >
          执行备份
        </v-btn>
        <!-- 还原标签页执行按钮（危险，danger） -->
        <v-btn
          v-else-if="tab === 'restore'"
          color="error"
          prepend-icon="mdi-database-import"
          :loading="restoreExecuting"
          @click="doRestore"
        >
          执行还原
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { localPickDialog, localSaveDialog } from '@/api/sftp'
import {
  mysqlBackup,
  mysqlBackupRunHistory,
  mysqlRestore,
} from '@/api/mysqlBackup'
import type { MySqlBackupResult, MySqlBackupRun } from '@/api/mysqlBackup'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'
import EmptyState from '@/components/common/EmptyState.vue'

/** 标签页：backup = 备份；restore = 还原；history = 运行历史 */
type BackupTab = 'backup' | 'restore' | 'history'

/** 各标签页标题（chip 展示） */
const TAB_TITLES: Record<BackupTab, string> = {
  backup: '备份',
  restore: '还原',
  history: '运行历史',
}

const props = defineProps<{
  /** v-model：对话框显隐 */
  modelValue: boolean
  /** MySQL 连接 ID（useMysqlStore 的 connId，只读） */
  connId: string
}>()

const emit = defineEmits<{
  /** v-model：对话框显隐 */
  (e: 'update:modelValue', value: boolean): void
  /** 备份执行成功后触发，供父组件按需刷新 */
  (e: 'backup-completed', result: MySqlBackupResult): void
  /** 还原执行成功后触发，供父组件按需刷新 */
  (e: 'restore-completed', result: MySqlBackupResult): void
}>()

const ui = useUiStore()
const mysqlStore = useMysqlStore()

// ---------- 标签页状态 ----------
const tab = ref<BackupTab>('backup')

// ---------- 备份参数 ----------
/** 选中的表名列表；空数组 = 全库（调 mysqlBackup 时传 null） */
const backupTables = ref<string[]>([])
const backupPath = ref('')
const includeData = ref(true)
const includeCreate = ref(true)
const backupResult = ref<MySqlBackupResult | null>(null)
const backupExecuting = ref(false)

/** 当前连接的表名列表（来自 mysql store，只读） */
const tableNames = computed<string[]>(() => mysqlStore.tables.map((t) => t.name))

// ---------- 还原参数 ----------
const restorePath = ref('')
const restoreResult = ref<MySqlBackupResult | null>(null)
const restoreExecuting = ref(false)

// ---------- 运行历史 ----------
const history = ref<MySqlBackupRun[]>([])
const historyLoading = ref(false)

/** 加载最近备份运行历史（切到运行历史标签页 / 点击刷新时） */
async function loadHistory(): Promise<void> {
  historyLoading.value = true
  try {
    history.value = await mysqlBackupRunHistory()
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`加载运行历史失败：${msg}`, 'error')
  } finally {
    historyLoading.value = false
  }
}

// ---------- 文件路径选择（自研原生对话框） ----------

/** 备份：保存对话框获取目标路径 */
async function pickBackupPath(): Promise<void> {
  const selected = await localSaveDialog('backup.sql', {
    title: '选择备份保存路径',
    filterName: 'SQL',
    extensions: ['sql'],
  })
  if (typeof selected === 'string') backupPath.value = selected
}

/** 还原：打开对话框选择备份文件 */
async function pickRestorePath(): Promise<void> {
  const selected = await localPickDialog('send', {
    title: '选择备份文件',
    filterName: 'SQL 备份文件',
    extensions: ['sql'],
  })
  if (typeof selected === 'string') restorePath.value = selected
}

// ---------- 执行 ----------

/** 执行备份：校验路径 → 调 mysqlBackup，展示行数与耗时 */
async function doBackup(): Promise<void> {
  if (!backupPath.value.trim()) {
    ui.toast('请选择保存路径', 'error')
    return
  }
  backupResult.value = null
  backupExecuting.value = true
  try {
    // 表多选为空 = 全库（tables 传 null）
    const tables = backupTables.value.length > 0 ? backupTables.value : null
    backupResult.value = await mysqlBackup(
      props.connId,
      tables,
      backupPath.value.trim(),
      includeData.value,
      includeCreate.value,
    )
    ui.toast(`备份成功，共 ${backupResult.value.rows_total} 行`, 'success')
    emit('backup-completed', backupResult.value)
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`备份失败：${msg}`, 'error')
  } finally {
    backupExecuting.value = false
  }
}

/** 执行还原：危险操作，uiStore.confirm 二次确认后调 mysql_restore */
async function doRestore(): Promise<void> {
  if (!restorePath.value.trim()) {
    ui.toast('请选择备份文件', 'error')
    return
  }
  const ok = await ui.confirm({
    title: '还原确认',
    message: `将执行备份文件中的全部 SQL 还原到当前数据库，现有数据可能被覆盖或删除，是否继续？`,
    confirmText: '执行还原',
    danger: true,
  })
  if (!ok) return
  restoreResult.value = null
  restoreExecuting.value = true
  try {
    restoreResult.value = await mysqlRestore(props.connId, restorePath.value.trim(), true)
    ui.toast(`还原成功，共 ${restoreResult.value.rows_total} 行`, 'success')
    emit('restore-completed', restoreResult.value)
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`还原失败：${msg}`, 'error')
  } finally {
    restoreExecuting.value = false
  }
}

// ---------- 显隐与工具 ----------

/** 时间戳格式化（created_at 为 Rust 侧 RFC 3339 / ISO 字符串） */
function formatTime(value: string): string {
  const ts = Number(value)
  const date = Number.isFinite(ts) && value !== '' ? new Date(ts * 1000) : new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const pad = (n: number): string => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

/** 关闭对话框并重置状态 */
function close(): void {
  emit('update:modelValue', false)
}

/** 对话框开启时清空结果与错误；切到运行历史标签页时自动加载 */
function onDialogToggle(v: boolean): void {
  if (!v) {
    tab.value = 'backup'
    backupResult.value = null
    restoreResult.value = null
    restorePath.value = ''
    history.value = []
  }
  emit('update:modelValue', v)
}
</script>

<style scoped>
.backup-panel__window {
  min-height: 320px;
  max-height: 480px;
  overflow-y: auto;
}

.backup-panel__result {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 96px;
  gap: 4px;
}

.backup-panel__history {
  max-height: 360px;
  overflow-y: auto;
}

.backup-panel__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  gap: 4px;
}


/* x-small chip 统一 11px（工具栏按钮档） */
:deep(.v-chip--size-x-small .v-chip__content) {
  font-size: 12px !important;
}
</style>
