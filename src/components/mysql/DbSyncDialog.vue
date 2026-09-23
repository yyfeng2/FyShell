<template>
  <v-dialog
    :model-value="modelValue"
    width="640"
    persistent
    @update:model-value="(v: boolean) => onToggle(v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-database-refresh-outline</v-icon>
        数据同步
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ store.connLabel }}
        </v-chip>
      </v-card-title>
      <v-divider />

      <v-card-text class="data-sync__body pb-2 pt-3">
        <!-- 源区：当前连接 + 源表（连接当前库） -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">源表</span>
          <v-select
            :model-value="sourceTable"
            :items="sourceTableItems"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            :loading="sourceLoading"
            @update:model-value="(v: string) => onSourceTableChange(v)"
          />
        </div>

        <v-divider class="my-2" />

        <!-- 目标区：连接/库/表三下拉 -->
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
            @update:model-value="() => onTargetDbChange()"
          />
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">目标表</span>
          <v-select
            v-model="targetTable"
            :items="targetTableItems"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            :loading="targetTablesLoading"
          />
        </div>
        <v-divider class="my-2" />

        <!-- 比对结果 -->
        <div class="d-flex align-center mb-1">
          <v-btn
            size="x-small"
            variant="tonal"
            color="primary"
            prepend-icon="mdi-compare"
            :loading="comparing"
            :disabled="!sourceTable || !targetConnId || !targetDb || !targetTable"
            @click="compare"
          >
            比对
          </v-btn>
          <span v-if="comparing" class="text-caption text-medium-emphasis ml-2">按主键比对中…</span>
        </div>

        <template v-if="outcome">
          <div class="d-flex sync__counts">
            <div class="sync__count sync__count--source">
              <div class="sync__count-num">{{ outcome.only_source }}</div>
              <div class="sync__count-label">仅源有</div>
            </div>
            <div class="sync__count sync__count--target">
              <div class="sync__count-num">{{ outcome.only_target }}</div>
              <div class="sync__count-label">仅目标有</div>
            </div>
            <div class="sync__count sync__count--changed">
              <div class="sync__count-num">{{ outcome.changed }}</div>
              <div class="sync__count-label">不一致</div>
            </div>
          </div>
          <div class="text-caption text-medium-emphasis mb-1">
            主键：{{ outcome.key_columns.join(', ') }}（差异明细各限 20 条）
          </div>
          <div class="sync__detail">
            <div v-if="outcome.sample_source.length" class="sync__detail-group">
              <span class="text-subtitle-2">仅源有的主键：</span>
              <div v-for="k in outcome.sample_source" :key="`s-${k.join('|')}`" class="sync__detail-row">
                {{ k.join(' | ') }}
              </div>
            </div>
            <div v-if="outcome.sample_target.length" class="sync__detail-group">
              <span class="text-subtitle-2">仅目标有的主键：</span>
              <div v-for="k in outcome.sample_target" :key="`t-${k.join('|')}`" class="sync__detail-row">
                {{ k.join(' | ') }}
              </div>
            </div>
            <div v-if="outcome.sample_changed.length" class="sync__detail-group">
              <span class="text-subtitle-2">不一致的主键：</span>
              <div v-for="k in outcome.sample_changed" :key="`c-${k.join('|')}`" class="sync__detail-row">
                {{ k.join(' | ') }}
              </div>
            </div>
            <div
              v-if="!outcome.sample_source.length && !outcome.sample_target.length && !outcome.sample_changed.length"
              class="text-caption text-medium-emphasis pa-2"
            >
              两表数据一致
            </div>
          </div>

          <!-- 同步方向开关 -->
          <div class="mt-2">
            <v-switch
              v-model="insertMissing"
              label="插入缺失行（仅源有的行插入目标）"
              density="compact"
              hide-details
            />
            <v-switch
              v-model="deleteExtra"
              label="删除多余行（仅目标有的行从目标删除）"
              density="compact"
              hide-details
              color="warning"
            />
            <v-switch
              v-model="updateDiff"
              label="更新不一致行（以源端为准覆盖）"
              density="compact"
              hide-details
            />
          </div>
        </template>
        <div v-else class="text-caption text-medium-emphasis">
          选择源表与目标表后点击「比对」查看差异
        </div>

      </v-card-text>

      <v-divider />
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="close">关闭</v-btn>
        <v-btn
          color="primary"
          prepend-icon="mdi-database-refresh-outline"
          :loading="executing"
          :disabled="!outcome || (!insertMissing && !deleteExtra && !updateDiff)"
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
import { mysqlListTables } from '@/api/mysql'
import { mysqlDataSync } from '@/api/mysqlTools'
import type { MySqlDataSyncOutcome } from '@/api/mysqlTools'
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
  targetItems,
  selectTarget,
  reset,
  dispose,
} = useTargetConnection()

// ---------- 源表（连接当前库） ----------
const sourceTables = ref<MySqlTableInfo[]>([])
const sourceTable = ref('')
const sourceLoading = ref(false)
/** 源端当前库名（后端同步 options.source_db 必填） */
const sourceDb = ref('')
/** 源表下拉候选（表名字符串列表） */
const sourceTableItems = computed(() => sourceTables.value.map((t) => t.name))

