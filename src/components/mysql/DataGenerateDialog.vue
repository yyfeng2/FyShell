<template>
  <v-dialog
    :model-value="modelValue"
    width="720"
    persistent
    @update:model-value="(v: boolean) => onToggle(v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-dice-multiple-outline</v-icon>
        数据生成
        <v-spacer />
        <v-chip size="x-small" variant="tonal" color="primary" class="mr-1">
          {{ store.connLabel }}
        </v-chip>
      </v-card-title>
      <v-divider />

      <v-card-text class="data-generate__body pb-2 pt-3">
        <!-- 目标区：当前连接当前库 + 表下拉 -->
        <div class="fy-field-row">
          <span class="fy-field-row__label">目标表</span>
          <v-select
            :model-value="table"
            :items="tables"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            :loading="tablesLoading"
            @update:model-value="(v: string) => onTableChange(v)"
          />
        </div>
        <div class="fy-field-row">
          <span class="fy-field-row__label">生成行数</span>
          <v-text-field
            v-model.number="rows"
            type="number"
            min="1"
            max="100000"
            density="compact"
            variant="outlined"
            single-line
            hide-details
          />
        </div>
        <v-switch
          v-model="truncate"
          label="生成前清空目标表（TRUNCATE）"
          density="compact"
          hide-details
          color="warning"
        />

        <v-divider class="my-2" />

        <!-- 列规则：列名只读 + 生成类型 + 参数 -->
        <div class="d-flex align-center mb-1">
          <span class="text-subtitle-2">列生成规则（未配置的列取表默认值）</span>
          <v-spacer />
          <v-btn
            size="x-small"
            variant="text"
            :loading="columnsLoading"
            title="重新按列类型预填默认规则"
            @click="loadColumns"
          >
            重置规则
          </v-btn>
        </div>
        <div class="generate__columns">
          <div v-for="(r, i) in rules" :key="r.name" class="generate__rule-row">
            <span class="generate__col-name" :title="r.name">{{ r.name }}</span>
            <v-select
              v-model="r.kind"
              :items="kindItems"
              item-title="label"
              item-value="value"
              density="compact"
              variant="outlined"
              single-line
              hide-details
              class="generate__kind"
              @update:model-value="() => onKindChange(i)"
            />
            <!-- int/decimal：min/max -->
            <template v-if="r.kind === 'int' || r.kind === 'decimal'">
              <v-text-field
                v-model.number="r.min"
                type="number"
                label="min"
                density="compact"
                variant="outlined"
                single-line
                hide-details
                class="generate__num"
              />
              <v-text-field
                v-model.number="r.max"
                type="number"
                label="max"
                density="compact"
                variant="outlined"
                single-line
                hide-details
                class="generate__num"
              />
            </template>
            <!-- string：长度 -->
            <v-text-field
              v-if="r.kind === 'string'"
              v-model.number="r.length"
              type="number"
              min="1"
              label="长度"
              density="compact"
              variant="outlined"
              single-line
              hide-details
              class="generate__num"
            />
            <!-- fixed：候选值 -->
            <v-text-field
              v-if="r.kind === 'fixed'"
              :model-value="r.values.join(',')"
              label="候选值（逗号分隔）"
              density="compact"
              variant="outlined"
              single-line
              hide-details
              class="generate__values"
              @update:model-value="(v: string) => onValuesChange(i, v)"
            />
            <!-- null_ratio：概率置 NULL -->
            <v-text-field
              v-model.number="r.nullRatio"
              type="number"
              min="0"
              max="100"
              label="NULL%"
              density="compact"
              variant="outlined"
              single-line
              hide-details
              class="generate__num generate__null"
            />
          </div>
          <div v-if="!columnsLoading && rules.length === 0" class="text-caption text-medium-emphasis pa-2">
            请先选择表
          </div>
        </div>

        <!-- 预览 3 行样例 -->
        <div v-if="previewRows.length" class="mt-2">
          <div class="text-subtitle-2 mb-1">
            <v-icon size="small" class="mr-1">mdi-eye-outline</v-icon>预览（3 行样例）
          </div>
          <div class="generate__preview">
            <table>
              <thead>
                <tr>
                  <th v-for="c in previewColumns" :key="c">{{ c }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(row, ri) in previewRows" :key="ri">
                  <td v-for="(cell, ci) in row" :key="ci">{{ cell }}</td>
                </tr>
              </tbody>
            </table>
          </div>
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
          prepend-icon="mdi-dice-multiple-outline"
          :loading="executing"
          :disabled="!table || rules.length === 0"
          @click="doGenerate"
        >
          开始生成
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { mysqlListTables } from '@/api/mysql'
import { mysqlTableDesignGet } from '@/api/mysqlDesign'
import { mysqlDataGenerate } from '@/api/mysqlTools'
import type { MySqlGenerateColumnRule } from '@/api/mysqlTools'
import type { MySqlTableInfo } from '@/api/types'
import { useUiStore } from '@/stores/ui'
import { useMysqlStore } from '@/stores/mysql'
import { errText } from '@/composables/useTargetConnection'

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
const store = useMysqlStore()

/** 带界面前端规则：nullRatio 为 0-100 百分数（null_ratio: number | null 提交后端） */
interface UiRule {
  name: string
  kind: string
  min: number | null
  max: number | null
  length: number | null
  values: string[]
  nullRatio: number | null
}

/** 生成类型候选（与后端 do_generate 校验一致） */
const kindItems = [
  { label: '随机整数', value: 'int' },
  { label: '随机小数', value: 'decimal' },
  { label: '随机字符串', value: 'string' },
  { label: 'UUID', value: 'uuid' },
  { label: '中文姓名', value: 'name' },
  { label: '手机号', value: 'phone' },
  { label: '邮箱', value: 'email' },
  { label: '日期时间', value: 'datetime' },
  { label: '候选值', value: 'fixed' },
]

const tables = ref<MySqlTableInfo[]>([])
const tablesLoading = ref(false)
const table = ref('')
const rows = ref(100)
const truncate = ref(false)
const rules = ref<UiRule[]>([])
const columnsLoading = ref(false)
const executing = ref(false)
const errorMsg = ref('')

/** 对话框打开：复位 + 加载表清单并预填规则 */
watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      errorMsg.value = ''
      table.value = ''
      rows.value = 100
      truncate.value = false
      rules.value = []
      void loadTables()
    }
  },
)

