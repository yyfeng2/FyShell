<template>
  <v-dialog
    :model-value="modelValue"
    width="640"
    persistent
    @update:model-value="(v: boolean) => onToggle(v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-database-arrow-right-outline</v-icon>
        数据传输
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ store.connLabel }}
        </v-chip>
      </v-card-title>
      <v-divider />

      <v-card-text class="pb-2 pt-3">
        <!-- 源区：当前连接 + 源库下拉 -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">源数据库</span>
          <v-select
            :model-value="sourceDb"
            :items="sourceDbs"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            :loading="sourceLoading"
            @update:model-value="(v: string) => onSourceDbChange(v)"
          />
        </div>

        <v-divider class="my-2" />

        <!-- 目标区：连接下拉 + 库下拉 -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">目标连接</span>
          <v-select
            :model-value="targetKey"
            :items="targetItems"
            item-title="label"
            item-value="key"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            :loading="targetLoading"
            @update:model-value="(v: string) => selectTarget(v)"
          />
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">目标数据库</span>
          <v-select
            v-model="targetDb"
            :items="targetDbs"
            density="compact"
            variant="outlined"
            single-line
            hide-details
          />
        </div>
        <v-alert v-if="targetError" type="error" variant="tonal" density="compact" class="mt-2">
          {{ targetError }}
        </v-alert>

        <v-divider class="my-2" />

        <!-- 传输内容 -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">传输内容</span>
          <v-radio-group v-model="content" density="compact" hide-details class="mt-0">
            <v-radio label="结构和数据" value="both" />
            <v-radio label="仅结构" value="structure" />
            <v-radio label="仅数据" value="data" />
          </v-radio-group>
        </div>
        <v-switch
          v-model="recreate"
          label="目标表已存在时覆盖重建（DROP + CREATE）"
          density="compact"
          hide-details
          color="warning"
        />

        <v-divider class="my-2" />

        <!-- 表选择：复选列表（全选/全不选） -->
        <div class="d-flex align-center mb-1">
          <span class="text-subtitle-2">选择表（{{ selectedTables.length }}/{{ sourceTables.length }}）</span>
          <v-spacer />
          <v-btn size="x-small" variant="text" @click="selectAll">全选</v-btn>
          <v-btn size="x-small" variant="text" @click="selectedTables = []">全不选</v-btn>
          <v-btn
            size="x-small"
            variant="text"
            :loading="tablesLoading"
            title="刷新表列表"
            @click="loadSourceTables"
          >
            刷新
          </v-btn>
        </div>
        <div class="transfer__tables">
          <v-checkbox
            v-for="t in sourceTables"
            :key="t.name"
            v-model="selectedTables"
            :value="t.name"
            :label="`${t.name}（${t.rows} 行）`"
            density="compact"
            hide-details
          />
          <div v-if="!tablesLoading && sourceTables.length === 0" class="text-caption text-medium-emphasis pa-2">
            当前源库没有表
          </div>
        </div>

        <!-- 执行结果 -->
        <v-alert v-if="errorMsg" type="error" variant="tonal" density="compact" closable class="mt-2">
          {{ errorMsg }}
        </v-alert>
        <div v-if="results.length" class="transfer__results mt-2">
          <div class="text-subtitle-2 mb-1">
            <v-icon size="small" class="mr-1">mdi-check-circle-outline</v-icon>传输完成
          </div>
          <div v-for="r in results" :key="r.table" class="transfer__result-row">
            <v-icon size="x-small" class="mr-1" :color="r.skipped ? 'warning' : 'success'">
              {{ r.skipped ? 'mdi-skip-over' : 'mdi-check' }}
            </v-icon>
            {{ r.table }} — {{ r.skipped ? '已存在，跳过结构' : `传输 ${r.rows.toLocaleString()} 行` }}
          </div>
        </div>
      </v-card-text>

      <v-divider />
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="close">关闭</v-btn>
        <v-btn
          color="primary"
          prepend-icon="mdi-database-arrow-right-outline"
          :loading="executing"
          :disabled="!targetConnId || !targetDb || selectedTables.length === 0"
          @click="doTransfer"
        >
          开始传输
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { mysqlDbList } from '@/api/mysqlDb'
import { mysqlListTables } from '@/api/mysql'
import { mysqlDataTransfer } from '@/api/mysqlTools'
import type { MySqlTransferTableResult } from '@/api/mysqlTools'
import type { MySqlTableInfo } from '@/api/types'
import { useUiStore } from '@/stores/ui'
import { errText, useTargetConnection } from '@/composables/useTargetConnection'

