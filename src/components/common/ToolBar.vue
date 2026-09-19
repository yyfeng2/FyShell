<script setup lang="ts">
/**
 * ToolBar —— 经典工具栏（参考 Xshell）
 *
 * 单行分组式：图标按钮按"新建 | 连接 | 传输"分组，组间竖分隔线；
 * 地址栏（可输入 + 下拉切换会话）在工具栏下方独立一行（AddressBar.vue）。
 * 所有按钮带 title 工具提示（Xshell 风格提示体系）。
 */
const emit = defineEmits<{
  (e: 'new-session'): void
  (e: 'new-folder'): void
  (e: 'connect'): void
  (e: 'disconnect'): void
  (e: 'search'): void
  (e: 'transfer'): void
  (e: 'sftp'): void
  (e: 'help'): void
}>()
</script>

<template>
  <div class="toolbar">
    <!-- 新建组 -->
    <v-btn icon="mdi-plus" size="20" variant="text" title="新建会话 (Ctrl+T)" @click="emit('new-session')" />
    <v-btn icon="mdi-folder-plus-outline" size="20" variant="text" title="新建文件夹" @click="emit('new-folder')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 连接组：连接/断开保留语义色（全工具栏唯一的彩色点缀） -->
    <v-btn icon="mdi-lan-connect" size="20" variant="text" color="success" title="连接选中的会话" @click="emit('connect')" />
    <v-btn icon="mdi-lan-disconnect" size="20" variant="text" color="error" title="断开当前会话" @click="emit('disconnect')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />
    <!-- 传输 / 视图组 -->
    <v-btn icon="mdi-magnify" size="20" variant="text" title="搜索会话" @click="emit('search')" />
    <v-btn icon="mdi-swap-vertical" size="20" variant="text" title="传输队列" @click="emit('transfer')" />
    <v-btn icon="mdi-folder-swap-outline" size="20" variant="text" title="SFTP 文件传输" @click="emit('sftp')" />
    <v-divider vertical inset class="mx-1 toolbar__divider" />

    <v-spacer />
    <v-btn icon="mdi-help-circle-outline" size="20" variant="text" title="帮助" @click="emit('help')" />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  min-height: 26px; /* 与 MenuBar/StatusBar 26px 节奏一致（原 20px 比相邻条带矮 6px） */
  padding: 0 4px;
  background: var(--fy-chrome-bg);
  user-select: none;
}

.toolbar__divider {
  height: 18px;
}

/* 覆盖 Vuetify 竖向 inset divider 默认 margin-block 8px：工具栏行高收紧到内容高 */
.toolbar :deep(.v-divider--inset) {
  margin-block: 0;
}

/* 图标按钮：20px 按钮 + 16px 图标（Vuetify 默认 20px 图标偏粗糙），细腻清晰 */
.toolbar :deep(.v-btn .v-icon) {
  font-size: 16px;
}
</style>
