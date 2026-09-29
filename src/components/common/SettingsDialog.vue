<template>
  <v-dialog
    :model-value="modelValue"
    max-width="680"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-cog-outline" size="small" class="mr-2" />
        设置
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
            <v-icon :icon="s.icon" size="13" class="mr-2" />
            {{ s.title }}
          </button>
        </nav>
        <v-divider vertical />
        <!-- 右侧内容区 -->
        <div class="settings-dialog__content">
          <!-- 外观 -->
          <template v-if="section === 'appearance'">
            <div class="settings-dialog__section-title">外观</div>
            <div class="fy-field-row">
              <span class="fy-field-row__label">主题模式</span>
              <v-select
                :model-value="settings.themeMode"
                :items="THEME_MODE_ITEMS"
                item-title="title"
                item-value="value"
                density="compact"
                class="settings-dialog__field"
                @update:model-value="(v: unknown) => settings.setThemeMode(v as ThemeMode)"
              />
            </div>
            <!-- 布局缩放：界面等比缩放（CSS zoom），立即生效并持久化 -->
            <div class="fy-field-row">
              <span class="fy-field-row__label">布局缩放</span>
              <v-select
                :model-value="settings.uiFontSize"
                :items="UI_SCALE_ITEMS"
                density="compact"
                class="settings-dialog__field"
                @update:model-value="(v: unknown) => settings.setUiFontSize(Number(v))"
              />
            </div>
            <!-- 工具栏：勾选展示不勾选隐藏；勾选时才出现 图标/小图标 选择（默认图标） -->
            <div class="key-mouse__group-title">工具栏</div>
            <v-checkbox
              :model-value="settings.toolbarVisible"
              label="显示工具栏"
              color="primary"
              density="compact"
              hide-details
              @update:model-value="(v: unknown) => settings.setToolbarVisible(!!v)"
            />
            <template v-if="settings.toolbarVisible">
              <v-checkbox
                :model-value="settings.toolbarMode === 'icon-title'"
                label="图标（图标 + 标题）"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => v && settings.setToolbarMode('icon-title')"
              />
              <v-checkbox
                :model-value="settings.toolbarMode === 'small-icon'"
                label="小图标（仅图标）"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => v && settings.setToolbarMode('small-icon')"
              />
            </template>
            <!-- 托盘：勾选=关闭窗口隐藏到托盘，默认不勾选（关闭窗口即退出） -->
            <div class="key-mouse__group-title">托盘</div>
            <v-checkbox
              :model-value="settings.trayCloseToTray"
              label="关闭窗口时最小化到托盘"
              color="primary"
              density="compact"
              hide-details
              @update:model-value="(v: unknown) => settings.setTrayCloseToTray(!!v)"
            />
          </template>

          <!-- 终端 -->
          <template v-else-if="section === 'terminal'">
            <div class="settings-dialog__section-title">终端</div>
            <div class="fy-field-row">
              <span class="fy-field-row__label">默认字体大小（px）</span>
              <v-text-field
                :model-value="settings.terminalFontSize"
                type="number"
                min="6"
                max="72"
                density="compact"
                class="settings-dialog__field"
                @change="onFontSizeChange"
              />
            </div>
            <div class="fy-field-row">
              <span class="fy-field-row__label">字体家族</span>
              <v-text-field
                :model-value="settings.terminalFontFamily"
                density="compact"
                class="settings-dialog__field"
                placeholder="Consolas, &quot;Liberation Mono&quot;, monospace"
                @change="onFontFamilyChange"
              />
            </div>
            <div class="fy-field-row">
              <span class="fy-field-row__label">滚动缓冲行数</span>
              <v-text-field
                :model-value="settings.terminalScrollback"
                type="number"
                min="100"
                max="1000000"
                density="compact"
                class="settings-dialog__field"
                @change="onScrollbackChange"
              />
            </div>
            <v-switch
              :model-value="settings.terminalCursorBlink"
              label="光标闪烁"
              color="primary"
              density="compact"
              hide-details
              @update:model-value="(v: unknown) => settings.setTerminalCursorBlink(!!v)"
            />
          </template>

          <!-- 键盘和鼠标 -->
          <template v-else-if="section === 'keyboard-mouse'">
            <div class="settings-dialog__section-title">键盘和鼠标</div>

            <!-- 按键对应 -->
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">按键对应</div>
                <div class="settings-dialog__row-desc">
                  键盘操作可由菜单功能、发送字符串等自定义。
                </div>
              </div>
              <v-btn size="small" variant="tonal" prepend-icon="mdi-pencil-outline" @click="showKeyMapping = true">
                编辑(E)...
              </v-btn>
            </div>

            <!-- 鼠标 -->
            <div class="key-mouse__group-title">鼠标</div>
            <div class="key-mouse__fields">
              <div class="fy-field-row">
                <span class="fy-field-row__label">中间按钮</span>
                <v-select
                  :model-value="settings.mouseMiddleButton"
                  :items="MOUSE_BUTTON_ITEMS"
                  item-title="title"
                  item-value="value"
                  density="compact"
                  class="settings-dialog__field"
                  @update:model-value="(v: unknown) => settings.setMouseMiddleButton(v as MouseButtonAction)"
                />
              </div>
              <div class="fy-field-row">
                <span class="fy-field-row__label">向右按钮</span>
                <v-select
                  :model-value="settings.mouseRightButton"
                  :items="MOUSE_BUTTON_ITEMS"
                  item-title="title"
                  item-value="value"
                  density="compact"
                  class="settings-dialog__field"
                  @update:model-value="(v: unknown) => settings.setMouseRightButton(v as MouseButtonAction)"
                />
              </div>
              <v-switch
                :model-value="settings.mouseCtrlClickMoveCursor"
                label="用(Ctrl +鼠标左单击)移动终端光标"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setMouseCtrlClickMoveCursor(!!v)"
              />
              <v-switch
                :model-value="settings.mouseUrlHyperlink"
                label="使用URL超链接"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setMouseUrlHyperlink(!!v)"
              />
              <template v-if="settings.mouseUrlHyperlink">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">URL prefix</span>
                  <v-text-field
                    :model-value="settings.mouseUrlPrefixes"
                    density="compact"
                    hint="以 | 分隔的 URL 前缀"
                    class="settings-dialog__field key-mouse__indent"
                    @change="onUrlPrefixesChange"
                  />
                </div>
                <v-switch
                  :model-value="settings.mouseCtrlClickOpenHyperlink"
                  label="[Ctrl +单击]以打开超链接"
                  color="primary"
                  density="compact"
                  hide-details
                  @update:model-value="(v: unknown) => settings.setMouseCtrlClickOpenHyperlink(!!v)"
                />
              </template>
            </div>

            <!-- 选择 -->
            <div class="key-mouse__group-title">选择</div>
            <div class="key-mouse__hint-line">双击指定选择时使用的分隔符。</div>
            <div class="key-mouse__fields">
              <div class="key-mouse__delimiter-row">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">分隔符</span>
                  <v-text-field
                    :model-value="settings.selectionWordSeparators"
                    density="compact"
                    hide-details
                    class="flex-grow-1"
                    @change="onWordSeparatorsChange"
                  />
                </div>
                <v-btn size="small" variant="tonal" @click="resetWordSeparators">重置</v-btn>
              </div>
              <v-switch
                :model-value="settings.selectionShiftDoubleClick"
                label="[Shift +双击]按分隔符而不是空格选择"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionShiftDoubleClick(!!v)"
              />
              <v-switch
                :model-value="settings.selectionAutoCopy"
                label="将选定的文本自动复制到剪贴板"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionAutoCopy(!!v)"
              />
              <v-switch
                :model-value="settings.selectionSoftTabs"
                label="转换制表符软标签"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionSoftTabs(!!v)"
              />
              <v-switch
                :model-value="settings.selectionCopyIncludeNewline"
                label="复制选定的文本时，包括最后一个新行字符"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionCopyIncludeNewline(!!v)"
              />
              <v-switch
                :model-value="settings.selectionCopyTrimWhitespace"
                label="复制时，删除尾部的空白"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionCopyTrimWhitespace(!!v)"
              />
              <v-switch
                :model-value="settings.selectionCopyNonblankOnly"
                label="复制时，排除仅含空白的行"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setSelectionCopyNonblankOnly(!!v)"
              />
            </div>
          </template>

          <!-- 快捷键 -->
          <template v-else-if="section === 'shortcuts'">
            <div class="settings-dialog__section-title">快捷键</div>
            <div class="key-mouse__fields">
              <div v-for="item in SHORTCUT_ITEMS" :key="item.key" class="fy-field-row">
                <span class="fy-field-row__label">{{ item.title }}</span>
                <button
                  type="button"
                  class="shortcuts__combo"
                  :class="{ 'shortcuts__combo--capturing': capturingKey === item.key }"
                  @click="startCapture(item.key)"
                >
                  {{ capturingKey === item.key ? '请按下键位组合…' : formatShortcutCombo(settings.shortcutValue(item.key)) }}
                </button>
              </div>
            </div>
            <div v-if="captureError" class="shortcuts__error">{{ captureError }}</div>
            <div class="settings-dialog__row shortcuts__footer">
              <div class="settings-dialog__row-desc">
                点击键位框后按下新组合（需含 Ctrl 或 Alt），Esc 取消；全局快捷键即时生效。
              </div>
              <v-btn size="small" variant="tonal" @click="resetShortcuts">恢复默认</v-btn>
            </div>
          </template>

          <!-- SFTP -->
          <template v-else-if="section === 'sftp'">
            <div class="settings-dialog__section-title">SFTP</div>
            <div class="settings-dialog__row">
              <div class="settings-dialog__field-row">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">默认下载目录</span>
                  <v-text-field
                    :model-value="settings.sftpDownloadDir"
                    density="compact"
                    placeholder="留空使用系统下载目录"
                    hide-details
                    @change="onDownloadDirChange"
                  />
                </div>
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
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">删除用户数据</div>
                <div class="settings-dialog__row-desc">
                  清空导航树会话/文件夹、保存的连接、查询历史、备份档案、快捷命令、键位映射、SFTP
                  收藏、隧道、应用设置与主密码保险库，恢复到首次运行状态，不可恢复。
                </div>
              </div>
              <v-btn size="small" color="error" variant="tonal" @click="clearAllUserData">
                删除…
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
                <div class="settings-dialog__row-title">作者</div>
                <div class="settings-dialog__row-desc">yyfeng · 564792432@qq.com</div>
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
            <!-- 更新：默认不自动检测，勾选后启动时自动检测（检测到新版本仍需确认才下载安装） -->
            <div class="settings-dialog__row">
              <div>
                <div class="settings-dialog__row-title">启动时自动检测更新</div>
                <div class="settings-dialog__row-desc">
                  默认关闭，仅通过菜单栏「帮助 → 检测更新」手动检测；勾选后每次启动自动检测，检测到新版本仍需确认后才下载安装。
                </div>
              </div>
              <v-checkbox
                :model-value="settings.autoUpdateCheck"
                color="primary"
                density="compact"
                hide-details
                @update:model-value="(v: unknown) => settings.setAutoUpdateCheck(!!v)"
              />
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

  <!-- 键位映射管理对话框（键盘和鼠标分区 → 按键对应 → 编辑(E)...） -->
  <KeyMappingDialog v-model="showKeyMapping" />
