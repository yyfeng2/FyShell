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
        <span class="ssh-options__subtitle ml-2">
          {{ isSessionMode ? '设置仅对当前会话生效（覆盖全局值）' : '设置对所有 SSH 连接全局生效' }}
        </span>
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
      <!-- 会话模式生效范围提示 -->
      <div v-if="isSessionMode" class="ssh-options__session-hint">
        <v-icon icon="mdi-information-outline" size="13" class="mr-1" />
        <template v-if="byteType">
          {{ byteType === 'serial' ? '串口' : byteType === 'rlogin' ? 'RLOGIN' : 'Telnet' }} 会话「{{ sessionName }}」：
          连接面板编辑会话字段（保存后刷新会话树）；终端 / 外观 / 高级按会话覆盖全局值。
        </template>
        <template v-else>
          当前编辑会话「{{ sessionName }}」：响铃 / 关键词高亮 / 登录提示符自动响应按会话覆盖全局值；
          连接级选项（认证 / 压缩 / 代理等）当前仅全局生效。
        </template>
      </div>
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
              <v-icon :icon="leaf.icon" size="14" class="mr-2" />
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
          <!-- byte-stream 连接设置：会话模式且为本类会话时渲染真实编辑面板；否则显示新建引导 -->
          <template v-else-if="isByteStreamLeaf(current)">
            <ByteStreamPanel
              v-if="byteType === current"
              :type="current"
              :session-id="props.sessionId ?? ''"
              @saved="onByteStreamSaved"
            />
            <div v-else class="ssh-options__placeholder">
              <v-icon icon="mdi-lan-disconnect" size="36" class="mb-2" />
              <div class="ssh-options__placeholder-title">{{ byteStreamGuide(current).title }}</div>
              <div class="settings-dialog__hint">{{ byteStreamGuide(current).hint }}</div>
            </div>
          </template>
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
  type SshOptionsNavItem,
} from './types'
import ConnectionAuthPanels from './panels/ConnectionAuthPanels.vue'
import LoginScriptPanel from './panels/LoginScriptPanel.vue'
import SshPanels from './panels/SshPanels.vue'
import ProxyPanel from './panels/ProxyPanel.vue'
import KeepAlivePanel from './panels/KeepAlivePanel.vue'
import TerminalPanels from './panels/TerminalPanels.vue'
import AppearancePanels from './panels/AppearancePanels.vue'
import AdvancedPanels from './panels/AdvancedPanels.vue'
import ByteStreamPanel from './panels/ByteStreamPanel.vue'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useSessionStore } from '@/stores/session'

const props = defineProps<{
  modelValue: boolean
  /** 打开时默认展示的叶子页（缺省为用户身份验证） */
  initialLeaf?: SshOptionsLeaf
  /** 会话模式：会话节点 id（SSH 会话 UUID）；缺省为全局模式 */
  sessionId?: string
  /** 会话模式：会话名（标题展示用） */
  sessionName?: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 会话字段变更保存成功（父级刷新会话树） */
  (e: 'changed'): void
}>()

const opts = useSshOptionsStore()
const sessionStore = useSessionStore()

// 启动时从后端加载 SSH 选项（幂等）
void opts.ensureLoaded()

/** 是否处于会话模式（传入 sessionId） */
const isSessionMode = computed(() => !!props.sessionId)

/** 当前展示的叶子页 */
const current = ref<SshOptionsLeaf>('auth')

/** 会话模式：当前会话的 SessionConfig（查找不到为 null） */
const sessionCfg = computed(() =>
  props.sessionId ? sessionStore.getSessionById(props.sessionId) ?? null : null,
)

/** 会话为 byte-stream 类型（telnet/rlogin/serial）时的连接类型；否则 null（SSH/MySQL 会话或全局模式） */
const byteType = computed<'telnet' | 'rlogin' | 'serial' | null>(() => {
  const t = sessionCfg.value?.session_type
  return t === 'telnet' || t === 'rlogin' || t === 'serial' ? t : null
})

/** 左侧导航：会话模式且为 byte-stream 会话时，仅保留该连接叶子 + 共享终端/外观/高级组 */
const navGroups = computed<SshOptionsNavItem[]>(() => {
  const type = byteType.value
  if (!type) return SSH_OPTIONS_NAV
  return SSH_OPTIONS_NAV.map((g) => {
    if (g.key !== 'connection') return g
    const leaf = g.children.find((l) => l.key === type)
    return leaf ? { ...g, children: [leaf] } : null
  }).filter((g): g is SshOptionsNavItem => g !== null)
})

