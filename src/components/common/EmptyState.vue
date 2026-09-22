<script setup lang="ts">
/**
 * 统一空态组件（批3）：现代工具的"留白式空态"，替代零散布的单行灰字。
 * 视觉：32px 线性 icon（on-surface/0.35）+ 16px 标题 + 12px 说明，居中、充足内边距、max-width 约束。
 * 深浅主题自动（全部走主题变量，无硬编码色）。
 */
defineProps<{
  /** 图标名（mdi 或 lucide 均可，透传给 v-icon） */
  icon?: string
  title: string
  desc?: string
  /** 可选主动作按钮文案；点击抛 action 事件 */
  actionText?: string
}>()

const emit = defineEmits<{
  (e: 'action'): void
}>()
</script>

<template>
  <div class="fy-empty">
    <v-icon v-if="icon" :icon="icon" size="32" class="fy-empty__icon" />
    <div class="fy-empty__title">{{ title }}</div>
    <div v-if="desc" class="fy-empty__desc">{{ desc }}</div>
    <v-btn v-if="actionText" size="small" variant="tonal" density="compact" color="primary" class="mt-2" @click="emit('action')">
      {{ actionText }}
    </v-btn>
    <!-- 扩展动作区（欢迎页多按钮场景：slot 内自由排列） -->
    <slot />
  </div>
</template>

<style scoped>
.fy-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  max-width: 360px;
  margin: 0 auto;
  padding: 40px 24px;
  text-align: center;
}

.fy-empty__icon {
  opacity: 0.35;
  margin-bottom: 4px;
}

.fy-empty__title {
  font-size: 16px;
  color: rgb(var(--v-theme-on-surface));
}

.fy-empty__desc {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.55);
  line-height: 1.6;
}
</style>