</template>

<script setup lang="ts">
/**
 * SettingsDialog —— 高功能设置对话框
 *
 * 左侧分类导航 + 右侧内容区（参考 Navicat/HexHub 设置布局），分区：
 * - 外观：主题模式（浅色/深色/跟随系统，跟随系统经 matchMedia 实时切换 Vuetify theme）+ 布局缩放（CSS zoom 等比缩放全部界面）
 * - 终端：字号/字体家族/滚动缓冲/光标闪烁，变更实时生效到已打开终端（useXterm 联动）
 * - 快捷键：7 个可修改快捷键捕获式修改（需含 Ctrl/Alt 修饰、查重、恢复默认），
 *   WorkspaceView 全局注册与 useXterm 终端剪贴板键均按设置值联动
 * - SFTP：默认下载目录（文本输入 + 目录选择按钮）
 * - 数据：清除查询历史（mysql_history_clear，带确认）、清除无效数据（清空传输临时缓存）
 * - 安全：主密码设置快捷入口（复用 MasterPasswordDialog 的打开机制）
 * - 关于：应用版本（tauri.conf.json version）+ 作者联系方式 + 技术栈说明
 *
 * 所有设置项变更即写回后端 SQLite（settings 表），重启后仍保留。
 */
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { mysqlHistoryClear } from '@/api/mysqlConsole'
import { localPickDialog, transferClear } from '@/api/sftp'
import { userDataClear } from '@/api/settings'
import KeyMappingDialog from '@/components/common/KeyMappingDialog.vue'
import { formatShortcutCombo, SETTING_KEYS, SHORTCUT_DEFAULTS, useSettingsStore, type ThemeMode, type MouseButtonAction } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'
import { friendlyError } from '@/utils/errors'

