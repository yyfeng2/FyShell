<template>
  <v-dialog
    :model-value="modelValue"
    max-width="760"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-cog-outline" size="small" class="mr-2" />
        设置
      </v-card-title>
      <v-divider />
      <div class="settings-dialog__body">
        <!-- 左侧分类导航（参考 Navicat/HexHub 设置布局） -->
        <nav class="settings-dialog__nav" aria-label="设置分类">
          <button
            v-for="s in SECTIONS"
            :key="s.key"
            type="button"
            class="settings-dialog__nav-item"
            :class="{ 'settings-dialog__nav-item--active': section === s.key }"
            @click="section = s.key"
          >
            <v-icon :icon="s.icon" size="14" class="mr-2" />
            {{ s.title }}
          </button>
        </nav>
        <v-divider vertical />
        <!-- 右侧内容区 -->
        <div class="settings-dialog__content">
          <!-- 外观 -->
          <template v-if="section === 'appearance'">
            <div class="settings-dialog__section-title">外观</div>
            <v-select
              :model-value="settings.themeMode"
              :items="THEME_MODE_ITEMS"
              item-title="title"
              item-value="value"
              label="主题模式"
              class="settings-dialog__field"
              @update:model-value="(v: unknown) => settings.setThemeMode(v as ThemeMode)"
            />
            <div class="settings-dialog__hint">
              「跟随系统」监听系统深浅色偏好并实时切换；手动选择浅色/深色后固定主题。
            </div>
          </template>

          <!-- 终端 -->
          <template v-else-if="section === 'terminal'">
            <div class="settings-dialog__section-title">终端</div>
            <v-text-field
              :model-value="settings.terminalFontSize"
              label="默认字体大小（px）"
              type="number"
              min="6"
              max="72"
              density="compact"
              class="settings-dialog__field"
              @change="onFontSizeChange"
            />
            <v-text-field
              :model-value="settings.terminalFontFamily"
              label="字体家族"
              density="compact"
              class="settings-dialog__field"
              placeholder="&quot;Cascadia Mono&quot;, Consolas, monospace"
              @change="onFontFamilyChange"
            />
            <v-text-field
              :model-value="settings.terminalScrollback"
              label="滚动缓冲行数"
              type="number"
              min="100"
              max="1000000"
              density="compact"
              class="settings-dialog__field"
              @change="onScrollbackChange"
            />
            <v-switch
              :model-value="settings.terminalCursorBlink"
              label="光标闪烁"
              color="primary"
              density="compact"
              hide-details
              @update:model-value="(v: unknown) => settings.setTerminalCursorBlink(!!v)"
            />
            <div class="settings-dialog__hint">
              变更实时应用到已打开的终端（字号变化会联动重排），并对新终端生效。
            </div>
          </template>

          <!-- SFTP -->
          <template v-else-if="section === 'sftp'">
            <div class="settings-dialog__section-title">SFTP</div>
            <div class="settings-dialog__row">
              <div class="settings-dialog__field-row">
                <v-text-field
                  :model-value="settings.sftpDownloadDir"
                  label="默认下载目录"
                  density="compact"
                  placeholder="留空使用系统下载目录"
                  hide-details
                  @change="onDownloadDirChange"
                />
                <v-btn
                  size="small"
                  variant="tonal"
                  prepend-icon="mdi-folder-open"
                  title="选择目录"
                  @click="pickDownloadDir"
                >
                  浏览
                </v-btn>
              </div>
              <div class="settings-dialog__hint">SFTP 下载时默认保存到该目录。</div>
            </div>
          </template>

          <!-- 数据 -->
          <template v-else-if="section === 'data'">
            <div class="settings-dialog__section-title">数据</div>
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">清除查询历史</div>
                <div class="settings-dialog__row-desc">
                  删除 SQL 控制台保存的全部查询历史记录，不可恢复。
                </div>
              </div>
              <v-btn size="small" color="error" variant="tonal" @click="clearQueryHistory">
                清除
              </v-btn>
            </div>
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">清除无效数据</div>
                <div class="settings-dialog__row-desc">
                  清空已完成 / 失败 / 已取消的传输记录等临时缓存（进行中的任务不受影响）。
                </div>
              </div>
              <v-btn size="small" color="error" variant="tonal" @click="clearInvalidData">
                清除
              </v-btn>
            </div>
          </template>

          <!-- 安全（主密码快捷入口） -->
          <template v-else-if="section === 'security'">
            <div class="settings-dialog__section-title">安全</div>
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">主密码</div>
                <div class="settings-dialog__row-desc">
                  用于保护本地存储的敏感凭据（凭据仅存 Rust 侧 SQLite，不进 WebView）。
                </div>
              </div>
              <v-btn size="small" color="primary" variant="tonal" @click="emit('open-master-password')">
                主密码设置
              </v-btn>
            </div>
          </template>

          <!-- 关于 -->
          <template v-else-if="section === 'about'">
            <div class="settings-dialog__section-title">关于</div>
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">应用版本</div>
                <div class="settings-dialog__row-desc">FyShell v{{ appVersion }}</div>
              </div>
            </div>
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">技术栈</div>
                <div class="settings-dialog__row-desc">
                  Tauri 2（Rust）+ Vue 3 + Vuetify 3 + xterm.js，终端数据经 Rust 侧透传，凭据仅存本地 SQLite。
                </div>
              </div>
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
 * SettingsDialog —— 高功能设置对话框
 *
 * 左侧分类导航 + 右侧内容区（参考 Navicat/HexHub 设置布局），分区：
 * - 外观：主题模式（浅色/深色/跟随系统，跟随系统经 matchMedia 实时切换 Vuetify theme）
 * - 终端：字号/字体家族/滚动缓冲/光标闪烁，变更实时生效到已打开终端（useXterm 联动）
 * - SFTP：默认下载目录（文本输入 + 目录选择按钮）
 * - 数据：清除查询历史（mysql_history_clear，带确认）、清除无效数据（清空传输临时缓存）
 * - 安全：主密码设置快捷入口（复用 MasterPasswordDialog 的打开机制）
 * - 关于：应用版本（tauri.conf.json version）+ 技术栈说明
 *
 * 所有设置项变更即写回后端 SQLite（settings 表），重启后仍保留。
 */
