<script setup lang="ts">
/**
 * AddressBar —— 地址栏（Xshell 地址栏惯例：工具栏下独立一整行）
 *
 * 显示当前活动会话地址（user@host:port），可手动输入地址/会话名称回车连接，
 * 右端 ▼ 下拉列出全部会话可快速跳转。所有连接逻辑经事件抛给父级接线。
 */
import { computed, ref, watch } from 'vue'

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

// ---------------- ▼ 下拉会话筛选（会话多时定位用） ----------------

/** 下拉展开态（close-on-content-click=false，选中条目时手动关闭） */
const open = ref(false)
/** 筛选关键词（按会话标签包含匹配，不区分大小写） */
const filter = ref('')

/** 按关键词过滤后的会话列表 */
const filtered = computed(() => {
  const q = filter.value.trim().toLowerCase()
  if (!q) return props.sessions
  return props.sessions.filter((s) => s.label.toLowerCase().includes(q))
})

// 每次展开重置筛选，避免上次残留导致看似空列表
watch(open, (v) => {
  if (v) filter.value = ''
})

/** 选中条目：关闭下拉（清空筛选）并跳转 */
function goto(id: string): void {
  open.value = false
  filter.value = ''
  emit('goto-session', id)
}

/** 筛选框回车：跳转首个匹配会话 */
function gotoFilteredFirst(): void {
  const first = filtered.value[0]
  if (first) goto(first.id)
}

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
    <v-menu v-model="open" :close-on-content-click="false">
      <template #activator="{ props: act }">
        <button class="addressbar__drop" v-bind="act" title="选择会话">
          <v-icon icon="mdi-chevron-down" size="13" />
        </button>
      </template>
      <div class="addressbar__dropdown">
        <input
          v-model="filter"
          class="addressbar__filter"
          type="text"
          placeholder="筛选会话"
          aria-label="筛选会话"
          @keydown.enter="gotoFilteredFirst"
        />
        <v-list density="compact" max-height="320">
          <v-list-item
            v-for="s in filtered"
            :key="s.id"
            :value="s.id"
            @click="goto(s.id)"
          >
            <v-list-item-title>{{ s.label }}</v-list-item-title>
          </v-list-item>
          <v-list-item v-if="!filtered.length" disabled>
            <v-list-item-title>{{ filter ? '无匹配会话' : '暂无会话' }}</v-list-item-title>
          </v-list-item>
        </v-list>
      </div>
    </v-menu>
  </div>
</template>

<style scoped>
/* Xshell 地址栏：工具栏下独立一整行，横跨全宽，26px 与全局单行控件基线一致；
   背景透明（不占用独立色带，随窗口内容背景） */
.addressbar {
  display: flex;
  align-items: center;
  height: 26px;
  padding: 0 4px 0 8px;
  border-bottom: 1px solid var(--fy-chrome-border);
  background: transparent;
  user-select: none;
}

.addressbar__icon {
  flex: none;
  color: rgba(var(--v-theme-on-surface), 0.45);
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
  font-size: 12px;
}

.addressbar__input:focus-visible {
  outline: 1px solid rgb(var(--v-theme-primary));
  outline-offset: -1px;
  border-radius: 4px;
}

.addressbar__input::placeholder {
  color: rgba(var(--v-theme-on-surface), 0.4);
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
  color: rgba(var(--v-theme-on-surface), 0.45);
  background: transparent;
  border: none;
}

.addressbar__drop:hover {
  background: rgba(var(--v-theme-on-surface), 0.08);
}

/* 下拉会话列表：行距与 MenuBar 菜单一致（紧凑档） */
.addressbar :deep(.v-list-item) {
  min-height: 0;
  padding-top: 2px;
  padding-bottom: 2px;
}

.addressbar :deep(.v-list-item-title) {
  font-size: 12px;
  line-height: 1.3;
}

/* 下拉容器：筛选行 + 列表（v-menu content 自带 surface 背景与圆角阴影） */
.addressbar__dropdown {
  padding: 4px 0 2px;
}

/* 筛选框：与地址栏输入框同风格（透明底 + 底边框） */
.addressbar__filter {
  width: calc(100% - 12px);
  margin: 0 6px 4px;
  padding: 3px 6px;
  border: none;
  border-bottom: 1px solid var(--fy-chrome-border);
  outline: none;
  background: transparent;
  color: inherit;
  font-size: 12px;
}

.addressbar__filter:focus-visible {
  outline: none;
  border-bottom-color: rgb(var(--v-theme-primary));
}

.addressbar__filter::placeholder {
  color: rgba(var(--v-theme-on-surface), 0.4);
}
</style>