const props = defineProps<{
  /** v-model：对话框显隐 */
  modelValue: boolean
  /** 当前 MySQL 连接 ID（useMysqlStore 的 connId） */
  connId: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', v: boolean): void
  (e: 'completed'): void
}>()

const ui = useUiStore()
const {
  store,
  targetKey,
  targetConnId,
  targetDbs,
  targetDb,
  targetLoading,
  targetError,
  targetItems,
  selectTarget,
  reset,
  dispose,
} = useTargetConnection()

// ---------- 源库 ----------
const sourceDbs = ref<string[]>([])
const sourceDb = ref('')
const sourceLoading = ref(false)
const sourceTables = ref<MySqlTableInfo[]>([])
const tablesLoading = ref(false)

/** 对话框打开：加载源库列表（默认当前库）+ 复位目标连接 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      errorMsg.value = ''
      results.value = []
      void loadSourceDbs()
      reset()
    } else {
      // 关闭：断开建立的独立目标连接
      dispose()
    }
  },
)

/** 加载源库列表（mysql_db_list），默认选中当前库 */
async function loadSourceDbs(): Promise<void> {
  if (!props.connId) return
  sourceLoading.value = true
  try {
    const result = await mysqlDbList(props.connId)
    sourceDbs.value = result.databases
    if (!sourceDb.value || !result.databases.includes(sourceDb.value)) {
      sourceDb.value = result.current_db ?? ''
    }
    await loadSourceTables()
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    sourceLoading.value = false
  }
}

/** 加载源库的表清单（mysql_list_tables 带库限定，源库可与连接当前库不同） */
async function loadSourceTables(): Promise<void> {
  if (!props.connId || !sourceDb.value) return
  tablesLoading.value = true
  try {
    sourceTables.value = await mysqlListTables(props.connId, sourceDb.value)
    // 保留仍存在的选中项，移除已消失的
    const names = new Set(sourceTables.value.map((t) => t.name))
    selectedTables.value = selectedTables.value.filter((n) => names.has(n))
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    tablesLoading.value = false
  }
}

/** 切换源库：重新加载表清单 */
function onSourceDbChange(db: string): void {
  sourceDb.value = db
  void loadSourceTables()
}

// ---------- 表选择 ----------
const selectedTables = ref<string[]>([])

function selectAll(): void {
  selectedTables.value = sourceTables.value.map((t) => t.name)
}

// ---------- 传输内容 ----------
const content = ref<'both' | 'structure' | 'data'>('both')
const recreate = ref(false)
const executing = ref(false)
const errorMsg = ref('')
const results = ref<MySqlTransferTableResult[]>([])

/** 执行数据传输 */
async function doTransfer(): Promise<void> {
  if (!props.connId || !targetConnId.value || !targetDb.value) return
  errorMsg.value = ''
  executing.value = true
  try {
    results.value = await mysqlDataTransfer(props.connId, {
      source_db: sourceDb.value,
      target_conn_id: targetConnId.value,
      target_db: targetDb.value,
      tables: selectedTables.value,
      include_structure: content.value !== 'data',
      include_data: content.value !== 'structure',
      recreate: recreate.value,
    })
    const rows = results.value.reduce((sum, r) => sum + r.rows, 0)
    ui.toast(`数据传输完成，共传输 ${rows} 行`, 'success')
    emit('completed')
  } catch (err) {
    errorMsg.value = errText(err)
    ui.toast(errorMsg.value, 'error')
  } finally {
    executing.value = false
  }
}

/** 关闭对话框 */
function close(): void {
  if (errorMsg.value) {
    errorMsg.value = ''
    results.value = []
  }
  emit('update:modelValue', false)
}

/** 对话框显隐联动（persistent 期间不响应关闭） */
function onToggle(v: boolean): void {
  emit('update:modelValue', v)
}
</script>

<style scoped>
/* 表复选列表：限高可滚动（表较多时不撑开整页） */
.transfer__tables {
  max-height: 200px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.transfer__results {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 8px;
}

.transfer__result-row {
  font-size: 12px;
  line-height: 1.8;
}

/* rem 换算非整数档修复：text-caption/subtitle-2 10.5/12.25px → 12px 整数档 */
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
