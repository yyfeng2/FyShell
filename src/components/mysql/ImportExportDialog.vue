<template>
  <v-dialog
    :model-value="modelValue"
    width="560"
    persistent
    @update:model-value="onDialogToggle"
  >
    <v-card class="io-wizard">
      <!-- 标题条 -->
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-swap-vertical</v-icon>
        MySQL 导入导出
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ mode === 'export' ? '导出' : '导入' }}
        </v-chip>
      </v-card-title>
      <v-divider />
      <!-- 步骤指示器（自定义简易步骤条） -->
      <v-card-text class="pb-2 pt-3">
        <div class="io-wizard__steps">
          <span class="io-wizard__step io-wizard__step--active">1. 参数设置</span>
          <v-divider class="io-wizard__steps-divider" />
          <span class="io-wizard__step" :class="{ 'io-wizard__step--active': step > 1 }">
            2. 确认执行
          </span>
        </div>
      </v-card-text>

      <!-- 第一步：模式与参数 -->
      <v-card-text v-if="step === 1">
        <!-- 模式选择：导出 / 导入 -->
        <v-btn-toggle
          :model-value="mode"
          mandatory
          density="comfortable"
          class="mb-4"
          @update:model-value="(v: unknown) => (mode = v as IoMode)"
        >
          <v-btn value="export" prepend-icon="mdi-file-export-outline">导出</v-btn>
          <v-btn value="import" prepend-icon="mdi-file-import-outline">导入</v-btn>
        </v-btn-toggle>

        <!-- 导出参数 -->
        <template v-if="mode === 'export'">
          <v-textarea
            v-model="exportSql"
            label="查询 SQL"
            density="compact"
            rows="3"
            auto-grow
            variant="outlined"
            hint="仅支持 SELECT 语句"
          />
          <v-radio-group v-model="exportFormat" label="导出格式" density="compact" hide-details class="mt-2">
            <v-radio label="CSV（逗号分隔）" value="csv" />
            <v-radio label="JSON" value="json" />
            <v-radio label="SQL（INSERT 语句）" value="sql" />
          </v-radio-group>
          <v-switch
            v-if="exportFormat === 'sql'"
            v-model="includeCreateTable"
            label="附带建表语句（DROP + CREATE TABLE）"
            density="compact"
            hide-details
            class="mt-1"
          />
          <v-text-field
            :model-value="filePath"
            label="保存路径"
            density="compact"
            single-line
            readonly
            hide-details
            prepend-inner-icon="mdi-file-outline"
            placeholder="点击右侧按钮选择保存位置"
            class="mt-2"
          >
            <template #append-inner>
              <v-btn
                size="x-small"
                variant="text"
                icon="mdi-folder-open-outline"
                title="选择保存路径"
                @click="pickSavePath"
              />
            </template>
          </v-text-field>
        </template>

        <!-- 导入参数 -->
        <template v-else>
          <v-text-field
            :model-value="filePath"
            label="源文件路径"
            density="compact"
            single-line
            readonly
            hide-details
            prepend-inner-icon="mdi-file-outline"
            placeholder="点击右侧按钮选择导入文件"
            class="mb-2"
          >
            <template #append-inner>
              <v-btn
                size="x-small"
                variant="text"
                icon="mdi-folder-open-outline"
                title="选择文件"
                @click="pickOpenPath"
              />
            </template>
          </v-text-field>
          <v-text-field
            v-model="importTable"
            label="目标表名"
            density="compact"
            single-line
            :hint="importFormat === 'csv' ? 'CSV 格式必填' : 'SQL 格式的语句自带表名，此字段仅作展示'"
            :persistent-hint="importFormat === 'csv'"
          />
          <v-radio-group v-model="importFormat" label="文件格式" density="compact" class="mt-2">
            <v-radio label="CSV（逗号分隔）" value="csv" />
            <v-radio label="SQL（INSERT 语句）" value="sql" />
          </v-radio-group>
          <v-text-field
            v-model.number="batchSize"
            label="批量大小（每批行数，1-1000）"
            type="number"
            density="compact"
            single-line
            hide-details
            class="mb-2"
          />
          <v-switch
            v-model="replace"
            label="REPLACE INTO 覆盖已有主键"
            density="compact"
            hide-details
            color="warning"
          />
          <v-alert v-if="replace" type="warning" variant="tonal" density="compact" class="mt-1">
            开启后导入将以 REPLACE INTO 覆盖表中同主键数据，执行前需二次确认。
          </v-alert>
        </template>
      </v-card-text>

      <!-- 第二步：确认执行 / 结果展示 -->
      <v-card-text v-else>
        <template v-if="!result">
          <div class="io-wizard__summary">
            <div class="text-subtitle-2 mb-2">
              <v-icon size="small" class="mr-1">mdi-clipboard-check-outline</v-icon>
              请确认以下{{ mode === 'export' ? '导出' : '导入' }}配置
            </div>
            <div class="io-wizard__summary-item"><span class="io-wizard__summary-key">模式</span>{{ mode === 'export' ? '导出' : '导入' }}</div>
            <div class="io-wizard__summary-item"><span class="io-wizard__summary-key">文件</span>{{ filePath }}</div>
            <div v-if="mode === 'export'" class="io-wizard__summary-item">
              <span class="io-wizard__summary-key">SQL</span>{{ exportFormat.toUpperCase() }}
              <template v-if="exportFormat === 'sql' && includeCreateTable"> · 附带建表语句</template>
            </div>
            <div v-else class="io-wizard__summary-item">
              <span class="io-wizard__summary-key">目标表</span>{{ importTable || '-' }} · {{ importFormat.toUpperCase() }} · 每批 {{ batchSize }} 行
              <template v-if="replace"> · REPLACE INTO</template>
            </div>
          </div>
          <v-alert v-if="errorMsg" type="error" variant="tonal" density="compact" closable class="mt-2">
            {{ errorMsg }}
          </v-alert>
          <v-alert v-else type="warning" variant="tonal" density="compact" class="mt-2">
            {{ mode === 'export' ? '导出将直接覆盖目标文件。' : '导入将向目标表写入数据。' }}请确认后执行。
          </v-alert>
        </template>

        <!-- 执行结果 -->
        <div v-else class="io-wizard__result">
          <v-icon size="large" color="success" class="mb-1">mdi-check-circle-outline</v-icon>
          <div class="text-subtitle-1">{{ mode === 'export' ? '导出' : '导入' }}成功</div>
          <div class="text-body-2 text-medium-emphasis">
            {{ mode === 'export' ? '导出' : '导入' }} {{ result.rows_total.toLocaleString() }} 行 · 耗时 {{ result.duration_ms.toLocaleString() }} ms
          </div>
        </div>
      </v-card-text>

      <v-divider />
      <v-card-actions>
        <v-btn v-if="step === 2 && result" variant="text" @click="resetWizard">再导一次</v-btn>
        <v-btn v-else-if="step === 2" variant="text" prepend-icon="mdi-chevron-left" @click="step = 1">
          上一步
        </v-btn>
        <v-spacer />
        <v-btn variant="text" @click="close">取消</v-btn>
        <v-btn
          v-if="step === 1"
          color="primary"
          prepend-icon="mdi-arrow-right"
          @click="goNext"
        >
          下一步
        </v-btn>
        <v-btn
          v-else
          color="primary"
          prepend-icon="mdi-play"
          :loading="executing"
          @click="doExecute"
        >
          执行
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import {
  mysqlExport,
  mysqlImport,
} from '@/api/mysqlIo'
import type { MySqlIoResult } from '@/api/mysqlIo'
import { useUiStore } from '@/stores/ui'

