<script setup lang="ts">
/**
 * 全局弹层容器（store 驱动）
 *
 * 由 useUiStore() 驱动，负责渲染：
 * - 全局确认弹层（ui.confirm() 返回 Promise<boolean>，危险操作二次确认）
 * - 全局 toast 提示（ui.toast()）
 * - 主题同步到 Vuetify（须挂载在 v-app 内）
 *
 * 用法：在应用根视图挂载一次即可：<GlobalDialog />
 */
import { computed, watch } from 'vue'
import { useTheme } from 'vuetify'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()

/** 当前展示的弹层（队列首项） */
const current = computed(() => ui.dialogs[0] ?? null)

// ---- 主题同步：store 变化时应用到 Vuetify 全局主题 ----
// 必须挂载在 v-app 内，useTheme 才可用。
// 唯一主题出口：theme_mode（settings store）→ ui.theme → 此处写入 Vuetify。
// 注意：这里不做任何"手动锁定"处理——auto 模式由 settings 的 matchMedia 驱动，
// 手动模式由 setThemeMode 驱动，均已在 settings store 内决策完毕。
const vuetifyTheme = useTheme()

watch(
  () => ui.theme,
  (t) => {
    try {
      vuetifyTheme.global.name.value = t
    } catch {
      /* 忽略主题应用失败 */
    }
  },
  { immediate: true },
)
</script>

<template>
  <!-- 全局确认弹层：persistent，必须点击按钮关闭 -->
  <v-dialog
    :model-value="!!current"
    persistent
    max-width="420"
  >
    <v-card v-if="current" density="compact">
      <v-card-title class="text-subtitle-1 d-flex align-center">
        <v-icon
          :icon="current.danger ? 'mdi-alert-circle' : 'mdi-help-circle'"
          :color="current.danger ? 'error' : 'primary'"
          size="20"
          class="mr-2"
        />
        {{ current.title }}
      </v-card-title>
      <v-card-text class="text-body-2 text-medium-emphasis">{{ current.message }}</v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn size="small" variant="text" @click="ui.resolveDialog(current.id, false)">
          {{ current.cancelText }}
        </v-btn>
        <v-btn
          size="small"
          :color="current.danger ? 'error' : 'primary'"
          variant="flat"
          @click="ui.resolveDialog(current.id, true)"
        >
          {{ current.confirmText }}
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>

  <!-- 全局 toast：右下角堆叠，超时由 store 自动移除 -->
  <div class="global-toasts">
    <v-fab-transition group>
      <v-alert
        v-for="item in ui.toasts"
        :key="item.id"
        :color="item.color ?? 'surface'"
        :type="item.color === 'success' || item.color === 'error' || item.color === 'warning' || item.color === 'info' ? item.color : undefined"
        density="compact"
        variant="elevated"
        closable
        max-width="360"
        @click:close="ui.dismissToast(item.id)"
      >
        {{ item.message }}
      </v-alert>
    </v-fab-transition>
  </div>
</template>

<style scoped>
.global-toasts {
  position: fixed;
  right: 16px;
  bottom: 40px;
  z-index: 2100; /* 高于 v-dialog(2000+) 常规层级，保证弹层期间提示可见 */
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}
.global-toasts :deep(.v-alert) {
  pointer-events: auto;
}

/* rem 换算非整数档修复：text-body-2 12.25px → 12px 整数档 */
.text-body-2 {
  font-size: 12px !important;
}
</style>
