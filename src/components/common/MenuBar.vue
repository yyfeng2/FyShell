<script setup lang="ts">
/**
 * MenuBar —— 桌面风格菜单栏（参考 Xshell）
 *
 * 文件/编辑/查看/工具/设置/窗口/帮助 七个菜单，带加速下划线字母（Alt+字母
 * 经父级注册的全局快捷键程序化打开，是真实行为而非装饰）。
 * 配置类项（全局/会话设置、主密码、主题、自动隐藏）集中到"设置"菜单，
 * "工具"只留功能入口。菜单项通过 action 事件抛给父级接线，本组件不含业务逻辑；
 * 开关类项（查看菜单）与窗口菜单标签列表的状态由父级经 props 注入。
 */
import { ref } from 'vue'
import { formatShortcutCombo, SETTING_KEYS, useSettingsStore } from '@/stores/settings'

interface MenuItem {
  title: string
  action: string
  /** 右侧快捷键提示（Xshell 菜单风格） */
  shortcut?: string
  dividerBefore?: boolean
}

interface MenuDef {
  title: string
  accel: string
  items: MenuItem[]
}

/** 窗口菜单动态标签项（父级注入；Xshell 窗口菜单惯例：列出全部已开标签供跳转） */
interface WindowTabItem {
  id: string
  title: string
  active: boolean
}

const props = defineProps<{
  /** 查看菜单开关状态（勾选标记渲染用；左导航传折叠取反值） */
  toggles?: { nav: boolean; quickbar: boolean; composebar: boolean }
  /** 窗口菜单已开标签列表（缺省不渲染动态标签区） */
  windowTabs?: WindowTabItem[]
}>()

const emit = defineEmits<{
  (e: 'action', action: string): void
  (e: 'tab-action', id: string): void
}>()

const MENUS: MenuDef[] = [
  {
    title: '文件',
    accel: 'F',
    items: [
      { title: '新建会话', action: 'new-session', shortcut: 'Ctrl+T' },
      { title: '新建文件夹', action: 'new-folder' },
      { title: '打开…', action: 'open-session-list' },
      { title: '会话日志', action: 'session-log', dividerBefore: true },
      { title: '退出', action: 'quit', dividerBefore: true },
    ],
  },
  {
    title: '编辑',
    accel: 'E',
    items: [
      { title: '剪切', action: 'cut', shortcut: 'Ctrl+X' },
      { title: '复制', action: 'copy', shortcut: 'Ctrl+C' },
      { title: '粘贴', action: 'paste', shortcut: 'Ctrl+V' },
      { title: '全选', action: 'select-all', shortcut: 'Ctrl+A' },
    ],
  },
  {
    title: '查看',
    accel: 'V',
    items: [
      { title: '左导航', action: 'toggle-nav' },
      { title: '命令栏', action: 'toggle-quickbar' },
      { title: '撰写栏', action: 'toggle-composebar' },
    ],
  },
  {
    title: '工具',
    accel: 'T',
    items: [
      { title: '传输队列', action: 'transfer' },
      { title: 'SFTP 文件传输', action: 'sftp' },
      { title: '快捷命令', action: 'quick-command' },
      { title: 'SSH 隧道', action: 'tunnel' },
      { title: 'MySQL', action: 'mysql' },
      { title: 'Redis', action: 'redis' },
      { title: '终端', action: 'local-terminal', dividerBefore: true },
    ],
  },
  {
    title: '设置',
    accel: 'S',
    items: [
      { title: '全局设置…', action: 'settings' },
      { title: '主密码设置…', action: 'master-password', dividerBefore: true },
    ],
  },
  {
    title: '窗口',
    accel: 'W',
    items: [{ title: '下一个标签', action: 'next-tab', shortcut: 'Ctrl+Tab' }],
  },
  {
    title: '帮助',
    accel: 'H',
    items: [
      { title: '快捷键列表…', action: 'shortcut-list', dividerBefore: true },
      { title: '检测更新', action: 'check-update' },
      { title: '关于 FyShell', action: 'about', dividerBefore: true },
    ],
  },
]

/** 查看菜单开关项的勾选状态（由父级注入的 store 状态决定） */
function isChecked(action: string): boolean {
  if (!props.toggles) return false
  switch (action) {
    case 'toggle-nav':
      return props.toggles.nav
    case 'toggle-quickbar':
      return props.toggles.quickbar
    case 'toggle-composebar':
      return props.toggles.composebar
    default:
      return false
  }
}

const settings = useSettingsStore()

/** 可修改快捷键项（action → settings 键）：标注按设置值实时显示，与快捷键设置联动 */
const ACTION_SETTING_KEYS: Record<string, string> = {
  'new-session': SETTING_KEYS.shortcutNewSession,
  cut: SETTING_KEYS.shortcutCut,
  copy: SETTING_KEYS.shortcutCopy,
  paste: SETTING_KEYS.shortcutPaste,
  'select-all': SETTING_KEYS.shortcutSelectAll,
  'next-tab': SETTING_KEYS.shortcutNextTab,
}

