<script setup lang="ts">
/**
 * AddressBar —— 地址栏（Xshell 地址栏惯例：工具栏下独立一整行）
 *
 * 显示当前活动会话地址（user@host:port），可手动输入地址/会话名称回车连接，
 * 右端 ▼ 下拉列出全部会话可快速跳转。所有连接逻辑经事件抛给父级接线。
 */
import { ref, watch } from 'vue'

/** 地址栏会话条目（id 为树节点 id，父级经节点连接路由跳转） */
interface AddressBarSession {
  id: string
  label: string
}

const props = defineProps<{
  /** 当前活动标签的地址文本（会话显示 user@host:port，其他显示标签标题） */
  address: string
  /** 可快速跳转的会话列表（全量持久会话，不受搜索/折叠影响） */
  sessions: AddressBarSession[]
}>()

const emit = defineEmits<{
  /** 回车连接：输入命中已有会话直接连，否则父级打开会话表单预填主机 */
  (e: 'connect-address', addr: string): void
  (e: 'goto-session', id: string): void
}>()

const input = ref('')
/** 用户手动输入中（dirty）：活动标签切换时不覆盖输入内容 */
const dirty = ref(false)

// 活动会话地址变化（切换标签/连接成功）且用户非输入中时，同步显示；
// immediate：挂载时已有活动标签则立即显示地址
watch(
  () => props.address,
  (v) => {
    if (!dirty.value) input.value = v
  },
  { immediate: true },
)

function submit(): void {
  const v = input.value.trim()
  if (!v) return
  emit('connect-address', v)
  // 重置输入态：连接后地址栏跟随活动会话地址（连接失败时保留用户输入重试）
  dirty.value = false
}
</script>

<template>
  <div class="addressbar">
    <v-icon icon="mdi-folder-open" size="13" class="addressbar__icon" />
    <input
      v-model="input"
      class="addressbar__input"
      type="text"
      placeholder="输入主机地址或会话名称，回车连接"
      aria-label="地址栏：输入主机地址或会话名称，回车连接"
      @input="dirty = true"
      @blur="dirty = false"
      @keydown.enter="submit"
    />
    <v-menu :close-on-content-click="true">
      <template #activator="{ props: act }">
        <button class="addressbar__drop" v-bind="act" title="选择会话">
          <v-icon icon="mdi-chevron-down" size="13" />
        </button>
      </template>
      <v-list density="compact" max-height="320">
        <v-list-item
          v-for="s in sessions"
          :key="s.id"
          :value="s.id"
          @click="emit('goto-session', s.id)"
        >
          <v-list-item-title>{{ s.label }}</v-list-item-title>
        </v-list-item>
        <v-list-item v-if="!sessions.length" disabled>
          <v-list-item-title>暂无会话</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>
  </div>
</template>

<style scoped>
/* Xshell 地址栏：工具栏下独立一整行，横跨全宽，26px 与全局单行控件基线一致 */
.addressbar {
  display: flex;
  align-items: center;
  height: 26px;
  padding: 0 4px 0 8px;
  border-bottom: 1px solid var(--fy-chrome-border);
  background: var(--fy-chrome-bg);
  user-select: none;
}

.addressbar__icon {
  flex: none;
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

/* 输入框：占满行内剩余宽度（Xshell 地址栏横跨全宽） */
.addressbar__input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  padding: 0 6px;
  border: none;
  outline: none;
  background: transparent;
  color: inherit;
  font-size: 13px;
}

.addressbar__input:focus-visible {
  outline: 1px solid rgb(var(--v-theme-primary));
  outline-offset: -1px;
  border-radius: 4px;
}

.addressbar__input::placeholder {
  color: rgb(var(--v-theme-on-surface) / 0.4);
}

/* ▼ 下拉按钮：行右端 */
.addressbar__drop {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 4px;
  cursor: pointer;
  color: rgb(var(--v-theme-on-surface) / 0.45);
  background: transparent;
  border: none;
}

.addressbar__drop:hover {
  background: rgb(var(--v-theme-on-surface) / 0.08);
}

/* 下拉会话列表：行距与 MenuBar 菜单一致（紧凑档） */
.addressbar :deep(.v-list-item) {
  min-height: 0;
  padding-top: 2px;
  padding-bottom: 2px;
}

.addressbar :deep(.v-list-item-title) {
  font-size: 13px;
  line-height: 1.3;
}
</style>
