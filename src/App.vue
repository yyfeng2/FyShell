<script setup lang="ts">
/**
 * 应用根组件
 *
 * 渲染主工作区视图（由 views/WorkspaceView.vue 提供）。
 * 主题跟随系统（dark-light-auto）：挂载后监听系统深浅色偏好并切换 Vuetify theme，
 * 具体监听逻辑统一在 plugins/vuetify.ts 的 syncThemeWithSystem 中实现。
 */
import { onMounted } from 'vue'
import WorkspaceView from '@/views/WorkspaceView.vue'
import { applyInitialTheme, vuetify } from './plugins/vuetify'

onMounted(() => {
  // 初始化主题：已手动存储过则用存储值并锁定（不再跟随系统）；否则跟随系统（dark-light-auto）
  applyInitialTheme(vuetify)
})
</script>

<template>
  <!-- v-app 包裹整个应用：Vuetify 3 useTheme 需要 v-app provide，缺失会导致主题切换失效 -->
  <v-app>
    <WorkspaceView />
  </v-app>
</template>