/** 加载当前库表清单（不带 db 参数 = 连接当前库，与后端生成目标一致） */
async function loadTables(): Promise<void> {
  if (!props.connId) return
  tablesLoading.value = true
  try {
    tables.value = await mysqlListTables(props.connId)
    if (!table.value || !tables.value.some((t) => t.name === table.value)) {
      table.value = tables.value[0]?.name ?? ''
    }
    if (table.value) {
      await loadColumns()
    }
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    tablesLoading.value = false
  }
}

/** 按列类型预填默认规则（int→随机整数、varchar→随机字符串、datetime→日期时间、enum→解析候选、其余→固定值） */
async function loadColumns(): Promise<void> {
  if (!props.connId || !table.value) return
  columnsLoading.value = true
  try {
    const design = await mysqlTableDesignGet(props.connId, table.value)
    rules.value = design.columns.map((c) => defaultRule(c.name, c.data_type))
    regeneratePreview()
  } catch (err) {
    ui.toast(errText(err), 'error')
  } finally {
    columnsLoading.value = false
  }
}

/** 按列 data_type 推导默认生成规则 */
function defaultRule(name: string, dataType: string): UiRule {
  const t = dataType.toLowerCase()
  if (/^(tiny|small|medium|big)?int/.test(t)) {
    return { name, kind: 'int', min: 1, max: 1000, length: null, values: [], nullRatio: null }
  }
  if (/^(decimal|numeric|float|double)/.test(t)) {
    return { name, kind: 'decimal', min: 0, max: 1000, length: null, values: [], nullRatio: null }
  }
  if (/^(datetime|timestamp|date|time)/.test(t)) {
    return { name, kind: 'datetime', min: null, max: null, length: null, values: [], nullRatio: null }
  }
  // enum('a','b','c') 解析候选值
  const m = /enum\s*\((.+)\)/.exec(t)
  if (m) {
    const values = m[1]
      .split(',')
      .map((v) => v.trim().replace(/^'|'$/g, ''))
      .filter((v) => v)
    return { name, kind: 'fixed', min: null, max: null, length: null, values, nullRatio: null }
  }
  if (/^(varchar|char|text|tinytext|mediumtext|longtext)/.test(t)) {
    return { name, kind: 'string', min: null, max: null, length: 12, values: [], nullRatio: null }
  }
  return { name, kind: 'fixed', min: null, max: null, length: null, values: [], nullRatio: null }
}

/** 切换表：重新预填规则 */
function onTableChange(v: string): void {
  table.value = v
  void loadColumns()
}

/** 生成类型变化：kind 不再对应参数输入时清掉无关值（避免残留提交） */
function onKindChange(i: number): void {
  const r = rules.value[i]
  if (r.kind !== 'fixed') r.values = []
  if (r.kind !== 'string') r.length = null
  if (r.kind !== 'int' && r.kind !== 'decimal') {
    r.min = null
    r.max = null
  }
  regeneratePreview()
}

/** 候选值输入（逗号分隔）回写 */
function onValuesChange(i: number, v: string): void {
  rules.value[i].values = v
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s)
  regeneratePreview()
}

// ---------- 预览：与后端 generate_value 同款规则的前端简化版 ----------
const previewColumns = ref<string[]>([])
const previewRows = ref<string[][]>([])

let previewSeed = 1
function previewRandom(): number {
  // 简单可复现伪随机（mulberry32）
  previewSeed = (previewSeed + 0x6d2b79f5) | 0
  let t = previewSeed
  t = Math.imul(t ^ (t >>> 15), t | 1)
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296
}

/** 重新生成 3 行预览样例 */
function regeneratePreview(): void {
  previewSeed = Date.now() % 2147483647
  previewColumns.value = rules.value.map((r) => r.name)
  const out: string[][] = []
  for (let i = 0; i < 3; i++) {
    out.push(rules.value.map((r) => previewValue(r)))
  }
  previewRows.value = out
}