import { onMounted, ref, watch } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { open } from '@tauri-apps/plugin-dialog'
import { mysqlHistoryClear } from '@/api/mysqlConsole'
import { transferClear } from '@/api/sftp'
import { useSettingsStore, type ThemeMode } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'

/** 设置分区 key（左侧导航） */
type SettingsSection = 'appearance' | 'terminal' | 'sftp' | 'data' | 'security' | 'about'

/** 左侧导航定义 */
const SECTIONS: { key: SettingsSection; title: string; icon: string }[] = [
  { key: 'appearance', title: '外观', icon: 'mdi-palette-outline' },
  { key: 'terminal', title: '终端', icon: 'mdi-console' },
  { key: 'sftp', title: 'SFTP', icon: 'mdi-swap-horizontal' },
  { key: 'data', title: '数据', icon: 'mdi-database-outline' },
  { key: 'security', title: '安全', icon: 'mdi-lock-outline' },
  { key: 'about', title: '关于', icon: 'mdi-information-outline' },
]

/** 主题模式下拉选项 */
const THEME_MODE_ITEMS: { title: string; value: ThemeMode }[] = [
  { title: '浅色', value: 'light' },
  { title: '深色', value: 'dark' },
  { title: '跟随系统', value: 'auto' },
]

const props = defineProps<{
  modelValue: boolean
  /** 打开时默认展示的分区（如从"关于 FyShell"菜单进入时定位到"关于"） */
  initialSection?: SettingsSection
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'open-master-password'): void
}>()

const ui = useUiStore()
const settings = useSettingsStore()

// 启动时从后端加载设置（本组件常驻挂载于主工作区，setup 即应用启动时机；幂等）
void settings.ensureLoaded()

/** 当前展示的分区 */
const section = ref<SettingsSection>('appearance')

/** 应用版本（tauri.conf.json 的 version，经 app getVersion 读取） */
const appVersion = ref('')

/** 打开对话框时定位到指定分区 */
watch(
  () => props.modelValue,
  (visible) => {
    if (visible) section.value = props.initialSection ?? 'appearance'
  },
)


// ---------------- 终端设置提交（change 事件为原生冒泡，blur/Enter 时提交） ----------------

/** 默认字体大小：范围钳制 6-72px */
function onFontSizeChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 6 && v <= 72) settings.setTerminalFontSize(v)
}

