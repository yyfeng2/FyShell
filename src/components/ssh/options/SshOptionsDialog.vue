<template>
  <v-dialog
    :model-value="modelValue"
    max-width="760"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-tune-vertical" size="small" class="mr-2" />
        {{ isSessionMode ? `会话选项 - ${sessionName}` : 'SSH 选项' }}
      <v-spacer />
      <v-btn
        icon="mdi-close"
        size="x-small"
        variant="text"
        title="关闭"
        @click="emit('update:modelValue', false)"
      />
      </v-card-title>
      <v-divider />
      <div class="ssh-options settings-dialog__body">
        <!-- 左侧树形导航（参考 SecureCRT「会话选项」布局） -->
        <nav class="ssh-options__nav" aria-label="SSH 选项分类">
          <template v-for="group in navGroups" :key="group.key">
            <div class="ssh-options__nav-group">{{ group.title }}</div>
            <button
              v-for="leaf in group.children"
              :key="leaf.key"
              type="button"
              class="ssh-options__leaf"
              :class="{ 'ssh-options__leaf--active': current === leaf.key }"
              @click="current = leaf.key"
            >
              <v-icon :icon="leaf.icon" size="13" class="mr-2" />
              {{ leaf.title }}
            </button>
          </template>
        </nav>
        <v-divider vertical />
        <!-- 右侧内容区 -->
        <div class="settings-dialog__content">
          <ConnectionAuthPanels v-if="current === 'auth' || current === 'login-prompt'" :page="current" />
          <LoginScriptPanel v-else-if="current === 'login-script'" />
          <SshPanels v-else-if="isSshPanel(current)" :page="current" />
          <ProxyPanel v-else-if="current === 'proxy'" />
          <KeepAlivePanel v-else-if="current === 'keepalive'" />
          <TerminalPanels v-else-if="isTerminalPanel(current)" :page="current" />
          <AppearancePanels v-else-if="isAppearancePanel(current)" :page="current" />
          <AdvancedPanels v-else-if="isAdvancedPanel(current)" :page="current" />
        </div>
      </div>
      <v-divider />
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">关闭</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * SshOptionsDialog —— SSH 专属选项对话框
 *
 * 左侧两级树形导航 + 右侧面板（1:1 对齐 SecureCRT「会话选项」布局）；
 * 分区见 types.ts 的 SSH_OPTIONS_NAV。
 * 全局模式（无 sessionId）：对所有 SSH 连接生效；
 * 会话模式（传入 sessionId）：标题显示会话名，终端外观/行为类选项编辑写会话级键
 * （sshopt_session_{session_id}_{key}），显示会话级值优先、回退全局值。
 * 所有选项变更即写回后端 SQLite（settings 表），重启后仍保留。
 */
import { computed, ref, watch } from 'vue'
import {
  SSH_OPTIONS_NAV,
  type SshOptionsLeaf,
} from './types'
import ConnectionAuthPanels from './panels/ConnectionAuthPanels.vue'
import LoginScriptPanel from './panels/LoginScriptPanel.vue'
import SshPanels from './panels/SshPanels.vue'
import ProxyPanel from './panels/ProxyPanel.vue'
import KeepAlivePanel from './panels/KeepAlivePanel.vue'
import TerminalPanels from './panels/TerminalPanels.vue'
import AppearancePanels from './panels/AppearancePanels.vue'
import AdvancedPanels from './panels/AdvancedPanels.vue'
import { useSshOptionsStore } from '@/stores/sshOptions'

const props = defineProps<{
  modelValue: boolean
  /** 打开时默认展示的叶子页（缺省为用户身份验证） */
  initialLeaf?: SshOptionsLeaf
  /** 会话模式：会话节点 id（SSH 会话 UUID）；缺省为全局模式 */
  sessionId?: string
  /** 会话模式：会话名（标题展示用） */
  sessionName?: string
  /** 本地终端模式：左导航过滤掉 SSH 连接组（本地终端无 SSH 连接层，只留终端/外观组） */
  terminalOnly?: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
}>()

const opts = useSshOptionsStore()

// 启动时从后端加载 SSH 选项（幂等）
void opts.ensureLoaded()

/** 是否处于会话模式（传入 sessionId） */
const isSessionMode = computed(() => !!props.sessionId)

/** 左导航组（本地终端模式过滤 SSH 连接组，只留终端/外观组） */
const navGroups = computed(() =>
  props.terminalOnly
    ? SSH_OPTIONS_NAV.filter((g) => g.key === 'terminal' || g.key === 'appearance')
    : SSH_OPTIONS_NAV,
)