function previewValue(r: UiRule): string {
  if (r.nullRatio && r.nullRatio > 0 && previewRandom() * 100 < r.nullRatio) return 'NULL'
  const min = r.min ?? 0
  const max = r.max ?? 10000
  switch (r.kind) {
    case 'int': {
      const lo = Math.floor(min)
      return String(lo + Math.floor(previewRandom() * (Math.floor(max) - lo + 1)))
    }
    case 'decimal':
      return (min + previewRandom() * (max - min)).toFixed(2)
    case 'string': {
      const len = Math.max(1, r.length ?? 8)
      const chars = 'abcdefghijklmnopqrstuvwxyz0123456789'
      let s = ''
      for (let i = 0; i < Math.max(1, len); i++) {
        s += chars[Math.floor(previewRandom() * chars.length)]
      }
      return s
    }
    case 'uuid':
      return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
        const r2 = (previewRandom() * 16) | 0
        return (c === 'x' ? r2 : (r2 & 0x3) | 0x8).toString(16)
      })
    case 'name': {
      const surnames = ['张', '王', '李', '赵', '刘', '陈', '杨', '黄', '周', '吴']
      const givens = ['伟', '芳', '娜', '敏', '静', '磊', '军', '洋', '勇', '艳']
      return surnames[Math.floor(previewRandom() * surnames.length)] + givens[Math.floor(previewRandom() * givens.length)]
    }
    case 'phone': {
      const prefixes = ['3', '5', '7', '8', '9']
      let s = '1' + prefixes[Math.floor(previewRandom() * prefixes.length)]
      for (let i = 0; i < 9; i++) s += String(Math.floor(previewRandom() * 10))
      return s
    }
    case 'email': {
      const chars = 'abcdefghijklmnopqrstuvwxyz0123456789'
      let u = ''
      for (let i = 0; i < 8; i++) u += chars[Math.floor(previewRandom() * chars.length)]
      const domains = ['example.com', 'test.org', 'mail.cn', 'demo.net']
      return `${u}@${domains[Math.floor(previewRandom() * domains.length)]}`
    }
    case 'datetime': {
      const now = Date.now()
      const ts = now - Math.floor(previewRandom() * 365 * 24 * 3600 * 1000)
      const d = new Date(ts)
      const p = (n: number) => String(n).padStart(2, '0')
      return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
    }
    case 'fixed':
      return r.values.length ? r.values[Math.floor(previewRandom() * r.values.length)] : ''
    default:
      return ''
  }
}

// ---------- 执行 ----------
/** 执行数据生成（规则转后端契约：空输入强制成数字或 null，nullRatio -> null_ratio 百分数） */
async function doGenerate(): Promise<void> {
  if (!props.connId || !table.value) return
  errorMsg.value = ''
  executing.value = true
  try {
    const numOr = (v: number | null | undefined): number | null =>
      typeof v === 'number' && Number.isFinite(v) ? v : null
    const payload: MySqlGenerateColumnRule[] = rules.value.map((r) => ({
      name: r.name,
      kind: r.kind,
      min: numOr(r.min),
      max: numOr(r.max),
      length: numOr(r.length),
      values: r.values,
      null_ratio: r.nullRatio && r.nullRatio > 0 ? numOr(r.nullRatio) : null,
    }))
    const inserted = await mysqlDataGenerate(props.connId, {
      table: table.value,
      rows: Math.min(100_000, Math.max(1, Math.floor(Number(rows.value) || 100))),
      truncate: truncate.value,
      columns: payload,
    })
    ui.toast(`数据生成完成，共插入 ${inserted.toLocaleString()} 行`, 'success')
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
  if (errorMsg.value) errorMsg.value = ''
  emit('update:modelValue', false)
}

/** 对话框显隐联动（persistent 期间不响应关闭） */
function onToggle(v: boolean): void {
  emit('update:modelValue', v)
}
</script>

<style scoped>
/* 内容整体限高：多字段表时累计高度可达 750px+，避免顶到视口上限 */
.data-generate__body {
  max-height: 70vh;
  overflow-y: auto;
}

/* 列规则行：限高可滚动 */
.generate__columns {
  max-height: 240px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  padding: 4px;
}

.generate__rule-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}

.generate__col-name {
  width: 130px;
  flex: none;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.generate__kind {
  width: 120px;
  flex: none;
}

.generate__num {
  width: 76px;
  flex: none;
}

.generate__values {
  flex: 1;
  min-width: 120px;
}

.generate__null {
  width: 64px;
  flex: none;
}

/* 预览表格 */
.generate__preview {
  max-height: 160px;
  overflow: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
}

.generate__preview table {
  border-collapse: collapse;
  font-size: 12px;
}

.generate__preview th,
.generate__preview td {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  padding: 2px 8px;
  text-align: left;
  white-space: nowrap;
}


/* x-small chip 统一 11px（工具栏按钮档） */
:deep(.v-chip--size-x-small .v-chip__content) {
  font-size: 11px !important;
}
</style>
