<template>
  <!-- CodeMirror 6 SQL 编辑器容器（高度随内容自适应，超出滚动） -->
  <div ref="editorHost" class="sql-editor" />

  <!-- 编辑器右键菜单：格式化 / 大小写 / 压缩 / 清空 / 仅运行选中的（fixed 定位，覆盖层负责点击关闭） -->
  <template v-if="ctxMenu">
    <div
      class="sql-editor__ctx-overlay"
      @click="ctxMenu = null"
      @contextmenu.prevent="ctxMenu = null"
    />
    <v-card
      class="sql-editor__ctx-menu"
      :style="{ top: `${ctxMenu.y}px`, left: `${ctxMenu.x}px` }"
    >
      <v-list density="compact" nav>
        <v-list-item prepend-icon="mdi-format-align-left" @click="applyFormat()">
          格式化 SQL
        </v-list-item>
        <v-list-item prepend-icon="mdi-format-letter-case-upper" @click="applyFormat('upper')">
          格式化并转大写
        </v-list-item>
        <v-list-item prepend-icon="mdi-format-letter-case-lower" @click="applyFormat('lower')">
          格式化并转小写
        </v-list-item>
        <v-list-item prepend-icon="mdi-arrow-collapse-horizontal" @click="applyCompress">
          压缩（单行）
        </v-list-item>
        <v-list-item prepend-icon="mdi-eraser" @click="applyClear">清空</v-list-item>
        <v-divider />
        <v-list-item
          prepend-icon="mdi-play-circle-outline"
          :disabled="!hasSelection"
          @click="applyRunSelection"
        >
          仅运行选中的
        </v-list-item>
      </v-list>
    </v-card>
  </template>
</template>

<script setup lang="ts">
/**
 * CodeMirror 6 SQL 编辑器（SQL 控制台编辑区）
 *
 * 能力：
 * - SQL 语法高亮（@codemirror/lang-sql，MySQL 方言，支持反引号标识符）
 * - 关键字补全 + 表名补全（表名来自左侧表列表；列名后端暂无现成数据，不接列级补全）
 * - 浅色主题（与 Vuetify 浅灰/Xshell 经典风格协调）
 * - 高度随内容自适应（min 96px / max 320px，超出内部滚动）
 * - Ctrl+Enter 触发执行（emit('execute')，执行逻辑由父级对齐现有 runSql）
 *
 * 本组件不直接调用 invoke，数据交互一律走 @/api 封装层。
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { EditorView, keymap, placeholder } from '@codemirror/view'
import { Compartment, EditorState, Prec } from '@codemirror/state'
import { basicSetup } from 'codemirror'
import { MySQL, sql } from '@codemirror/lang-sql'
import { compressSql, formatSql } from './sql-format'

const props = defineProps<{
  /** 编辑器内容（v-model） */
  modelValue: string
  /** 占位提示文本（编辑器为空时展示） */
  placeholder?: string
  /** 表名列表（schema 表名补全用，来自左侧表列表） */
  tables?: string[]
}>()

const emit = defineEmits<{
  /** 内容变化（v-model） */
  (e: 'update:modelValue', value: string): void
  /** Ctrl+Enter 触发执行 */
  (e: 'execute'): void
  /** 右键菜单"仅运行选中的"：携带选中区文本（无选中时菜单项禁用） */
  (e: 'executeSelection', text: string): void
}>()

// ---------- CodeMirror 实例与动态扩展分区 ----------
const editorHost = ref<HTMLDivElement | null>(null)
/** CodeMirror 视图实例（onMounted 创建，onBeforeUnmount 销毁） */
let view: EditorView | null = null

/** SQL 语言分区：表名列表变化时 reconfigure，无需重建编辑器 */
const sqlCompartment = new Compartment()

/** 表名列表 -> lang-sql schema 形状（{ 表名: [] }，仅做表名补全） */
function sqlExtension(tables?: string[]): ReturnType<typeof sql> {
  const schema: Record<string, string[]> = {}
  for (const name of tables ?? []) schema[name] = []
  return sql({ dialect: MySQL, upperCaseKeywords: true, schema })
}

/** 浅色主题：白底编辑区 + 浅灰容器，色调与现有 Xshell 浅灰风格协调 */
const lightTheme = EditorView.theme({
  '&': {
    minHeight: '96px',
    maxHeight: '320px',
    background: 'rgb(var(--v-theme-surface))',
    color: 'rgb(var(--v-theme-on-surface))',
  },
  '.cm-scroller': {
    overflow: 'auto',
    fontFamily: "'Cascadia Mono', Consolas, monospace",
    fontSize: '13px',
    lineHeight: '1.6',
  },
  '.cm-gutters': {
    background: 'rgba(var(--v-theme-on-surface), 0.04)',
    color: 'rgba(var(--v-theme-on-surface), 0.4)',
    borderRight: '1px solid rgba(var(--v-theme-on-surface), 0.08)',
  },
  '.cm-activeLine': {
    background: 'rgba(var(--v-theme-primary), 0.06)',
  },
  '.cm-activeLineGutter': {
    background: 'rgba(var(--v-theme-primary), 0.1)',
    color: 'rgb(var(--v-theme-primary))',
  },
})

