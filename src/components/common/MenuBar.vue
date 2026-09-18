<script setup lang="ts">
/**
 * MenuBar —— 桌面风格菜单栏（参考 Xshell）
 *
 * 文件/编辑/查看/工具/选项/窗口/帮助 七个菜单，带加速下划线字母。
 * 菜单项通过 action 事件抛给父级接线，本组件不含业务逻辑。
 */
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

const MENUS: MenuDef[] = [
  {
    title: '文件',
    accel: 'F',
    items: [
      { title: '新建会话', action: 'new-session', shortcut: 'Ctrl+N' },
      { title: '新建文件夹', action: 'new-folder' },
      { title: '终端', action: 'local-terminal' },
      { title: '退出', action: 'quit', dividerBefore: true },
    ],
  },
  {
    title: '编辑',
    accel: 'E',
    items: [
      { title: '复制', action: 'copy' },
      { title: '粘贴', action: 'paste' },
      { title: '全选', action: 'select-all' },
    ],
  },
  {
    title: '查看',
    accel: 'V',
    items: [
      { title: '切换左导航', action: 'toggle-nav' },
      { title: '快速命令栏', action: 'toggle-quickbar' },
      { title: '切换主题（深色/浅色）', action: 'toggle-theme' },
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
      { title: '会话设置', action: 'session-settings' },
      { title: 'MySQL', action: 'mysql' },
      { title: '会话日志', action: 'session-log', dividerBefore: true },
      { title: '主密码设置', action: 'master-password' },
    ],
  },
  {
    title: '选项',
    accel: 'B',
    items: [
      { title: '设置', action: 'settings' },
      { title: '切换主题', action: 'toggle-theme' },
      { title: '左导航自动隐藏', action: 'toggle-nav-autohide' },
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
      { title: '检测更新', action: 'check-update' },
      { title: '关于 FyShell', action: 'about', dividerBefore: true },
    ],
  },
]

defineEmits<{ (e: 'action', action: string): void }>()
</script>

<template>
  <nav class="menubar">
    <v-menu v-for="menu in MENUS" :key="menu.title" :close-on-content-click="true">
      <template #activator="{ props: act }">
        <button class="menubar__item" v-bind="act">
          {{ menu.title }}<span class="menubar__accel">{{ menu.accel }}</span>
        </button>
      </template>
      <v-list density="compact" class="menubar__list">
        <template v-for="item in menu.items" :key="item.action">
          <v-divider v-if="item.dividerBefore" />
          <v-list-item :value="item.action" @click="$emit('action', item.action)">
            <v-list-item-title>{{ item.title }}</v-list-item-title>
            <v-list-item-action v-if="item.shortcut">
              <span class="menubar__shortcut">{{ item.shortcut }}</span>
            </v-list-item-action>
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
  border-bottom: 1px solid var(--fy-chrome-border, #d5d9de);
  background: var(--fy-chrome-bg, #f0f2f5);
  user-select: none;
}

.menubar__shortcut {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.45);
  font-family: var(--fy-font);
}

.menubar__item {
  font-size: 14px;
  padding: 2px 8px;
  border-radius: 3px;
  cursor: pointer;
  color: inherit;
  background: transparent;
  border: none;
  line-height: 1.4;
}

.menubar__item:hover {
  background: rgb(var(--v-theme-on-surface) / 0.08);
}

.menubar__accel {
  text-decoration: underline;
  margin-left: 1px;
}
</style>
