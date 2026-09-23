<template>
  <div class="query-tab">
    <!-- 行1：工具条（Navicat 查询编辑器同款） -->
    <div class="query-tab__tools">
      <v-btn variant="text" class="query-tab__tool" @click="openSaveQuery">
        <v-icon size="small" class="mr-1">mdi-content-save-outline</v-icon>保存
      </v-btn>
      <v-btn variant="text" disabled class="query-tab__tool" title="开发中">
        <v-icon size="small" class="mr-1">mdi-cog-outline</v-icon>查询创建工具
      </v-btn>
      <v-btn variant="text" class="query-tab__tool" @click="beautifySql">
        <v-icon size="small" class="mr-1">mdi-auto-fix</v-icon>美化 SQL
      </v-btn>
      <v-menu location="bottom" :close-on-content-click="true">
        <template #activator="{ props: act }">
          <v-btn variant="text" class="query-tab__tool" v-bind="act">
            <v-icon size="small" class="mr-1">mdi-code-tags</v-icon>代码段
          </v-btn>
        </template>
        <v-list density="compact" nav>
          <v-list-item
            v-for="snip in SNIPPETS"
            :key="snip.label"
            prepend-icon="mdi-chevron-right"
            @click="insertSnippet(snip.sql)"
          >
            {{ snip.label }}
          </v-list-item>
        </v-list>
      </v-menu>
      <v-btn variant="text" disabled class="query-tab__tool" title="开发中">
        <v-icon size="small" class="mr-1">mdi-chart-line</v-icon>创建图表
      </v-btn>
    </div>

    <!-- 行2：执行条（连接选择 / 查询下拉 / 运行 / 停止 / 解释） -->
    <div class="query-tab__exec">
      <v-select
        v-model="connSelect"
        :items="connItems"
        density="compact"
        variant="outlined"
        single-line
        hide-details
        class="query-tab__conn-select"
        @update:model-value="onConnChange"
      />
      <v-select
        v-model="savedSelect"
        :items="savedItems"
        density="compact"
        variant="outlined"
        single-line
        hide-details
        class="query-tab__saved-select"
        @update:model-value="onSavedRecall"
      />
      <v-menu location="bottom" :close-on-content-click="true">
        <template #activator="{ props: act }">
          <v-btn variant="text" color="primary" class="query-tab__tool" :loading="running" v-bind="act">
            <v-icon size="small" class="mr-1">mdi-play</v-icon>运行
          </v-btn>
        </template>
        <v-list density="compact" nav>
          <v-list-item prepend-icon="mdi-play" @click="runAll">运行</v-list-item>
          <v-list-item
            prepend-icon="mdi-play-circle-outline"
            :disabled="!hasSelection"
            @click="runSelectionOnly"
          >
            仅运行选中的
          </v-list-item>
        </v-list>
      </v-menu>
      <v-btn variant="text" disabled class="query-tab__tool" title="后端暂不支持取消查询">
        <v-icon size="small" class="mr-1">mdi-stop</v-icon>停止
      </v-btn>
      <v-btn variant="text" class="query-tab__tool" @click="showExplain = true">
        <v-icon size="small" class="mr-1">mdi-chart-bar</v-icon>解释
      </v-btn>
      <!-- 指定数据库：勾选启用（默认不勾选 = 连接当前库），勾选后出现库下拉 -->
      <v-switch
        v-model="useDbEnabled"
        label="指定数据库"
        density="compact"
        hide-details
        class="query-tab__db-switch"
      />
      <v-select
        v-if="useDbEnabled"
        :model-value="dbSelect"
        :items="dbItems"
        density="compact"
        variant="outlined"
        single-line
        hide-details
        :loading="dbLoading"
        class="query-tab__db-select"
        placeholder="当前库"
        @update:model-value="(v: string) => onDbSelect(v)"
      />
    </div>

    <!-- SQL 编辑器 -->
    <div class="query-tab__editor">
      <SqlEditor
        v-model="sql"
        placeholder="输入 SQL，Ctrl+Enter 运行"
        :tables="tableNames"
        @execute="runAll"
        @execute-selection="runSelectionOnly"
        @selection-change="onSelectionChange"
      />
    </div>

    <!-- 结果区（只读，多语句逐条展示；错误统一 toast 提示） -->
    <div class="query-tab__results">
      <div v-for="(res, i) in results" :key="i" class="query-tab__result">
        <div class="query-tab__result-label">
          <v-icon :icon="noteKind(res.note).icon" :color="noteKind(res.note).color" size="x-small" class="mr-1" />
          {{ res.note }}
        </div>
        <div v-if="res.columns.length > 0" class="query-tab__grid">
          <table>
            <thead>
              <tr>
                <th v-for="c in res.columns" :key="c">{{ c }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, ri) in res.rows" :key="ri">
                <td v-for="(v, ci) in row" :key="ci">{{ v ?? 'NULL' }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
      <!-- 结果空态：尚未运行任何 SQL（升级：现代空态组件，引导快捷键） -->
      <EmptyState
        v-if="results.length === 0 && !error"
        icon="mdi-sql-query"
        title="运行 SQL 查看结果"
        desc="输入语句后按 Ctrl+Enter 执行，多语句将逐条展示"
      />
    </div>

    <!-- 解释（执行计划） -->
    <ExplainPanel v-model="showExplain" :conn-id="store.connId ?? ''" :sql="sql" />

    <!-- 保存查询对话框：命名保存当前 SQL -->
    <v-dialog v-model="showSaveQuery" max-width="420">
      <v-card>
        <v-card-title class="d-flex align-center text-subtitle-1">保存查询</v-card-title>
        <v-divider />
        <v-card-text>
          <div class="text-caption text-medium-emphasis mb-1">当前 SQL</div>
          <div class="query-tab__preview">{{ savePreview }}</div>
          <v-text-field
            v-model="saveName"
            density="compact"
            variant="outlined"
            single-line
            hide-details
            autofocus
            label="查询名称"
            class="mt-3"
            @keyup.enter="confirmSave"
          />
        </v-card-text>
        <v-card-actions>
          <v-spacer />
          <v-btn variant="text" @click="showSaveQuery = false">取消</v-btn>
          <v-btn color="primary" :loading="saving" @click="confirmSave">保存</v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script setup lang="ts">
/**
 * MysqlQueryTab —— 查询 Tab（右键菜单「新建查询」入口，Navicat 查询编辑器同款）
 *
 * 两行工具条：保存/查询创建工具(开发中)/美化 SQL/代码段/创建图表(开发中)
 * + 连接下拉（已存连接切换）/查询下拉（已保存查询快召回）/运行▶/停止(置灰)/解释
 * + 指定数据库（勾选启用 + 库下拉，默认不选 = 连接当前库，选中即切库全局生效）。
 * 编辑器复用 SqlEditor（CodeMirror 6）；结果区只读表格，多语句逐条展示。
 * 语句路由与工作台同型：SELECT → mysqlQuery（分页 100/页），其余 → mysqlExecute
 * （危险 SQL 前端捕获后 ui.confirm 二次确认重调）。连接切换走 store.connectSaved
 * （全局生效，工作台同步）。每次右键新开一个查询 Tab，关闭不影响连接。
 */
import { computed, ref, watch } from 'vue'
import SqlEditor from './SqlEditor.vue'
import ExplainPanel from './ExplainPanel.vue'
import EmptyState from '@/components/common/EmptyState.vue'
import { formatSql, splitSqlStatements } from './sql-format'
import { useMysqlStore } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'
import { mysqlQuery, mysqlExecute } from '@/api/mysql'
import { mysqlDbList, mysqlDbSwitch } from '@/api/mysqlDb'
import { mysqlSavedQueryList, mysqlSavedQuerySave, type MySqlSavedQueryItem } from '@/api/mysqlConsole'
import { friendlyError as errText } from '@/utils/errors'

const props = defineProps<{
  /** 打开时预填的 SQL（当前查询 Tab 为空） */
  initialSql?: string
}>()

const store = useMysqlStore()
const ui = useUiStore()

/** 只读 SELECT 路由到 mysqlQuery（分页），其余语句走 mysqlExecute（写操作） */
const SELECT_RE = /^\s*SELECT\b/i

const PAGE_SIZE = 100

/** 代码段（内置 SQL 模板，插入编辑器） */
const SNIPPETS: { label: string; sql: string }[] = [
  { label: '查询所有表', sql: 'SHOW TABLES' },
  { label: '表结构', sql: 'SHOW CREATE TABLE `table`' },
  { label: 'SELECT 模板', sql: 'SELECT * FROM `table` LIMIT 100' },
  { label: 'JOIN 模板', sql: 'SELECT a.*, b.*\nFROM `t1` a\nJOIN `t2` b ON a.id = b.id' },
  { label: 'INSERT 模板', sql: "INSERT INTO `table` (col1, col2) VALUES ('v1', 'v2')" },
  { label: 'UPDATE 模板', sql: "UPDATE `table` SET col = 'v' WHERE id = 1" },
  { label: 'DELETE 模板', sql: 'DELETE FROM `table` WHERE id = 1' },
  { label: '分页模板', sql: 'SELECT * FROM `table` LIMIT 10 OFFSET 0' },
]

interface ResultItem {
  columns: string[]
  rows: (string | null)[][]
  note: string
}

/** 结果 note 语义分类（仅影响视觉，text 不变）：SELECT=primary、写操作=success、已取消=warning */
function noteKind(note: string): { icon: string; color: string } {
  if (note.startsWith('Query OK')) return { icon: 'mdi-check-circle', color: 'success' }
  if (note === '已取消') return { icon: 'mdi-minus-circle', color: 'warning' }
  return { icon: 'mdi-play-circle-outline', color: 'primary' }
}

const sql = ref(props.initialSql ?? '')
const results = ref<ResultItem[]>([])
const error = ref('')
const running = ref(false)
const hasSelection = ref(false)
const showExplain = ref(false)

// ---------- 保存查询 ----------
const showSaveQuery = ref(false)
const saveName = ref('')
const saving = ref(false)

const savePreview = computed(() => {
  const text = sql.value.trim()
  return text.length > 60 ? text.slice(0, 60) + '…' : text || '（当前 SQL 为空）'
})

function openSaveQuery(): void {
  saveName.value = ''
  showSaveQuery.value = true
}

/** 命名保存当前 SQL：重名时经 uiStore.confirm 覆盖确认（后端 UPSERT 保留原 id） */
async function confirmSave(): Promise<void> {
  const name = saveName.value.trim()
  if (!name) {
    ui.toast('请输入查询名称', 'warning')
    return
  }
  const text = sql.value.trim()
  if (!text) {
    ui.toast('当前 SQL 为空，无法保存', 'warning')
    return
  }
  saving.value = true
  try {
    const saved = await mysqlSavedQueryList()
    let willOverwrite = false
    if (saved.some((q) => q.name === name)) {
      const ok = await ui.confirm({
        title: '覆盖确认',
        message: `已存在同名查询「${name}」，确定覆盖吗？`,
        confirmText: '覆盖',
      })
      if (!ok) return
      willOverwrite = true
    }
    const overwritten = await mysqlSavedQuerySave(name, store.connId, text, willOverwrite)
    ui.toast(overwritten ? `已覆盖查询「${name}」` : `已保存查询「${name}」`, 'success')
    showSaveQuery.value = false
    await loadSavedQueries()
  } catch (e) {
    ui.toast(`保存失败：${errText(e)}`, 'error')
  } finally {
    saving.value = false
  }
}

// ---------- 已保存查询（下拉快召回） ----------
const savedSelect = ref<string | null>(null)
const savedQueries = ref<MySqlSavedQueryItem[]>([])

const savedItems = computed(() => {
  const items = savedQueries.value.map((q) => ({ title: q.name, value: q.name }))
  return [{ title: '已保存查询', value: null as string | null }, ...items]
})

async function loadSavedQueries(): Promise<void> {
  try {
    savedQueries.value = await mysqlSavedQueryList()
  } catch {
    savedQueries.value = []
  }
}

/** 快召回：选中的已保存查询插入编辑器 */
function onSavedRecall(): void {
  const q = savedQueries.value.find((x) => x.name === savedSelect.value)
  if (!q) return
  sql.value = q.sql
  savedSelect.value = null
}

// ---------- 连接下拉（已存连接切换） ----------
const connSelect = ref<string | null>(null)

const connItems = computed(() => {
  const items = store.savedConnections.map((c) => ({ title: `${c.host}:${c.port}`, value: c.id }))
  return [{ title: '未选择连接', value: null as string | null }, ...items]
})

/** 选中已存连接：切换连接（全局生效，工作台同步），新连接表清单重载 */
async function onConnChange(): Promise<void> {
  const id = connSelect.value
  if (!id || id === store.activeSavedId) return
  try {
    await store.connectSaved(id)
    store.tables = []
    await store.loadTables()
  } catch {
    // 连接失败由 store.connError / 工作台 v-alert 展示
  }
}

// 连接变化后回显下拉（含外部切换：树单击/工作台连接）
watch(
  () => store.activeSavedId,
  (id) => {
    connSelect.value = id ?? null
  },
  { immediate: true },
)

// ---------- 指定数据库（勾选启用，默认不选 = 连接当前库） ----------
const useDbEnabled = ref(false)
const dbSelect = ref('')
const dbItems = ref<string[]>([])
const dbLoading = ref(false)

/** 勾选启用后加载库列表；取消勾选复位选择 */
watch(useDbEnabled, (on) => {
  if (on) void loadDbs()
  else dbSelect.value = ''
})

async function loadDbs(): Promise<void> {
  const connId = store.connId
  if (!connId) return
  dbLoading.value = true
  try {
    dbItems.value = (await mysqlDbList(connId)).databases
  } catch (e) {
    ui.toast(`数据库列表获取失败：${errText(e)}`, 'error')
  } finally {
    dbLoading.value = false
  }
}

/** 选中数据库：切库（mysqlDbSwitch 重建连接池，全局生效，工作台同步） */
async function onDbSelect(db: string): Promise<void> {
  const connId = store.connId
  if (!connId || !db) return
  try {
    const newId = await mysqlDbSwitch(connId, db)
    store.connId = newId
    store.tables = []
    await store.loadTables()
    ui.toast(`已切换到数据库 ${db}`, 'success')
  } catch (e) {
    ui.toast(errText(e), 'error')
  }
}

// 连接切换后库列表失效：复位选择并重载（勾选保持）
watch(
  () => store.connId,
  () => {
    dbSelect.value = ''
    if (useDbEnabled.value) void loadDbs()
  },
)

// ---------- 编辑器工具 ----------
function beautifySql(): void {
  if (!sql.value.trim()) return
  sql.value = formatSql(sql.value)
}

function insertSnippet(text: string): void {
  sql.value = sql.value.trimEnd() ? `${sql.value.trimEnd()}\n${text}` : text
}

// ---------- 运行 ----------
function runAll(): void {
  if (!sql.value.trim()) {
    ui.toast('请输入 SQL 后再运行', 'warning')
    return
  }
  void run(sql.value)
}

function runSelectionOnly(): void {
  if (selectionText.value) void run(selectionText.value)
}

/** 编辑器选中区文本（SqlEditor executeSelection 事件回写） */
const selectionText = ref('')

/** 执行 SQL：多语句逐条路由（SELECT 分页查询，其余写操作），结果逐条展示 */
async function run(text: string): Promise<void> {
  const connId = store.connId
  if (!connId) {
    ui.toast('请先连接 MySQL 数据库后再运行', 'warning')
    return
  }
  if (!text.trim()) return
  running.value = true
  error.value = ''
  results.value = []
  try {
    const stmts = splitSqlStatements(text)
    for (const stmt of stmts) {
      if (SELECT_RE.test(stmt)) {
        const result = await mysqlQuery(connId, stmt, 1, PAGE_SIZE)
        results.value.push({
          columns: result.columns,
          rows: result.rows,
          note: `${result.total} 行（LIMIT ${PAGE_SIZE}）`,
        })
      } else {
        try {
          const affected = await mysqlExecute(connId, stmt)
          results.value.push({
            columns: [],
            rows: [],
            note: `Query OK，${affected} 行受影响`,
          })
        } catch (e) {
          const msg = errText(e)
          // 危险 SQL：首次未带 confirmed，后端 reject → 二次确认后重调
          if (msg.includes('confirmed')) {
            const ok = await ui.confirm({
              title: '危险操作确认',
              message: `语句包含危险操作（DROP/TRUNCATE/ALTER 或无 WHERE 的 DELETE/UPDATE），确定执行吗？\n\n${stmt}`,
              confirmText: '执行',
              danger: true,
            })
            if (!ok) {
              results.value.push({ columns: [], rows: [], note: '已取消' })
              continue
            }
            const affected = await mysqlExecute(connId, stmt, true)
            results.value.push({
              columns: [],
              rows: [],
              note: `Query OK，${affected} 行受影响`,
            })
          } else {
            throw e
          }
        }
      }
    }
  } catch (e) {
    ui.toast(errText(e), 'error')
  } finally {
    running.value = false
  }
}

// 编辑器选中区回写（SqlEditor executeSelection 事件）
function onSelectionChange(text: string): void {
  selectionText.value = text
  hasSelection.value = Boolean(text)
}

// ---------- 表名补全列表（当前连接的表） ----------
const tableNames = computed(() => store.tables.map((t) => t.name))

// 打开时加载已保存查询（快召回候选）
void loadSavedQueries()
</script>

<style scoped>
/* 行1 工具条：项目工作台功能按钮同款 12px + 0.2em + 0.3em */
.query-tab__tools {
  display: flex;
  align-items: center;
  gap: 0.3em;
  padding: 4px 12px;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  flex: none;
}

.query-tab__tool {
  --v-btn-size: 12px;
  --v-btn-height: auto;
  height: auto;
  padding: 0.2em 0.3em;
  font-size: 12px;
  text-transform: none;
  letter-spacing: 0;
}

/* 行2 执行条 */
.query-tab__exec {
  display: flex;
  align-items: center;
  gap: 0.5em;
  padding: 4px 12px;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  flex: none;
}

.query-tab__conn-select {
  flex: 0 0 220px;
  max-width: 220px;
}

.query-tab__saved-select {
  flex: 0 0 180px;
  max-width: 180px;
}

/* 指定数据库：勾选开关 + 库下拉 */
.query-tab__db-switch {
  flex: none;
  margin-right: -0.4em;
}

.query-tab__db-select {
  flex: 0 0 160px;
  max-width: 160px;
}

/* 编辑器与结果区 */
.query-tab__editor {
  flex: 0 0 auto;
  padding: 8px 12px 0;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.08);
}

.query-tab__results {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px 12px;
}

.query-tab__result {
  margin-bottom: 12px;
}

.query-tab__result-label {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.6);
  margin-bottom: 4px;
}

.query-tab__grid table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

.query-tab__grid th,
.query-tab__grid td {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  padding: 4px 8px;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 320px;
}

.query-tab__grid th {
  background: rgba(var(--v-theme-on-surface), 0.04);
  font-weight: 500;
}

.query-tab__preview {
  font-size: 12px;
  font-family: Consolas, 'Courier New', monospace;
  color: rgba(var(--v-theme-on-surface), 0.7);
  background: rgba(var(--v-theme-on-surface), 0.04);
  border-radius: 4px;
  padding: 6px 8px;
}

</style>