/** 快捷键标注动态解析：可修改项取设置值（空回落静态标注），其余用静态标注 */
function shortcutLabel(item: MenuItem): string {
  const key = ACTION_SETTING_KEYS[item.action]
  if (key) {
    const v = settings.shortcutValue(key)
    if (v) return formatShortcutCombo(v)
  }
  return item.shortcut ?? ''
}

/** 该菜单是否渲染勾选列（含开关项或动态标签勾选时，全菜单统一保留列宽——Windows 菜单惯例） */
function hasCheckColumn(menu: MenuDef): boolean {
  return (
    menu.title === '查看' ||
    (menu.title === '窗口' && (props.windowTabs?.length ?? 0) > 0)
  )
}

/** 各菜单展开态（v-model 双向绑定；Alt+字母导航经 openMenu 程序化打开） */
const openState = ref<boolean[]>(MENUS.map(() => false))

/** Alt+字母菜单导航：打开索引对应菜单并关闭其余 */
function openMenu(index: number): void {
  openState.value = openState.value.map((_, i) => i === index)
}

defineExpose({ openMenu })
</script>

<template>
  <nav class="menubar">
    <v-menu
      v-for="(menu, idx) in MENUS"
      :key="menu.title"
      v-model="openState[idx]"
      :close-on-content-click="true"
    >
      <template #activator="{ props: act }">
        <button class="menubar__item" v-bind="act">
          {{ menu.title }}<span class="menubar__accel">{{ menu.accel }}</span>
        </button>
      </template>
      <v-list density="compact" class="menubar__list">
        <template v-for="item in menu.items" :key="item.action">
          <v-divider v-if="item.dividerBefore" />
          <v-list-item :value="item.action" @click="$emit('action', item.action)">
            <!-- 开关项勾选标记：prepend 固定宽度占位保持标题对齐（Windows 菜单勾选列惯例） -->
            <template v-if="hasCheckColumn(menu)" #prepend>
              <span class="menubar__checkmark">
                <v-icon v-if="isChecked(item.action)" icon="mdi-check" size="13" />
              </span>
            </template>
            <v-list-item-title>{{ item.title }}</v-list-item-title>
            <!-- 快捷键提示放 append 槽：同行右对齐（v-list-item-action 嵌默认槽会渲染为标题下方块级元素） -->
            <template #append>
              <span v-if="shortcutLabel(item)" class="menubar__shortcut">{{ shortcutLabel(item) }}</span>
            </template>
          </v-list-item>
        </template>
        <!-- 窗口菜单动态标签区：全部已开标签带序号（Xshell 惯例「1 会话名」对应 Alt+数字直达），勾选指示当前激活 -->
        <template v-if="menu.title === '窗口' && (props.windowTabs?.length ?? 0) > 0">
          <v-divider />
          <v-list-item
            v-for="(t, i) in props.windowTabs"
            :key="t.id"
            :value="`tab:${t.id}`"
            @click="$emit('tab-action', t.id)"
          >
            <template #prepend>
              <span class="menubar__checkmark">
                <v-icon v-if="t.active" icon="mdi-check" size="13" />
              </span>
            </template>
            <v-list-item-title>{{ i + 1 }} {{ t.title }}</v-list-item-title>
          </v-list-item>
        </template>
      </v-list>
    </v-menu>
  </nav>
</template>

<style scoped>
.menubar {
  display: flex;
  align-items: center;
  height: 26px;
  padding: 0 4px;
  border-bottom: 1px solid var(--fy-chrome-border);
  background: var(--fy-chrome-bg);
  user-select: none;
}

/* 下拉菜单项行距压缩到最小（0.1 字符级）：覆盖 Vuetify v-list-item 默认行高/padding */
.menubar :deep(.v-list-item) {
  min-height: 0;
  padding-top: 2px;
  padding-bottom: 2px;
}

.menubar :deep(.v-list-item-title) {
  font-size: 13px;
  line-height: 1.3;
}

.menubar :deep(.v-divider) {
  margin: 2px 0;
}

.menubar__shortcut {
  font-size: 13px;
  color: rgba(var(--v-theme-on-surface), 0.45);
  font-family: var(--fy-font);
}

/* 勾选列固定宽度：未勾选项也占位，标题对齐（Windows 菜单惯例） */
.menubar__checkmark {
  display: inline-flex;
  width: 14px;
  justify-content: center;
  flex-shrink: 0;
}

.menubar__item {
  font-size: 13px;
  padding: 0 10px;
  border-radius: 4px;
  cursor: pointer;
  color: inherit;
  background: transparent;
  border: none;
  line-height: 26px;
}

.menubar__item:hover {
  background: rgba(var(--v-theme-on-surface), 0.08);
}

.menubar__accel {
  text-decoration: underline;
  margin-left: 1px;
}
</style>