/** 设置分区 key（左侧导航） */
type SettingsSection = 'appearance' | 'terminal' | 'keyboard-mouse' | 'shortcuts' | 'sftp' | 'data' | 'security' | 'about'

/** 左侧导航定义 */
const SECTIONS: { key: SettingsSection; title: string; icon: string }[] = [
  { key: 'appearance', title: '外观', icon: 'mdi-palette-outline' },
  { key: 'terminal', title: '终端', icon: 'mdi-console' },
  { key: 'keyboard-mouse', title: '键盘和鼠标', icon: 'mdi-keyboard-outline' },
  { key: 'shortcuts', title: '快捷键', icon: 'mdi-keyboard-settings-outline' },
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

/** 布局缩放下拉选项（百分比） */
const UI_SCALE_ITEMS: { title: string; value: number }[] = [
  { title: '90%（小）', value: 90 },
  { title: '100%（标准）', value: 100 },
  { title: '110%（大）', value: 110 },
  { title: '125%（特大）', value: 125 },
]

/** 鼠标中/右键行为下拉选项 */
const MOUSE_BUTTON_ITEMS: { title: string; value: MouseButtonAction }[] = [
  { title: '无操作', value: 'nothing' },
  { title: '粘贴剪贴板', value: 'paste' },
]

/** 键位映射管理对话框可见性（键盘和鼠标分区的「编辑(E)...」按钮） */
const showKeyMapping = ref(false)

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

// ---------------- 键盘和鼠标设置提交 ----------------

/** URL 前缀（失焦提交，非空才生效） */
function onUrlPrefixesChange(e: Event): void {
  const v = (e.target as HTMLInputElement).value.trim()
  if (v) settings.setMouseUrlPrefixes(v)
}

/** 双击选择分隔符（失焦提交；空值回退默认分隔符） */
function onWordSeparatorsChange(e: Event): void {
  const v = (e.target as HTMLInputElement).value
  if (v !== '') settings.setSelectionWordSeparators(v)
}

/** 分隔符重置为默认值（与 Xshell 默认一致） */
function resetWordSeparators(): void {
  settings.setSelectionWordSeparators('\\:\\~\\-!@#$%^&*()-=+[]{}')
}

// ---------------- 快捷键设置（捕获式修改，与 KeyMappingDialog 同模式） ----------------

/** 可修改快捷键项（settings 键 → 动作名） */
const SHORTCUT_ITEMS: { key: keyof typeof SHORTCUT_DEFAULTS; title: string }[] = [
  { key: SETTING_KEYS.shortcutNewSession, title: '新建会话' },
  { key: SETTING_KEYS.shortcutCloseTab, title: '关闭当前标签' },
  { key: SETTING_KEYS.shortcutNextTab, title: '切换到下一个标签' },
  { key: SETTING_KEYS.shortcutCopy, title: '复制' },
  { key: SETTING_KEYS.shortcutCut, title: '剪切' },
  { key: SETTING_KEYS.shortcutPaste, title: '粘贴' },
  { key: SETTING_KEYS.shortcutSelectAll, title: '全选' },
]

/** 正在捕获的设置键（null = 未在捕获） */
const capturingKey = ref<string | null>(null)

/** 捕获校验失败提示（空 = 无错误） */
const captureError = ref('')

/** 开始捕获某设置键的新键位组合（window 捕获阶段监听，Escape 取消） */
function startCapture(key: string): void {
  capturingKey.value = key
  captureError.value = ''
  window.addEventListener('keydown', onCaptureKeydown, true)
}

function stopCapture(): void {
  capturingKey.value = null
  window.removeEventListener('keydown', onCaptureKeydown, true)
}

function onCaptureKeydown(e: KeyboardEvent): void {
  // 捕获阶段立即终止（含同元素后续监听）：捕获期间全局快捷键不触发
  e.preventDefault()
  e.stopImmediatePropagation()
  const key = capturingKey.value
  stopCapture()
  if (!key) return
  // Escape 取消捕获，不修改
  if (e.key === 'Escape') return
  const combo = ui.shortcutOf(e)
  // 校验：必须带 Ctrl 或 Alt 修饰（纯字母/数字键会与终端输入冲突）
  if (!e.ctrlKey && !e.metaKey && !e.altKey) {
    captureError.value = `「${formatShortcutCombo(combo)}」无效：需包含 Ctrl 或 Alt 修饰键`
    return
  }
  // 校验：不能与其他可修改快捷键重复
  const dup = SHORTCUT_ITEMS.find((it) => it.key !== key && settings.shortcutValue(it.key) === combo)
  if (dup) {
    captureError.value = `「${formatShortcutCombo(combo)}」已用于「${dup.title}」`
    return
  }
  settings.setShortcut(key, combo)
}

/** 全部快捷键恢复默认值 */
function resetShortcuts(): void {
  for (const item of SHORTCUT_ITEMS) {
    settings.setShortcut(item.key, SHORTCUT_DEFAULTS[item.key])
  }
  captureError.value = ''
}

/** 目录选择按钮：自研原生目录选择器，选中后立即生效 */
async function pickDownloadDir(): Promise<void> {
  try {
    const selected = await localPickDialog('recv', { title: '选择默认下载目录' })
    if (typeof selected === 'string' && selected) {
      settings.setSftpDownloadDir(selected)
    }
  } catch (e) {
    ui.toast(`选择目录失败：${friendlyError(e)}`, 'error')
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
    ui.toast(`清除查询历史失败：${friendlyError(e)}`, 'error')
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
    ui.toast(`清除失败：${friendlyError(e)}`, 'error')
  }
}

/**
 * 删除全部用户数据（恢复到首次运行状态）：清空全部用户数据表 + 会话日志。
 * 二次确认列出全部删除项；成功后刷新 WebView 重置前端缓存状态（数据库已清空）。
 */
async function clearAllUserData(): Promise<void> {
  const ok = await ui.confirm({
    title: '删除用户数据',
    message: '确定删除全部用户数据吗？将清空导航树会话与文件夹、保存的连接、MySQL/Redis 查询历史与保存的查询、备份档案、快捷命令、键位映射、SFTP 收藏、隧道、认证配置、应用设置以及主密码与凭据保险库，恢复到首次运行状态。此操作不可恢复。',
    danger: true,
  })
  if (!ok) return
  try {
    await userDataClear()
    // 刷新 WebView：前端 Pinia 缓存（导航树/设置等）全部重置为空数据
    window.location.reload()
  } catch (e) {
    ui.toast(`删除用户数据失败：${friendlyError(e)}`, 'error')
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

onUnmounted(() => {
  // 组件卸载时捕获监听兜底清理（正常路径 stopCapture 已移除）
  stopCapture()
})
</script>

<style scoped>
.settings-dialog__body {
  display: flex;
  height: 380px;
}

/* 标题行收紧（Vuetify v-card-title 默认 16px 内边距） */
.v-card-title {
  padding: 10px 16px;
}

/* 左侧分类导航 */
.settings-dialog__nav {
  display: flex;
  flex-direction: column;
  flex: 0 0 148px;
  gap: 2px;
  padding: 8px 6px;
  background: var(--fy-chrome-bg);
  overflow-y: auto;
}

.settings-dialog__nav-item {
  display: flex;
  align-items: center;
  padding: 4px 10px;
  border-radius: 4px;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 12px;
  line-height: 1.4;
  cursor: pointer;
  text-align: left;
  user-select: none;
}

.settings-dialog__nav-item:hover {
  background: var(--fy-hover-bg);
}

/* 键盘焦点可见态：2px primary 焦点环（浅灰底上 1px 偏细），压掉浏览器默认矩形框 */
.settings-dialog__nav-item:focus-visible {
  outline: 2px solid var(--fy-focus-color);
  outline-offset: -2px;
}

.settings-dialog__nav-item--active {
  background: rgba(var(--v-theme-primary), 0.15);
  color: rgb(var(--v-theme-primary));
}

.settings-dialog__nav-item--active .mr-2 {
  color: rgb(var(--v-theme-primary));
}

/* 右侧内容区 */
.settings-dialog__content {
  flex: 1 1 auto;
  min-width: 0;
  padding: 12px 16px;
  overflow-y: auto;
}

.settings-dialog__section-title {
  font-size: 12px;
  font-weight: 400;
  margin-bottom: 8px;
}

.settings-dialog__field {
  max-width: 420px;
}

/* 数据/安全分区行：左侧标题说明 + 右侧操作按钮 */
.settings-dialog__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 6px 0;
}

.settings-dialog__row + .settings-dialog__row {
  border-top: 1px solid var(--fy-chrome-border);
}

.settings-dialog__row-title {
  font-size: 12px;
  font-weight: 400;
}

.settings-dialog__row-desc {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.55);
  margin-top: 2px;
}

.settings-dialog__field-row {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 420px;
  flex: 1 1 auto;
}

/* 键盘和鼠标分区：分组小标题 + 字段列 + 分隔符行 */
.key-mouse__group-title {
  font-size: 12px;
  font-weight: 400;
  margin: 12px 0 8px;
}

.key-mouse__hint-line {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.55);
  margin-bottom: 8px;
}

.key-mouse__fields {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: flex-start;
}

.key-mouse__fields .settings-dialog__field {
  width: 100%;
  max-width: 420px;
}

.key-mouse__indent {
  margin-left: 24px;
}

.key-mouse__delimiter-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  max-width: 420px;
}

/* 快捷键分区：键位框 + 错误提示 + 底部说明/恢复默认行 */
.shortcuts__combo {
  min-width: 180px;
  padding: 2px 10px;
  font-size: 12px;
  font-family: var(--fy-mono);
  border: 1px solid var(--fy-chrome-border);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: center;
}

.shortcuts__combo:hover {
  border-color: rgba(var(--v-theme-on-surface), 0.35);
}

.shortcuts__combo--capturing {
  border-color: rgb(var(--v-theme-primary));
  color: rgb(var(--v-theme-primary));
}

.shortcuts__error {
  margin-top: 8px;
  font-size: 12px;
  color: rgb(var(--v-theme-error));
}

.shortcuts__footer {
  margin-top: 12px;
}
</style>