/** 字体家族：非空才生效（清空回退默认值） */
function onFontFamilyChange(e: Event): void {
  const v = (e.target as HTMLInputElement).value.trim()
  if (v) settings.setTerminalFontFamily(v)
}

/** 滚动缓冲行数：范围钳制 100-1000000 行 */
function onScrollbackChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 100) settings.setTerminalScrollback(Math.min(v, 1_000_000))
}

/** 默认下载目录（手输路径，失焦提交） */
function onDownloadDirChange(e: Event): void {
  settings.setSftpDownloadDir((e.target as HTMLInputElement).value)
}

/** 目录选择按钮：@tauri-apps/plugin-dialog 目录选择，选中后立即生效 */
async function pickDownloadDir(): Promise<void> {
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择默认下载目录' })
    if (typeof selected === 'string' && selected) {
      settings.setSftpDownloadDir(selected)
    }
  } catch (e) {
    ui.toast(`选择目录失败：${String(e)}`, 'error')
  }
}

// ---------------- 数据清理 ----------------

/** 清除查询历史（危险操作二次确认，统一走 ui.confirm） */
async function clearQueryHistory(): Promise<void> {
  const ok = await ui.confirm({
    title: '清除查询历史',
    message: '确定清除全部 SQL 查询历史吗？此操作不可恢复。',
    danger: true,
  })
  if (!ok) return
  try {
    await mysqlHistoryClear()
    ui.toast('查询历史已清除', 'success')
  } catch (e) {
    ui.toast(`清除查询历史失败：${String(e)}`, 'error')
  }
}

/** 清除无效数据（清空已完成/失败的传输记录等临时缓存） */
async function clearInvalidData(): Promise<void> {
  const ok = await ui.confirm({
    title: '清除无效数据',
    message: '确定清空传输记录等临时缓存吗？进行中的任务不受影响。',
    danger: true,
  })
  if (!ok) return
  try {
    await transferClear()
    ui.toast('临时缓存已清空', 'success')
  } catch (e) {
    ui.toast(`清除失败：${String(e)}`, 'error')
  }
}

// ---------------- 关于 ----------------

onMounted(async () => {
  // 应用版本：tauri.conf.json 的 version（core:default 权限已覆盖 app:default）
  try {
    appVersion.value = await getVersion()
  } catch {
    // 读取失败时保持占位（显示为 v 未知）
  }
})
</script>

<style scoped>
.settings-dialog__body {
  display: flex;
  height: 420px;
}

/* 左侧分类导航 */
.settings-dialog__nav {
  display: flex;
  flex-direction: column;
  flex: 0 0 160px;
  gap: 2px;
  padding: 10px 6px;
  background: var(--fy-chrome-bg, #f0f2f5);
  overflow-y: auto;
}

.settings-dialog__nav-item {
  display: flex;
  align-items: center;
  padding: 6px 10px;
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

.settings-dialog__nav-item:hover {
  background: rgb(var(--v-theme-on-surface) / 0.08);
}

.settings-dialog__nav-item--active {
  background: rgb(var(--v-theme-primary) / 0.15);
  color: rgb(var(--v-theme-primary));
}

.settings-dialog__nav-item--active .mr-2 {
  color: rgb(var(--v-theme-primary));
}

/* 右侧内容区 */
.settings-dialog__content {
  flex: 1 1 auto;
  min-width: 0;
  padding: 14px 18px;
  overflow-y: auto;
}

.settings-dialog__section-title {
  font-size: 14px;
  font-weight: 600;
  margin-bottom: 12px;
}

.settings-dialog__field {
  max-width: 420px;
}

.settings-dialog__hint {
  margin-top: 10px;
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface) / 0.45);
}

/* 数据/安全分区行：左侧标题说明 + 右侧操作按钮 */
.settings-dialog__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 0;
}

.settings-dialog__row + .settings-dialog__row {
  border-top: 1px solid var(--fy-chrome-border, #d5d9de);
}

.settings-dialog__row-title {
  font-size: 13px;
  font-weight: 500;
}

.settings-dialog__row-desc {
  font-size: 11px;
  color: rgb(var(--v-theme-on-surface) / 0.55);
  margin-top: 2px;
}

.settings-dialog__field-row {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 420px;
  flex: 1 1 auto;
}
</style>