/** 打开/关闭对话框：会话模式设置 store 会话上下文并加载会话覆盖项，关闭时恢复全局 */
watch(
  () => props.modelValue,
  (visible) => {
    if (visible) {
      current.value = props.initialLeaf ?? byteType.value ?? 'auth'
      if (props.sessionId) {
        opts.dialogSessionId = props.sessionId
        void opts.loadSessionOverrides(props.sessionId)
        // 载入会话树（幂等；保证 byteType/会话字段来自后端最新值）
        void sessionStore.load()
      } else {
        opts.dialogSessionId = null
      }
    } else {
      opts.dialogSessionId = null
    }
  },
)

// 会话树载入为异步：字节流会话的类型确认后，若仍停留在默认页则跳到对应连接叶子
watch(byteType, (t) => {
  if (t && props.modelValue && current.value === 'auth') {
    current.value = t
  }
})

/** byte-stream 叶子页（telnet/rlogin/serial） */
function isByteStreamLeaf(leaf: SshOptionsLeaf): boolean {
  return leaf === 'telnet' || leaf === 'rlogin' || leaf === 'serial'
}

/** byte-stream 引导面板（全局模式 / 非本类会话）说明文案 */
function byteStreamGuide(leaf: SshOptionsLeaf): { title: string; hint: string } {
  const map: Record<string, { title: string; hint: string }> = {
    telnet: {
      title: 'Telnet 连接参数为会话级',
      hint: '请在「新建会话」中选择 Telnet 类型创建，再从会话树右键的「设置」中编辑连接参数。',
    },
    rlogin: {
      title: 'RLOGIN 连接参数为会话级',
      hint: 'RLOGIN 与 Telnet 协议兼容（复用 Telnet 实现）；请新建 RLOGIN 会话后从会话树右键「设置」编辑。',
    },
    serial: {
      title: '串口连接参数为会话级',
      hint: '请在「新建会话」中选择串口类型创建，再从会话树右键的「设置」中编辑端口与波特率。',
    },
  }
  return map[leaf] ?? map.telnet!
}

function onByteStreamSaved(): void {
  emit('changed')
}

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
  flex: 0 0 172px;
  gap: 2px;
  padding: 10px 6px;
  background: var(--fy-chrome-bg);
  overflow-y: auto;
}

.ssh-options__nav-group {
  padding: 8px 10px 4px;
  font-size: 14px;
  font-weight: 400;
  color: rgb(var(--v-theme-on-surface) / 0.85);
  user-select: none;
}

.ssh-options__leaf {
  display: flex;
  align-items: center;
  padding: 5px 10px 6px 26px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 14px;
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

/* 占位页（TELNET/RLOGIN/串口） */
.ssh-options__placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 60px 20px;
  color: rgb(var(--v-theme-on-surface) / 0.4);
  text-align: center;
}

/* 副标题 */
.ssh-options__subtitle {
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.5);
}

/* 会话模式生效范围提示条 */
.ssh-options__session-hint {
  display: flex;
  align-items: center;
  padding: 6px 16px;
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.6);
  background: rgb(var(--v-theme-primary) / 0.06);
}
</style>

<!-- 面板共享样式：非 scoped（panels 内的 settings-dialog__* 类名与 SettingsDialog 同名同值，注入一次即可） -->
<style>
.ssh-options .settings-dialog__section-title {
  font-size: 14px;
  font-weight: 400;
  margin-bottom: 12px;
}

.ssh-options .settings-dialog__field {
  max-width: 420px;
}

.ssh-options .settings-dialog__hint {
  margin-top: 10px;
  font-size: 14px;
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

.ssh-options .settings-dialog__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
}

.ssh-options .settings-dialog__row + .settings-dialog__row {
  border-top: 1px solid var(--fy-chrome-border);
}

.ssh-options .settings-dialog__row-title {
  font-size: 14px;
  font-weight: 400;
}

.ssh-options .settings-dialog__row-desc {
  font-size: 14px;
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
  padding: 14px 18px;
  overflow-y: auto;
}
</style>
