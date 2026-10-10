<template>
  <div class="mysql-cli" @click="focusInput">
    <!-- 顶部状态条：当前库 -->
    <div class="mysql-cli__bar">
      <span>当前库：{{ currentDb ?? '（未选择）' }}</span>
    </div>
    <!-- 输入行（输出区上方，命令在顶部输入） -->
    <div class="mysql-cli__prompt">
      <span class="mysql-cli__prefix">mysql&gt;</span>
      <input
        ref="inputEl"
        v-model="input"
        class="mysql-cli__input"
        spellcheck="false"
        autocomplete="off"
        @keydown="onKeydown"
      />
    </div>
    <!-- 输出区（深色控制台） -->
    <div ref="outputEl" class="mysql-cli__out">
      <div
        v-for="(item, i) in entries"
        :key="i"
        class="mysql-cli__line"
        :class="`mysql-cli__line--${item.kind}`"
      >{{ item.text }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * MysqlCliConsole —— 命令列界面（右键菜单「命令列界面...」入口，Navicat 同款 mysql> 控制台）
 *
 * 深色控制台：mysql> 提示符 + 上下键历史 + ASCII 表格输出（CJK 双列宽对齐）。
 * 任意语句走 mysqlCliExec（不做 COUNT 包装/分页，SHOW/DESC/EXPLAIN 均可）；
 * 危险 SQL（DROP/TRUNCATE/ALTER、无 WHERE 的 DELETE/UPDATE）后端 reject 后
 * ui.confirm 二次确认重发（与查询 Tab execute 同款管道）。全角分号 ； 归一化为
 * ASCII ;（中文输入法误触防护）。
 * 内置命令 exit/quit（关闭 Tab）、clear（清屏）、help、use <库名>（走
 * store.switchDb 重建连接池，连接代际守卫，连接默认库全局生效）。
 * 与 MySQL 工作台共享连接（store.connId），关闭本 Tab 不断开连接。
 */
import { nextTick, onMounted, ref, watch } from 'vue'
import { useMysqlStore, CONFIRM_HINT_RE } from '@/stores/mysql'
import { useUiStore } from '@/stores/ui'
import { mysqlCliExec } from '@/api/mysql'
import { mysqlDbList } from '@/api/mysqlDb'
import type { MySqlQueryResult } from '@/api/types'
import { friendlyError } from '@/utils/errors'

const emit = defineEmits<{ (e: 'exit'): void }>()

const store = useMysqlStore()
const ui = useUiStore()

interface Entry {
  kind: 'input' | 'output' | 'error' | 'info'
  text: string
}

const HELP_TEXT = [
  '内置命令：',
  '  exit | quit     退出命令列界面',
  '  clear           清屏',
  '  use <库名>       切换默认数据库',
  '  help            显示本帮助',
  '其余语句直接回车执行（SELECT/SHOW/DESC/EXPLAIN/INSERT/UPDATE/... 均可）。',
].join('\n')

const entries = ref<Entry[]>([])
const input = ref('')
const history = ref<string[]>([])
const historyIndex = ref(-1)
const running = ref(false)
const currentDb = ref<string | null>(null)
const outputEl = ref<HTMLElement | null>(null)
const inputEl = ref<HTMLInputElement | null>(null)

/** 字符显示宽度：CJK 等全角字符按 2 列计（等宽字体下表格对齐） */
function textWidth(s: string): number {
  let w = 0
  for (const ch of s) {
    w += (ch.codePointAt(0) ?? 0) > 0x7f ? 2 : 1
  }
  return w
}

/** 右侧补空格到指定显示宽度 */
function pad(s: string, width: number): string {
  return s + ' '.repeat(Math.max(0, width - textWidth(s)))
}

/** mysql 客户端风格 ASCII 表格（+---+ 边框 + 列名 + 数据行） */
function formatTable(columns: string[], rows: (string | null)[][]): string {
  const widths = columns.map((c, i) =>
    Math.max(textWidth(c), ...rows.map((r) => textWidth(r[i] ?? 'NULL'))),
  )
  const border = `+${widths.map((w) => '-'.repeat(w + 2)).join('+')}+`
  const lines = [border, `|${columns.map((c, i) => ` ${pad(c, widths[i])} `).join('|')}|`, border]
  for (const row of rows) {
    lines.push(`|${row.map((v, i) => ` ${pad(v ?? 'NULL', widths[i])} `).join('|')}|`)
  }
  lines.push(border)
  return lines.join('\n')
}

function push(kind: Entry['kind'], text: string): void {
  entries.value.push({ kind, text })
}

async function scrollToBottom(): Promise<void> {
  await nextTick()
  const el = outputEl.value
  if (el) el.scrollTop = el.scrollHeight
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    e.preventDefault()
    if (running.value) return
    const raw = input.value
    input.value = ''
    historyIndex.value = -1
    void runLine(raw)
    return
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault()
    // 首次上翻到最新一条；已到最早一条则不再翻
    if (history.value.length === 0 || historyIndex.value === 0) return
    historyIndex.value = historyIndex.value < 0 ? history.value.length - 1 : historyIndex.value - 1
    input.value = history.value[historyIndex.value] ?? ''
    return
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    if (historyIndex.value < 0) return
    const next = historyIndex.value + 1
    if (next >= history.value.length) {
      historyIndex.value = -1
      input.value = ''
      return
    }
    historyIndex.value = next
    input.value = history.value[next] ?? ''
  }
}

