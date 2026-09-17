<script setup lang="ts">
/**
 * SplitLayout —— 通用分屏容器组件
 *
 * 特性：
 * - 支持 'row'（左右两栏，分隔条为竖线）/ 'column'（上下两栏，分隔条为横线）分割
 * - 分隔条可拖动调整两栏比例，双击恢复 50/50
 * - 可嵌套：slot 内再放 SplitLayout 即可（终端分屏 / SFTP 双栏复用同一组件）
 *
 * 用法：
 * <SplitLayout v-model="ratio" direction="row">
 *   <template #first>...</template>
 *   <template #second>...</template>
 * </SplitLayout>
 */
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 分割方向：'row' = 左右两栏；'column' = 上下两栏 */
    direction?: 'row' | 'column'
    /** 第一栏占比（0-100，v-model） */
    modelValue?: number
    /** 第一栏最小占比 */
    min?: number
    /** 第一栏最大占比 */
    max?: number
  }>(),
  { direction: 'row', modelValue: 50, min: 15, max: 85 },
)

const emit = defineEmits<{
  (e: 'update:modelValue', ratio: number): void
}>()

const container = ref<HTMLElement | null>(null)
const dragging = ref(false)

/** 指针捕获失败的降级：改为 window 级监听 move/up/cancel（光标离开分隔条仍能继续拖动） */
function addWindowTrack(): void {
  window.addEventListener('pointermove', onDividerMove)
  window.addEventListener('pointerup', onDividerUp)
  window.addEventListener('pointercancel', onDividerUp)
}

function removeWindowTrack(): void {
  window.removeEventListener('pointermove', onDividerMove)
  window.removeEventListener('pointerup', onDividerUp)
  window.removeEventListener('pointercancel', onDividerUp)
}

/** 按下分隔条：捕获指针，开始拖动 */
function onDividerDown(e: PointerEvent): void {
  dragging.value = true
  let captured = false
  try {
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    captured = true
  } catch {
    /* 某些环境不支持指针捕获 */
  }
  // 捕获失败则降级为 window 级跟踪；释放失败时同步复位 dragging 状态
  if (!captured) addWindowTrack()
}

/** 拖动中：按指针位置换算第一栏占比 */
function onDividerMove(e: PointerEvent): void {
  if (!dragging.value || !container.value) return
  const rect = container.value.getBoundingClientRect()
  const ratio =
    props.direction === 'row'
      ? ((e.clientX - rect.left) / rect.width) * 100
      : ((e.clientY - rect.top) / rect.height) * 100
  const clamped = Math.min(props.max, Math.max(props.min, ratio))
  emit('update:modelValue', clamped)
}

/** 松开/取消：结束拖动并移除 window 级监听 */
function onDividerUp(e: PointerEvent): void {
  if (!dragging.value) return
  dragging.value = false
  removeWindowTrack()
  try {
    ;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
  } catch {
    /* 忽略 */
  }
}

/** 双击分隔条：恢复 50/50（并 clamp 到 min/max 范围） */
function onDividerDblClick(): void {
  emit('update:modelValue', Math.min(props.max, Math.max(props.min, 50)))
}
</script>

<template>
  <div
    ref="container"
    class="split-layout"
    :class="[`split-layout--${direction}`, { 'split-layout--dragging': dragging }]"
  >
    <div class="split-layout__pane" :style="{ flexBasis: `${modelValue}%` }">
      <slot name="first" />
    </div>
    <div
      class="split-layout__divider"
      :title="`拖动调整比例（双击恢复 50/50）`"
      @pointerdown="onDividerDown"
      @pointermove="onDividerMove"
      @pointerup="onDividerUp"
      @pointercancel="onDividerUp"
      @dblclick="onDividerDblClick"
    />
    <div class="split-layout__pane" :style="{ flexBasis: `${100 - modelValue}%` }">
      <slot name="second" />
    </div>
  </div>
</template>

<style scoped>
.split-layout {
  display: flex;
  width: 100%;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.split-layout--row {
  flex-direction: row;
}

.split-layout--column {
  flex-direction: column;
}

.split-layout__pane {
  flex: 1 1 50%;
  min-width: 0; /* 允许内容收缩，防止撑破布局 */
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.split-layout__divider {
  flex: 0 0 5px;
  background: rgb(var(--v-theme-surface-variant, 32 33 35) / 0.35);
  transition: background 0.12s;
  touch-action: none; /* 交给指针事件处理 */
  z-index: 1;
}

.split-layout--row .split-layout__divider {
  cursor: col-resize;
}

.split-layout--column .split-layout__divider {
  cursor: row-resize;
}

.split-layout__divider:hover,
.split-layout--dragging .split-layout__divider {
  background: rgb(var(--v-theme-primary, 82 132 255));
}

/* 拖动期间全局禁止选中文本，避免选中高亮干扰 */
.split-layout--dragging {
  user-select: none;
}
</style>
