<template>
  <v-dialog
    :model-value="modelValue"
    width="720"
    persistent
    @update:model-value="(v: boolean) => onToggle(v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-compare-horizontal</v-icon>
        结构同步
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ store.connLabel }}
        </v-chip>
      </v-card-title>
      <v-divider />

      <v-card-text class="pb-2 pt-3">
        <!-- 源库 -->
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

        <!-- 比对按钮 -->
        <div class="d-flex align-center mb-1">
          <v-btn
            size="x-small"
            variant="tonal"
            color="primary"
            prepend-icon="mdi-compare"
            :loading="comparing"
            :disabled="!sourceDb || !targetConnId || !targetDb"
            @click="compare"
          >
            比对
          </v-btn>
          <span v-if="comparing" class="text-caption text-medium-emphasis ml-2">结构比对中…</span>
          <v-spacer />
          <span v-if="diffItems.length" class="text-caption text-medium-emphasis">
            {{ diffItems.length }} 张表有差异
          </span>
        </div>

        <!-- 差异计划（same 不显示） -->
        <template v-if="plan">
          <div class="structure__plan">
            <div v-for="item in diffItems" :key="item.table" class="structure__plan-group">
              <div class="structure__plan-head">
                <v-icon
                  size="x-small"
                  class="mr-1"
                  :color="item.kind === 'create' ? 'primary' : 'warning'"
                >
                  {{ item.kind === 'create' ? 'mdi-table-plus' : 'mdi-table-edit' }}
                </v-icon>
                <span class="structure__plan-table">{{ item.table }}</span>
                <v-chip size="x-small" variant="tonal" :color="item.kind === 'create' ? 'primary' : 'warning'">
                  {{ item.kind === 'create' ? 'CREATE（目标缺失）' : 'ALTER（列差异）' }}
                </v-chip>
              </div>
              <pre class="structure__sql">{{ item.sql }}</pre>
            </div>
            <div
              v-if="diffItems.length === 0"
              class="text-caption text-medium-emphasis pa-2"
            >
              两库结构一致
            </div>
          </div>
        </template>
        <div v-else class="text-caption text-medium-emphasis">
          选择源库与目标库后点击「比对」生成差异计划
        </div>

        <!-- 执行结果 -->
        <v-alert v-if="errorMsg" type="error" variant="tonal" density="compact" closable class="mt-2">
          {{ errorMsg }}
        </v-alert>
      </v-card-text>

      <v-divider />
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="close">关闭</v-btn>
        <v-btn
          color="primary"
          prepend-icon="mdi-table-check"
          :loading="executing"
          :disabled="diffItems.length === 0"
          @click="doSync"
        >
          执行同步
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { mysqlDbList } from '@/api/mysqlDb'
import { mysqlStructureSync } from '@/api/mysqlTools'
import type { MySqlStructureSyncPlan } from '@/api/mysqlTools'
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

// ---------- 比对计划 ----------
const comparing = ref(false)
const executing = ref(false)
const errorMsg = ref('')
const plan = ref<MySqlStructureSyncPlan | null>(null)

/** 有差异的条目（same 不显示） */
const diffItems = computed(() => plan.value?.items.filter((i) => i.kind !== 'same') ?? [])

/** 对话框打开：加载源库列表（默认当前库）+ 复位目标连接 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      errorMsg.value = ''
      plan.value = null
      sourceDb.value = ''
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
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    sourceLoading.value = false
  }
}

/** 切换源库：已比对结果失效 */
function onSourceDbChange(db: string): void {
  sourceDb.value = db
  plan.value = null
}

/** 比对：execute=false 仅返回差异计划 */
async function compare(): Promise<void> {
  if (!props.connId || !targetConnId.value || !targetDb.value) return
  errorMsg.value = ''
  comparing.value = true
  try {
    plan.value = await mysqlStructureSync(props.connId, {
      source_db: sourceDb.value,
      target_conn_id: targetConnId.value,
      target_db: targetDb.value,
    }, false)
  } catch (err) {
    errorMsg.value = errText(err)
    ui.toast(errorMsg.value, 'error')
  } finally {
    comparing.value = false
  }
}

/** 执行同步（后端逐条执行计划） */
async function doSync(): Promise<void> {
  if (!props.connId || !targetConnId.value || !targetDb.value) return
  errorMsg.value = ''
  executing.value = true
  try {
    plan.value = await mysqlStructureSync(props.connId, {
      source_db: sourceDb.value,
      target_conn_id: targetConnId.value,
      target_db: targetDb.value,
    }, true)
    const count = diffItems.value.length
    ui.toast(`结构同步完成，共执行 ${count} 条变更`, 'success')
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
    plan.value = null
  }
  emit('update:modelValue', false)
}

/** 对话框显隐联动（persistent 期间不响应关闭） */
function onToggle(v: boolean): void {
  emit('update:modelValue', v)
}
</script>

<style scoped>
/* 差异计划：限高可滚动 */
.structure__plan {
  max-height: 260px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 4px;
}

.structure__plan-group + .structure__plan-group {
  margin-top: 4px;
  border-top: 1px dashed rgba(var(--v-theme-on-surface), 0.12);
}

.structure__plan-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}

.structure__plan-table {
  font-size: 12px;
  font-weight: 500;
}

/* DDL 原文（等宽小字号） */
.structure__sql {
  margin: 2px 0 4px;
  padding: 4px 8px;
  font-size: 11px;
  line-height: 1.6;
  font-family: monospace;
  white-space: pre-wrap;
  word-break: break-all;
  background: rgba(var(--v-theme-on-surface), 0.04);
  border-radius: 4px;
}


/* x-small chip 统一 11px（工具栏按钮档） */
:deep(.v-chip--size-x-small .v-chip__content) {
  font-size: 11px !important;
}
</style>