// ---------- 目标表 ----------
const targetTableItems = ref<string[]>([])
const targetTable = ref('')
const targetTablesLoading = ref(false)

// ---------- 同步选项 ----------
const insertMissing = ref(true)
const deleteExtra = ref(false)
const updateDiff = ref(true)
const comparing = ref(false)
const executing = ref(false)
const outcome = ref<MySqlDataSyncOutcome | null>(null)

/** 对话框打开：加载源表 + 复位目标连接 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      outcome.value = null
      sourceTable.value = ''
      targetTable.value = ''
      insertMissing.value = true
      deleteExtra.value = false
      updateDiff.value = true
      void loadSourceTables()
      reset()
    } else {
      // 关闭：断开建立的独立目标连接
      dispose()
    }
  },
)

/** 目标连接建立后（targetConnId 变化）自动加载目标表清单 */
watch(
  () => targetConnId.value,
  (id) => {
    targetTable.value = ''
    targetTableItems.value = []
    outcome.value = null
    if (id && targetDb.value) {
      void loadTargetTables()
    }
  },
)

/** 加载源端表清单（连接当前库）与当前库名 */
async function loadSourceTables(): Promise<void> {
  if (!props.connId) return
  sourceLoading.value = true
  try {
    sourceTables.value = await mysqlListTables(props.connId)
    try {
      const result = await mysqlDbList(props.connId)
      sourceDb.value = result.current_db ?? ''
    } catch {
      // 库名获取失败不阻断表清单展示（比对时会再报具体错误）
    }
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    sourceLoading.value = false
  }
}

/** 切换源表：已比对结果失效 */
function onSourceTableChange(v: string): void {
  sourceTable.value = v
  outcome.value = null
}

/** 切换目标库：重载目标表清单 */
async function onTargetDbChange(): Promise<void> {
  targetTable.value = ''
  targetTableItems.value = []
  outcome.value = null
  await loadTargetTables()
}

/** 加载目标库的表清单（mysql_list_tables 带库限定） */
async function loadTargetTables(): Promise<void> {
  if (!targetConnId.value || !targetDb.value) return
  targetTablesLoading.value = true
  try {
    targetTableItems.value = (await mysqlListTables(targetConnId.value, targetDb.value)).map((t) => t.name)
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    targetTablesLoading.value = false
  }
}

/** 比对：execute=false 仅返回差异与样例 */
async function compare(): Promise<void> {
  if (!props.connId || !targetConnId.value || !targetDb.value || !targetTable.value) return
  comparing.value = true
  try {
    outcome.value = await mysqlDataSync(props.connId, {
      source_db: sourceDb.value,
      source_table: sourceTable.value,
      target_conn_id: targetConnId.value,
      target_db: targetDb.value,
      target_table: targetTable.value,
      insert_missing: false,
      delete_extra: false,
      update_diff: false,
    }, false)
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    comparing.value = false
  }
}

/** 执行同步（后端执行时重新比对后应用） */
async function doSync(): Promise<void> {
  if (!props.connId || !targetConnId.value || !targetDb.value || !targetTable.value) return
  executing.value = true
  try {
    outcome.value = await mysqlDataSync(props.connId, {
      source_db: sourceDb.value,
      source_table: sourceTable.value,
      target_conn_id: targetConnId.value,
      target_db: targetDb.value,
      target_table: targetTable.value,
      insert_missing: insertMissing.value,
      delete_extra: deleteExtra.value,
      update_diff: updateDiff.value,
    }, true)
    ui.toast(
      `数据同步完成：插入 ${outcome.value.only_source} 行、删除 ${outcome.value.only_target} 行、更新 ${outcome.value.changed} 行`,
      'success',
    )
    emit('completed')
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    executing.value = false
  }
}

/** 关闭对话框 */
function close(): void {
  outcome.value = null
  emit('update:modelValue', false)
}

/** 对话框显隐联动（persistent 期间不响应关闭） */
function onToggle(v: boolean): void {
  emit('update:modelValue', v)
}
</script>

<style scoped>
/* 内容整体限高：多区块累计高度可达 750px+，避免顶到视口上限 */
.data-sync__body {
  max-height: 70vh;
  overflow-y: auto;
}

/* 三组计数卡片 */
.sync__counts {
  gap: 8px;
  margin-bottom: 8px;
}

.sync__count {
  flex: 1;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 6px 8px;
  text-align: center;
}

.sync__count--source {
  border-left: 3px solid rgb(var(--v-theme-primary));
}

.sync__count--target {
  border-left: 3px solid rgb(var(--v-theme-warning));
}

.sync__count--changed {
  border-left: 3px solid rgb(var(--v-theme-error));
}

.sync__count-num {
  font-size: 12px;
  font-weight: 500;
}

.sync__count-label {
  font-size: 11px;
  color: rgba(var(--v-theme-on-surface), 0.6);
}

/* 差异明细 */
.sync__detail {
  max-height: 160px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 4px 8px;
}

.sync__detail-group + .sync__detail-group {
  margin-top: 4px;
}

.sync__detail-row {
  font-size: 12px;
  line-height: 1.8;
  font-family: monospace;
}


/* x-small chip 统一 11px（工具栏按钮档） */
:deep(.v-chip--size-x-small .v-chip__content) {
  font-size: 11px !important;
}
</style>
