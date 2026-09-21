<template>
  <v-dialog
    :model-value="modelValue"
    width="640"
    persistent
    @update:model-value="onDialogToggle"
  >
    <v-card class="autorun-panel">
      <!-- 标题条 -->
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-robot-outline</v-icon>
        自动运行（备份任务）
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ profiles.length }} 项
        </v-chip>
        <v-btn size="x-small" variant="text" icon="mdi-close" title="关闭" @click="close" />
      </v-card-title>
      <v-divider />

      <!-- 新建/编辑任务表单 -->
      <v-card-text v-if="editing" class="pt-4">
        <div class="text-subtitle-2 mb-2">
          <v-icon size="small" class="mr-1">mdi-plus-circle-outline</v-icon>
          {{ editId === null ? '新建备份任务' : '编辑备份任务' }}
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">任务名称</span>
          <v-text-field
            v-model="editName"
            density="compact"
            single-line
            variant="outlined"
            placeholder="例如：每日全库备份"
          />
        </div>
        <!-- 表多选：空 = 全库 -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">备份表</span>
          <v-select
            v-model="editTables"
            :items="tableNames"
            density="compact"
            variant="outlined"
            multiple
            chips
            closable-chips
            clearable
            hint="不选 = 备份全库所有表"
            persistent-hint
          >
            <template #no-data>
              <div class="px-4 py-2 text-body-2 text-medium-emphasis">
                未加载表列表（可直接备份全库）
              </div>
            </template>
          </v-select>
        </div>
        <!-- 备份选项 -->
        <div class="d-flex flex-wrap mt-3">
          <v-switch
            v-model="editIncludeData"
            label="包含数据"
            density="compact"
            hide-details
            color="primary"
          />
          <v-switch
            v-model="editIncludeCreate"
            label="附带 DROP + CREATE"
            density="compact"
            hide-details
            color="primary"
            class="ml-6"
          />
        </div>
      </v-card-text>

      <!-- 任务列表 -->
      <v-card-text v-else class="pt-4">
        <div v-if="loading" class="autorun-panel__empty">
          <v-progress-circular indeterminate size="28" width="2" />
          <div class="text-body-2 text-medium-emphasis mt-2">正在加载任务列表…</div>
        </div>
        <div v-else-if="profiles.length === 0" class="autorun-panel__empty">
          <v-icon size="large" class="mb-1">mdi-robot-outline</v-icon>
          <div class="text-body-2 text-medium-emphasis">暂无备份任务，点击下方按钮新建</div>
        </div>
        <v-list v-else density="compact" class="autorun-panel__list">
          <v-list-item v-for="p in profiles" :key="String(p.id)" class="autorun-panel__item">
            <!-- 任务名 + 选项摘要 -->
            <template #default>
              <div class="text-body-2">{{ p.name }}</div>
              <div class="text-caption text-medium-emphasis">
                {{ tablesLabel(p.tables) }} ·
                {{ p.include_data ? '包含数据' : '仅结构' }}
                <template v-if="p.include_create"> · 附带 DROP + CREATE</template>
              </div>
            </template>
            <template #prepend>
              <v-icon size="small" class="mr-1">mdi-content-save-outline</v-icon>
            </template>
            <!-- 操作：立即运行 / 删除 -->
            <template #append>
              <v-btn
                size="x-small"
                variant="text"
                icon="mdi-play"
                color="primary"
                title="立即运行"
                @click="runNow(p)"
              />
              <v-btn
                size="x-small"
                variant="text"
                icon="mdi-delete-outline"
                color="error"
                title="删除任务"
                @click="removeProfile(p)"
              />
            </template>
          </v-list-item>
        </v-list>
      </v-card-text>

      <v-divider />
      <v-card-actions>
        <template v-if="editing">
          <v-btn variant="text" @click="cancelEdit">取消</v-btn>
          <v-spacer />
          <v-btn color="primary" prepend-icon="mdi-content-save" @click="saveProfile">
            保存任务
          </v-btn>
        </template>
        <template v-else>
          <v-btn variant="text" prepend-icon="mdi-refresh" :loading="loading" @click="loadProfiles">
            刷新
          </v-btn>
          <v-spacer />
          <v-btn variant="text" @click="close">关闭</v-btn>
          <v-btn color="primary" prepend-icon="mdi-plus" @click="startCreate">新建任务</v-btn>
        </template>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import {
  mysqlBackup,
  mysqlBackupProfileDelete,
  mysqlBackupProfileList,
  mysqlBackupProfileSave,
} from '@/api/mysqlBackup'
import type { MySqlBackupProfile, MySqlBackupProfileInput } from '@/api/mysqlBackup'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{
  /** v-model：对话框显隐 */
  modelValue: boolean
  /** MySQL 连接 ID（useMysqlStore 的 connId，只读） */
  connId: string
}>()