/** Ctrl+Enter 执行（Prec.highest 避免被 basicSetup 默认键位拦截） */
const executeKeymap = Prec.highest(
  keymap.of([
    {
      key: 'Mod-Enter',
      run: () => {
        emit('execute')
        return true
      },
    },
  ]),
)

// ---------- 右键菜单：格式化 / 大小写 / 压缩 / 清空 / 仅运行选中的 ----------

/** 右键菜单状态：屏幕坐标（null = 菜单关闭） */
const ctxMenu = ref<{ x: number; y: number } | null>(null)

/** 打开菜单时的选中区文本（空串 = 无选中，"仅运行选中的"禁用） */
const hasSelection = ref(false)
/** 打开菜单时缓存的确切选中区文本（点击菜单项时发射，避免选区变化） */
const selectedText = ref('')

/** 替换整个编辑器文档（updateListener 会同步 emit update:modelValue） */
function replaceDoc(text: string): void {
  if (!view) return
  const current = view.state.doc.toString()
  view.dispatch({ changes: { from: 0, to: current.length, insert: text } })
}

/** 格式化（可选关键字大小写），空内容时忽略 */
function applyFormat(keywordCase?: 'upper' | 'lower'): void {
  ctxMenu.value = null
  if (!view) return
  replaceDoc(formatSql(view.state.doc.toString(), keywordCase))
}

/** 压缩为单行 */
function applyCompress(): void {
  ctxMenu.value = null
  if (!view) return
  replaceDoc(compressSql(view.state.doc.toString()))
}

/** 清空编辑器内容 */
function applyClear(): void {
  ctxMenu.value = null
  replaceDoc('')
}

/** 仅运行选中的：携带选中区文本交由父级按 SELECT/写操作路由执行 */
function applyRunSelection(): void {
  ctxMenu.value = null
  if (!view || !hasSelection.value) return
  emit('executeSelection', selectedText.value)
}

onMounted(() => {
  if (!editorHost.value) return
  view = new EditorView({
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        basicSetup,
        sqlCompartment.of(sqlExtension(props.tables)),
        lightTheme,
        // 自动换行：长 SQL 折行展示，不出现横向滚动条
        EditorView.lineWrapping,
        placeholder(props.placeholder ?? ''),
        executeKeymap,
        // 右键菜单拦截：记录打开时刻的选中区文本，交由 Vue 模板渲染菜单
        EditorView.domEventHandlers({
          contextmenu: (event, ev) => {
            const sel = ev.state.selection.main
            const text = ev.state.sliceDoc(sel.from, sel.to).trim()
            hasSelection.value = !!text
            selectedText.value = text
            ctxMenu.value = {
              x: Math.min(event.clientX, window.innerWidth - 180),
              y: Math.min(event.clientY, window.innerHeight - 260),
            }
            return true
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            emit('update:modelValue', update.state.doc.toString())
          }
        }),
      ],
    }),
    parent: editorHost.value,
  })
})

onBeforeUnmount(() => {
  view?.destroy()
  view = null
})

// 外部值变化（如历史召回 recallSql 直接改 sql ref）同步进编辑器
watch(
  () => props.modelValue,
  (value) => {
    const current = view?.state.doc.toString() ?? ''
    if (view && value !== current) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: value } })
    }
  },
)

// 表名列表变化（切换连接/刷新表列表）时动态更新补全数据源
watch(
  () => (props.tables ?? []).join('\n'),
  () => {
    view?.dispatch({ effects: sqlCompartment.reconfigure(sqlExtension(props.tables)) })
  },
)
</script>

<style scoped>
/* 编辑器容器：与现有浅灰面板边框风格一致 */
.sql-editor {
  border: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  border-radius: 4px;
  overflow: hidden;
}

/* 内容区光标色跟随主题（保证浅色下光标可见） */
.sql-editor :deep(.cm-content) {
  caret-color: rgb(var(--v-theme-primary));
}

/* 补全面板：跟随浅色主题（默认白底，覆盖暗色变量风险） */
.sql-editor :deep(.cm-tooltip.cm-tooltip-autocomplete > ul) {
  font-family: 'Cascadia Mono', Consolas, monospace;
  font-size: 12px;
}

/* 编辑器右键菜单覆盖层与菜单本体（fixed 定位，跟随鼠标坐标） */
.sql-editor__ctx-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
}

.sql-editor__ctx-menu {
  position: fixed;
  z-index: 2001;
  min-width: 180px;
}
</style>