/** 向导模式：export = 导出查询结果；import = 导入文件到表 */
type IoMode = 'export' | 'import'

const props = defineProps<{
  /** v-model：对话框显隐 */
  modelValue: boolean
  /** MySQL 连接 ID（useMysqlStore 的 connId，只读） */
  connId: string
}>()

const emit = defineEmits<{
  /** v-model：对话框显隐 */
  (e: 'update:modelValue', value: boolean): void
  /** 执行成功后触发（mode + 结果），供父组件按需刷新 */
  (e: 'completed', mode: IoMode, result: MySqlIoResult): void
}>()

const ui = useUiStore()

// ---------- 向导状态 ----------
const step = ref(1)
const mode = ref<IoMode>('export')

/** 执行结果（成功后展示 rows_total + duration_ms） */
const result = ref<MySqlIoResult | null>(null)
/** 执行错误信息（v-alert 展示） */
const errorMsg = ref('')
/** 执行中标志（导入 replace 确认重调期间也为 true） */
const executing = ref(false)

// ---------- 导出参数 ----------
const exportSql = ref('SELECT * FROM ')
const exportFormat = ref<'csv' | 'json' | 'sql'>('csv')
const includeCreateTable = ref(false)

// ---------- 导入参数 ----------
const importTable = ref('')
const importFormat = ref<'csv' | 'sql'>('csv')
const batchSize = ref(500)
const replace = ref(false)

/** 导出/导入共用的文件路径 */
const filePath = ref('')

/** 各格式对应的文件扩展名（保存对话框默认文件名） */
const EXT_BY_FORMAT: Record<string, string> = { csv: 'csv', json: 'json', sql: 'sql' }

// ---------- 文件路径选择（Tauri dialog 插件） ----------