const emit = defineEmits<{
  /** v-model：对话框显隐 */
  (e: 'update:modelValue', value: boolean): void
  /** 「立即运行」备份成功后触发，供父组件按需刷新 */
  (e: 'run-completed', profile: MySqlBackupProfile, rowsTotal: number): void
}>()

const ui = useUiStore()
const mysqlStore = useMysqlStore()

// ---------- 任务列表 ----------
const profiles = ref<MySqlBackupProfile[]>([])
const loading = ref(false)

/** 加载任务配置列表 */
async function loadProfiles(): Promise<void> {
  loading.value = true
  try {
    profiles.value = await mysqlBackupProfileList()
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`加载备份任务失败：${msg}`, 'error')
  } finally {
    loading.value = false
  }
}

/** 表名列表的人类可读描述 */
function tablesLabel(tables: string[]): string {
  return tables.length > 0 ? `${tables.length} 张表` : '全库'
}

// ---------- 新建/编辑任务 ----------
const editing = ref(false)
/** 编辑中的任务 ID（null = 新建） */
const editId = ref<string | null>(null)
const editName = ref('')
const editTables = ref<string[]>([])
const editIncludeData = ref(true)
const editIncludeCreate = ref(true)

/** 当前连接的表名列表（来自 mysql store，只读） */
const tableNames = computed<string[]>(() => mysqlStore.tables.map((t) => t.name))

/** 进入新建任务模式 */
function startCreate(): void {
  editId.value = null
  editName.value = ''
  editTables.value = []
  editIncludeData.value = true
  editIncludeCreate.value = true
  editing.value = true
}

/** 取消新建/编辑，回到任务列表 */
function cancelEdit(): void {
  editing.value = false
}

/** 保存任务配置（新建/更新，调 mysql_backup_profile_save） */
async function saveProfile(): Promise<void> {
  if (!editName.value.trim()) {
    ui.toast('请输入任务名称', 'error')
    return
  }
  const profile: MySqlBackupProfileInput = {
    id: editId.value,
    name: editName.value.trim(),
    tables: editTables.value,
    include_data: editIncludeData.value,
    include_create: editIncludeCreate.value,
  }
  try {
    await mysqlBackupProfileSave(profile)
    ui.toast(`任务「${profile.name}」已保存`, 'success')
    editing.value = false
    await loadProfiles()
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`保存失败：${msg}`, 'error')
  }
}

// ---------- 立即运行 ----------

/** 按任务配置立即运行备份：dialog save 获取路径 → 调 mysqlBackup */
async function runNow(profile: MySqlBackupProfile): Promise<void> {
  const selected = await save({
    title: `选择「${profile.name}」的备份保存路径`,
    defaultPath: 'backup.sql',
    filters: [{ name: 'SQL', extensions: ['sql'] }],
  })
  if (typeof selected !== 'string') return
  try {
    // 表列表为空 = 全库（tables 传 null）
    const tables = profile.tables.length > 0 ? profile.tables : null
    const result = await mysqlBackup(
      props.connId,
      tables,
      selected,
      profile.include_data,
      profile.include_create,
    )
    ui.toast(`任务「${profile.name}」执行成功，共 ${result.rows_total} 行`, 'success')
    emit('run-completed', profile, result.rows_total)
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`任务「${profile.name}」执行失败：${msg}`, 'error')
  }
}

// ---------- 删除任务 ----------

/** 删除任务：危险操作，uiStore.confirm 二次确认 */
async function removeProfile(profile: MySqlBackupProfile): Promise<void> {
  const ok = await ui.confirm({
    title: '删除备份任务',
    message: `确定删除任务「${profile.name}」吗？该操作不可恢复。`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  if (profile.id === null) return
  try {
    await mysqlBackupProfileDelete(profile.id)
    ui.toast(`任务「${profile.name}」已删除`, 'success')
    await loadProfiles()
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err)
    ui.toast(`删除失败：${msg}`, 'error')
  }
}

// ---------- 显隐 ----------

/** 关闭对话框并复位编辑状态 */
function close(): void {
  emit('update:modelValue', false)
}

/** 对话框开启时加载任务列表；关闭时复位编辑状态 */
function onDialogToggle(v: boolean): void {
  if (!v) {
    editing.value = false
    editId.value = null
    editName.value = ''
    editTables.value = []
  }
  emit('update:modelValue', v)
}
</script>

<style scoped>
.autorun-panel__list {
  max-height: 400px;
  overflow-y: auto;
}

.autorun-panel__item {
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.autorun-panel__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  gap: 4px;
}

/* rem 换算非整数档修复：text-caption/body-2 10.5/12.25px → 12px 整数档 */
.text-caption,
.text-subtitle-2,
.text-body-2 {
  font-size: 12px !important;
}

/* x-small chip 统一 11px（工具栏按钮档） */
:deep(.v-chip--size-x-small .v-chip__content) {
  font-size: 11px !important;
}
</style>
