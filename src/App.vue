<script setup lang="ts">
/**
 * 应用根组件
 *
 * 渲染主工作区视图（由 views/WorkspaceView.vue 提供）。
 * 主题模式（浅色/深色/跟随系统）单一来源为 stores/settings.ts（SQLite theme_mode）：
 * 挂载后触发 ensureLoaded 从后端加载设置并应用主题（GlobalDialog 统一写入 Vuetify）。
 * 全局压制 WebView 默认右键菜单（空白处弹出的系统菜单不属于本程序）：
 * 输入框/文本域/可编辑元素例外（保留系统剪切板菜单），终端右键由 useXterm 自行处理。
 */
import { onMounted } from 'vue'
import WorkspaceView from '@/views/WorkspaceView.vue'
import { useSettingsStore } from '@/stores/settings'

onMounted(() => {
  // 加载后端设置（幂等）：SQLite 的 theme_mode 启动即生效，而非等用户打开设置对话框
  void useSettingsStore().ensureLoaded()
  // 全局右键菜单压制：非功能性区域 preventDefault，输入类元素放行
  document.addEventListener('contextmenu', (e) => {
    const t = e.target as HTMLElement | null
    if (t && t.closest('input, textarea, [contenteditable="true"]')) return
    e.preventDefault()
  })
})
</script>

<template>
  <!-- v-app 包裹整个应用：Vuetify 3 useTheme 需要 v-app provide，缺失会导致主题切换失效 -->
  <v-app>
    <WorkspaceView />
  </v-app>
</template>
