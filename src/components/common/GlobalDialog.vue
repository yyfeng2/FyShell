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
import { computed, ref, watch } from 'vue'
import { useTheme } from 'vuetify'
import { useUiStore } from '@/stores/ui'

const ui = useUiStore()

/** 当前展示的弹层（队列首项） */
const current = computed(() => ui.dialogs[0] ?? null)

// ---- 确认弹层：默认聚焦"确认"按钮（Enter 快捷确认，防误点习惯位"取消"） ----
const confirmBtnRef = ref<HTMLButtonElement | null>(null)

/** 聚焦确认按钮：Vuetify 过渡（≈225ms）期间焦点被 .v-overlay__content 抢占，且 focus trap 在过渡
   结束后才释放——释放时刻无法精确预测。用轻量轮询持续尝试（100ms 步进，成功即停，1.2s 兜底防死循环）。
   注意：按钮必须实时 querySelector 取（watch flush:'pre' 时 ref 仍指向上一轮旧按钮，detached focus 无效）。 */
function focusConfirm(): void {
  let tries = 0
  const timer = window.setInterval(() => {
    const btn =
      document.querySelector<HTMLButtonElement>('.v-dialog .v-card-actions button:last-child') ??
      confirmBtnRef.value
    if (btn && document.activeElement !== btn) btn.focus()
    tries += 1
    if (document.activeElement === btn || tries >= 12) window.clearInterval(timer)
  }, 100)
}

watch(current, (d) => {
  if (d) focusConfirm()
})

/** Enter 快捷确认（表单场景回车直接确认） */
function onConfirmKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter' && current.value) {
    e.preventDefault()
    ui.resolveDialog(current.value.id, true)
  }
}

// ---- Toast：语义色前缀图标 + detail 折叠展示（可复制原文） ----
/** color → 前置图标（仅在语义色上装备；普通 surface toast 不加图标保持干净） */
function toastIcon(color?: string): string | undefined {
  if (color === 'error') return 'mdi-alert-circle'
  if (color === 'warning') return 'mdi-alert-triangle'
  if (color === 'success') return 'mdi-check-circle'
  if (color === 'info') return 'mdi-information'
  return undefined
}

/** 已展开详情原文的 toast id 集合 */
const expandedToasts = ref(new Set<number>())

function toggleToastDetail(id: number): void {
  const next = new Set(expandedToasts.value)
  if (next.has(id)) {
    next.delete(id)
  } else {
    next.add(id)
  }
  expandedToasts.value = next
}

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
  <!-- 全局确认弹层：persistent，必须点击按钮关闭；Enter 快捷确认，确认按钮默认聚焦 -->
  <v-dialog
    :model-value="!!current"
    persistent
    max-width="420"
    @keydown.enter="onConfirmKeydown"
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
      <v-card-text class="global-dialog__message text-body-2 text-medium-emphasis">{{ current.message }}</v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn size="small" variant="text" @click="ui.resolveDialog(current.id, false)">
          {{ current.cancelText }}
        </v-btn>
        <v-btn
          ref="confirmBtnRef"
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

  <!-- 全局 toast：右下角堆叠，超时由 store 自动移除；语义色前缀图标 + detail 折叠展示 -->
  <div class="global-toasts">
    <v-fab-transition group>
      <v-alert
        v-for="item in ui.toasts"
        :key="item.id"
        :color="item.color ?? 'surface'"
        :type="item.color === 'success' || item.color === 'error' || item.color === 'warning' || item.color === 'info' ? item.color : undefined"
        :icon="toastIcon(item.color)"
        density="compact"
        variant="elevated"
        closable
        max-width="360"
        @click:close="ui.dismissToast(item.id)"
      >
        <div>{{ item.message }}</div>
        <!-- 原文细节：折叠"查看详情"，展开后可复制（批3） -->
        <template v-if="item.detail">
          <div class="global-toast__detail-toggle" @click="toggleToastDetail(item.id)">
            {{ expandedToasts.has(item.id) ? '收起详情' : '查看详情' }}
          </div>
          <v-expand-transition>
            <div v-show="expandedToasts.has(item.id)" class="global-toast__detail">
              {{ item.detail }}
            </div>
          </v-expand-transition>
        </template>
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


/* 超长 message（如含长路径/堆栈的错误串）限高滚动，保证底部按钮可见 */
.global-dialog__message {
  max-height: 50vh;
  overflow-y: auto;
}

/* toast 原文详情：折叠入口 + 展开原文（可复制，限高滚动防长串撑爆） */
.global-toast__detail-toggle {
  margin-top: 4px;
  font-size: 12px;
  color: rgb(var(--v-theme-primary));
  cursor: pointer;
  user-select: none;
}

.global-toast__detail {
  margin-top: 6px;
  padding: 6px 8px;
  border-radius: 4px;
  background: var(--fy-hover-bg);
  font-size: 12px;
  font-family: var(--fy-mono);
  line-height: 1.5;
  user-select: text; /* 允许选中复制原文 */
  word-break: break-all;
  max-height: 120px;
  overflow-y: auto;
}
</style>
