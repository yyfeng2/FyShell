<template>
  <!-- 窗口：新建会话默认标签着色 -->
  <template v-if="page === 'window'">
    <div class="settings-dialog__section-title">窗口</div>
    <div class="settings-dialog__row">
      <div>
        <div class="settings-dialog__row-title">默认标签着色</div>
        <div class="settings-dialog__row-desc">新建 SSH 会话时预填的标签颜色；单个会话仍可在连接属性中修改。</div>
      </div>
      <div class="d-flex align-center">
        <v-color-picker
          :model-value="opts.defaultTabColor || undefined"
          mode="hex"
          elevation="8"
          width="220"
          @update:model-value="onColorChange"
        />
        <v-btn v-if="opts.defaultTabColor" size="small" variant="text" class="ml-2" @click="opts.setDefaultTabColor(null)">
          清除
        </v-btn>
      </div>
    </div>
  </template>

  <!-- 突出显示：关键词高亮规则 -->
  <template v-else-if="page === 'highlight'">
    <div class="settings-dialog__section-title">突出显示</div>
    <v-switch
      :model-value="opts.highlightEnabled"
      label="终端关键词高亮"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setHighlightEnabled(!!v)"
    />
    <div v-for="(rule, i) in opts.highlightRules" :key="i" class="settings-dialog__row">
      <div class="d-flex align-center">
        <span class="highlight-color-chip" :style="{ backgroundColor: rule.color }" />
        <code class="mr-4">{{ rule.keyword }}</code>
      </div>
      <v-btn
        icon="mdi-delete-outline"
        size="x-small"
        variant="text"
        title="删除规则"
        @click="removeRule(i)"
      />
    </div>
    <div class="d-flex align-center mt-2">
      <div class="fy-field-row">
        <span class="fy-field-row__label">关键词</span>
        <v-text-field
          v-model="newKeyword"
          density="compact"
          class="mr-2"
          hide-details
        />
      </div>
      <input
        v-model="newColor"
        type="color"
        class="highlight-color-input mr-2"
        title="高亮颜色"
      />
      <v-btn size="small" variant="tonal" prepend-icon="mdi-plus" @click="addRule">添加</v-btn>
    </div>
    <div class="settings-dialog__hint">对当前打开终端的输出实时高亮（新开终端同样生效）。</div>
  </template>
</template>

<script setup lang="ts">
/**
 * AppearancePanels —— 窗口 / 突出显示（SSH 选项）
 */
import { ref } from 'vue'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useUiStore } from '@/stores/ui'

/** 展示页（window | highlight） */
defineProps<{ page: string }>()

const opts = useSshOptionsStore()
const ui = useUiStore()

const newColor = ref('#ffd54f')
const newKeyword = ref('')

function onColorChange(value: string | Record<string, number>): void {
  if (typeof value === 'string') {
    opts.setDefaultTabColor(value)
  } else if (value && typeof value.r === 'number') {
    const hex =
      '#' + [value.r, value.g, value.b].map((c) => Math.round(c).toString(16).padStart(2, '0')).join('')
    opts.setDefaultTabColor(hex)
  }
}

function addRule(): void {
  const keyword = newKeyword.value.trim()
  if (!keyword) return
  if (opts.highlightRules.some((r) => r.keyword === keyword)) {
    ui.toast('该关键词已存在', 'error')
    return
  }
  opts.setHighlightRules([...opts.highlightRules, { keyword, color: newColor.value }])
  newKeyword.value = ''
}

function removeRule(index: number): void {
  const next = opts.highlightRules.filter((_, i) => i !== index)
  opts.setHighlightRules(next)
}
</script>

<style scoped>
.highlight-color-chip {
  display: inline-block;
  width: 14px;
  height: 14px;
  border-radius: 4px;
  margin-right: 8px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.15);
}

.highlight-color-input {
  width: 44px;
  height: 36px;
  padding: 2px;
  border: none;
  background: transparent;
  cursor: pointer;
}
</style>