/** 导航内可用的叶子集合（terminalOnly 下连接组叶子不可用） */
const availableLeaves = computed(
  () => new Set(navGroups.value.flatMap((g) => g.children.map((l) => l.key))),
)

/** 当前展示的叶子页 */
const current = ref<SshOptionsLeaf>('auth')

/** 打开/关闭对话框：会话模式设置 store 会话上下文并加载会话覆盖项，关闭时恢复全局 */
watch(
  () => props.modelValue,
  (visible) => {
    if (visible) {
      // terminalOnly 下连接组叶子不在导航内，回退到终端组首项（键盘）
      const fallback: SshOptionsLeaf = props.terminalOnly ? 'keyboard' : 'auth'
      const wanted = props.initialLeaf ?? fallback
      current.value = availableLeaves.value.has(wanted) ? wanted : fallback
      if (props.sessionId) {
        opts.dialogSessionId = props.sessionId
        void opts.loadSessionOverrides(props.sessionId)
      } else {
        opts.dialogSessionId = null
      }
    } else {
      opts.dialogSessionId = null
    }
  },
)

// 面板分组归属判断（面板组件按组拆分，page prop 切换叶子）
function isSshPanel(leaf: SshOptionsLeaf): boolean {
  return leaf === 'ssh-security' || leaf === 'ssh-tunnel' || leaf === 'ssh-sftp'
}
function isTerminalPanel(leaf: SshOptionsLeaf): boolean {
  return leaf === 'keyboard' || leaf === 'vt-mode' || leaf === 'term-advanced'
}
function isAppearancePanel(leaf: SshOptionsLeaf): boolean {
  return leaf === 'window' || leaf === 'highlight'
}
function isAdvancedPanel(leaf: SshOptionsLeaf): boolean {
  return leaf === 'trace' || leaf === 'bell' || leaf === 'logging'
}
</script>

<style scoped>
/* 左侧树形导航（组标题 + 缩进叶子，复用 SettingsDialog 布局参数） */
.ssh-options__nav {
  display: flex;
  flex-direction: column;
  flex: 0 0 160px;
  gap: 2px;
  padding: 8px 6px;
  background: var(--fy-chrome-bg);
  overflow-y: auto;
}

.ssh-options__nav-group {
  padding: 6px 10px 3px;
  font-size: 13px;
  font-weight: 400;
  color: rgb(var(--v-theme-on-surface) / 0.85);
  user-select: none;
}

.ssh-options__leaf {
  display: flex;
  align-items: center;
  padding: 4px 10px 5px 24px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 13px;
  line-height: 1.4;
  cursor: pointer;
  text-align: left;
  user-select: none;
}

.ssh-options__leaf:hover {
  background: rgb(var(--v-theme-on-surface) / 0.08);
}

.ssh-options__leaf--active {
  background: rgb(var(--v-theme-primary) / 0.15);
  color: rgb(var(--v-theme-primary));
}

.ssh-options__leaf--active .mr-2 {
  color: rgb(var(--v-theme-primary));
}
</style>

<!-- 面板共享样式：非 scoped（panels 内的 settings-dialog__* 类名与 SettingsDialog 同名同值，注入一次即可） -->
<style>
.ssh-options .settings-dialog__section-title {
  font-size: 14px;
  font-weight: 400;
  margin-bottom: 8px;
}

.ssh-options .settings-dialog__field {
  max-width: 420px;
}

.ssh-options .settings-dialog__hint {
  margin-top: 8px;
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

.ssh-options .settings-dialog__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 0;
}

.ssh-options .settings-dialog__row + .settings-dialog__row {
  border-top: 1px solid var(--fy-chrome-border);
}

.ssh-options .settings-dialog__row-title {
  font-size: 14px;
  font-weight: 400;
}

.ssh-options .settings-dialog__row-desc {
  font-size: 12px;
  color: rgb(var(--v-theme-on-surface) / 0.55);
  margin-top: 2px;
}

.ssh-options .settings-dialog__field-row {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 420px;
  flex: 1 1 auto;
}

/* 右侧内容区（与 SettingsDialog 布局一致） */
.ssh-options.settings-dialog__body {
  display: flex;
  height: 420px;
}

.ssh-options .settings-dialog__content {
  flex: 1 1 auto;
  min-width: 0;
  padding: 12px 16px;
  overflow-y: auto;
}
</style>