/**
 * 执行 SQL（带危险 SQL 二次确认）：首次不带 confirmed，后端 reject 的提示命中
 * CONFIRM_HINT_RE 时弹 ui.confirm，确认后带 confirmed=true 重发；取消抛 CANCELLED。
 */
const CANCELLED = Symbol('cli-cancelled')
async function execSql(connId: string, sql: string): Promise<MySqlQueryResult> {
  try {
    return await mysqlCliExec(connId, sql)
  } catch (e) {
    if (CONFIRM_HINT_RE.test(friendlyError(e))) {
      const ok = await ui.confirm({
        title: '危险操作确认',
        message: `语句包含危险操作（DROP/TRUNCATE/ALTER 或无 WHERE 的 DELETE/UPDATE），确定执行吗？\n\n${sql}`,
        confirmText: '执行',
        danger: true,
      })
      if (!ok) throw CANCELLED
      return await mysqlCliExec(connId, sql, true)
    }
    throw e
  }
}

/** 执行一行：echo → 内置命令分发 → mysql_cli_exec */
async function runLine(raw: string): Promise<void> {
  const line = raw.trim()
  push('input', `mysql> ${raw}`)
  if (!line) {
    await scrollToBottom()
    return
  }
  // 全角分号归一化（中文输入法误触防护）；历史存归一化后语句，上翻重发即干净
  const norm = line.replace(/；/g, ';')
  history.value.push(norm)
  const connId = store.connId
  if (!connId) {
    push('error', '连接已断开，请重新连接后再试')
    await scrollToBottom()
    return
  }
  running.value = true
  try {
    const lower = norm.toLowerCase()
    if (
      lower === 'exit' || lower === 'quit' || lower === '\\q' ||
      lower === 'exit;' || lower === 'quit;' || lower === '\\q;'
    ) {
      push('info', 'Bye')
      emit('exit')
      return
    }
    if (lower === 'clear' || lower === 'cls' || lower === 'clear;' || lower === 'cls;') {
      entries.value = []
      return
    }
    if (lower === 'help' || lower === 'help;' || lower === '?' || lower === '?;') {
      push('info', HELP_TEXT)
      return
    }
    // use <库名> → 切库（统一走 store.switchDb：重建连接池 + 连接代际守卫；
    // 失败由外层 catch 提示；未生效（已被其它入口接管）则不更新本地显示）
    const useMatch = norm.match(/^use\s+(.+?)\s*;?$/i)
    if (useMatch) {
      const dbName = useMatch[1].replace(/`/g, '').trim()
      if (!dbName) {
        push('error', 'ERROR 请输入库名，如 use mydb')
        return
      }
      const applied = await store.switchDb(dbName)
      if (!applied) return
      currentDb.value = dbName
      push('info', 'Database changed')
      return
    }
    const started = performance.now()
    const result = await execSql(connId, norm)
    const elapsed = Math.max(0, Math.round(performance.now() - started))
    if (result.columns.length > 0) {
      push('output', formatTable(result.columns, result.rows))
      let note = `${result.rows.length} row(s) in set (${elapsed} ms)`
      if (result.rows.length >= (result.page_size ?? 1000)) {
        note += '（结果已截断，最多显示 1000 行）'
      }
      push('info', note)
    } else {
      push('info', `Query OK, ${result.total} row(s) affected (${elapsed} ms)`)
    }
  } catch (e) {
    if (e === CANCELLED) {
      push('info', '已取消')
    } else {
      push('error', friendlyError(e))
    }
  } finally {
    running.value = false
    await scrollToBottom()
  }
}

function focusInput(): void {
  inputEl.value?.focus()
}

/** 回读当前库（打开时与连接切换后各一次；失败留空即可）
 *  走 mysql_db_list 的 current_db 字段，不经 cli_exec → 不污染查询历史 */
async function refreshCurrentDb(): Promise<void> {
  const connId = store.connId
  if (!connId) {
    currentDb.value = null
    return
  }
  try {
    const list = await mysqlDbList(connId)
    currentDb.value = list.current_db
  } catch {
    currentDb.value = null
  }
}

onMounted(() => {
  entries.value = [
    { kind: 'info', text: 'MySQL 命令列界面。输入 help 查看内置命令。' },
  ]
  void refreshCurrentDb()
  void nextTick(() => focusInput())
})

watch(
  () => store.connId,
  () => {
    void refreshCurrentDb()
  },
)
</script>

<style scoped>
.mysql-cli {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background: #0c0c0c;
  color: #cccccc;
  cursor: text;
}

/* 顶部状态条 */
.mysql-cli__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 4px 12px;
  font-size: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  flex: none;
}

/* 输出区 */
.mysql-cli__out {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px 12px;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.5;
}

.mysql-cli__line {
  white-space: pre-wrap;
  word-break: break-all;
}

.mysql-cli__line--input {
  color: #ffffff;
}

.mysql-cli__line--output {
  color: #cccccc;
}

.mysql-cli__line--info {
  color: #8a8a8a;
}

.mysql-cli__line--error {
  color: #f14c4c;
}

/* 输入行（输出区上方，命令在顶部输入） */
.mysql-cli__prompt {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  flex: none;
}

.mysql-cli__prefix {
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  color: #ffffff;
  flex: none;
}

.mysql-cli__input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: #ffffff;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  caret-color: #ffffff;
}
</style>