/** 导出：保存对话框获取目标路径 */
async function pickSavePath(): Promise<void> {
  const ext = EXT_BY_FORMAT[exportFormat.value] ?? 'csv'
  const selected = await save({
    title: '选择导出保存路径',
    defaultPath: `export.${ext}`,
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
  })
  if (typeof selected === 'string') filePath.value = selected
}

/** 导入：打开对话框选择源文件（CSV/SQL 两种格式） */
async function pickOpenPath(): Promise<void> {
  const selected = await open({
    title: '选择导入文件',
    multiple: false,
    filters: [{ name: '数据文件', extensions: ['csv', 'sql'] }],
  })
  if (typeof selected === 'string') filePath.value = selected
}

// ---------- 参数校验 ----------

/** 第一步进入第二步前校验必填项；返回错误提示（空字符串 = 通过） */
function validate(): string {
  if (mode.value === 'export') {
    if (!exportSql.value.trim()) return '请输入查询 SQL'
    if (!filePath.value.trim()) return '请选择保存路径'
  } else {
    if (!filePath.value.trim()) return '请选择导入文件'
    // CSV 格式目标表必填；SQL 格式的语句自带表名，可留空
    if (importFormat.value === 'csv' && !importTable.value.trim()) return '请输入目标表名'
    const size = batchSize.value
    if (!Number.isFinite(size) || size < 1 || size > 1000) return '批量大小需在 1-1000 之间'
  }
  return ''
}

/** 下一步：校验通过进入确认页 */
function goNext(): void {
  const msg = validate()
  if (msg) {
    ui.toast(msg, 'error')
    return
  }
  errorMsg.value = ''
  result.value = null
  step.value = 2
}

// ---------- 执行 ----------

/** 执行导入/导出；replace 未确认时后端报错 → uiStore.confirm 确认后带 confirmed 重调 */
async function doExecute(): Promise<void> {
  errorMsg.value = ''
  const confirmed = true
  try {
    if (mode.value === 'export') {
      result.value = await mysqlExport(props.connId, {
        sql: exportSql.value.trim(),
        format: { format: exportFormat.value },
        file_path: filePath.value.trim(),
        include_create_table: includeCreateTable.value,
      })
    } else {
      const options = {
        file_path: filePath.value.trim(),
        table: importTable.value.trim(),
        format: { format: importFormat.value },
        batch_size: Math.min(1000, Math.max(1, Math.round(batchSize.value))),
        replace: replace.value,
      }
      try {
        result.value = await mysqlImport(props.connId, options)
      } catch (err) {
        const msg = typeof err === 'string' ? err : String(err)
        // replace 覆盖未确认：后端返回带提示错误，弹确认框后带 confirmed 重调
        if (replace.value && /confirm|确认|confirmed/i.test(msg)) {
          const ok = await ui.confirm({
            title: 'REPLACE INTO 覆盖确认',
            message: `导入将以 REPLACE INTO 覆盖表 ${options.table} 中同主键的数据，是否继续？`,
            danger: true,
          })
          if (ok) {
            result.value = await mysqlImport(props.connId, options, confirmed)
          }
          return
        }
        throw err
      }
    }
    const done = result.value
    if (!done) return
    ui.toast(`${mode.value === 'export' ? '导出' : '导入'}成功，共 ${done.rows_total} 行`, 'success')
    emit('completed', mode.value, done)
  } catch (err) {
    errorMsg.value = typeof err === 'string' ? err : String(err)
    ui.toast(errorMsg.value, 'error')
  }
}

// ---------- 显隐与重置 ----------

/** 关闭对话框（执行中不允许关闭，由 persistent 保证） */
function close(): void {
  if (errorMsg.value) {
    // 出错后关闭视为放弃，重置向导
    resetWizard()
  }
  emit('update:modelValue', false)
}

/** 重置向导到第一步（保留输入，清空结果与错误） */
function resetWizard(): void {
  step.value = 1
  result.value = null
  errorMsg.value = ''
}

/** 对话框关闭时复位步骤与结果；开启时重置错误 */
function onDialogToggle(v: boolean): void {
  if (!v) {
    step.value = 1
    result.value = null
    errorMsg.value = ''
  }
  emit('update:modelValue', v)
}
</script>

<style scoped>
.io-wizard__steps {
  display: flex;
  align-items: center;
  gap: 12px;
}

.io-wizard__step {
  font-size: 14px;
  opacity: 0.55;
}

.io-wizard__step--active {
  opacity: 1;
  font-weight: 400;
}

.io-wizard__steps-divider {
  flex: 1 1 auto;
}

.io-wizard__summary {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 12px;
}

.io-wizard__summary-item {
  display: flex;
  gap: 8px;
  font-size: 14px;
  line-height: 1.6;
  word-break: break-all;
}

.io-wizard__summary-key {
  flex: 0 0 auto;
  font-weight: 400;
  opacity: 0.7;
}

.io-wizard__result {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 160px;
  gap: 4px;
}
</style>
