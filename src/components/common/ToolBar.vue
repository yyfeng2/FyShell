<script setup lang="ts">
/**
 * ToolBar —— 经典工具栏（参考 Xshell）
 *
 * 单行分组式：最左"导航"开关（按下态高亮），按钮按"新建 | 连接 | 传输"分组，
 * 组间竖分隔线；右侧紧接"字体/编码/配色"快捷切换。
 * 全部按钮为"图标+底部汉字"结构（toolbar__titled，2-3 字简单命名）。
 * 字体按钮弹 Xshell 式三下拉面板（字体家族/字体样式/字号），选择即生效；
 * 配色按钮打开配色方案对话框。地址栏在工具栏下方独立一行（AddressBar.vue）。
 * 所有按钮带 title 工具提示（Xshell 风格提示体系）。
 */
import { computed, ref } from 'vue'
import { ENCODINGS } from '@/stores/session'
import { useSettingsStore } from '@/stores/settings'

/** 左导航展开态 / 终端字号 / 字体家族 / 字体样式 / 活动会话编码（父级注入，决定按下态与勾选态） */
const props = defineProps<{
  navOpen?: boolean
  fontSize?: number
  fontFamily?: string
  fontStyle?: string
  encoding?: string
}>()

const emit = defineEmits<{
  (e: 'nav'): void
  (e: 'new-session'): void
  (e: 'new-folder'): void
  (e: 'connect'): void
  (e: 'disconnect'): void
  (e: 'search'): void
  (e: 'transfer'): void
  (e: 'sftp'): void
  (e: 'font-size', size: number): void
  (e: 'font-family', family: string): void
  (e: 'font-style', style: string): void
  (e: 'encoding', encoding: string): void
  (e: 'scheme'): void
}>()

/** 字体快捷切换候选（px），完整范围仍在设置对话框"外观"分区 */
const FONT_SIZES = [12, 14, 16, 18, 20, 24]

/** 字体家族候选（label 显示名 + value 完整 CSS 列表），首项「系统默认」与设置 store 默认值同一来源 */
const FONT_FAMILIES = [
  { label: '系统默认', value: 'Consolas, "Liberation Mono", Menlo, Courier, monospace' },
  { label: 'Cascadia Mono', value: '"Cascadia Mono", Consolas, "Microsoft YaHei", monospace' },
  { label: 'Consolas', value: 'Consolas, "Microsoft YaHei", monospace' },
  { label: 'Courier New', value: '"Courier New", monospace' },
  { label: 'JetBrains Mono', value: '"JetBrains Mono", Consolas, monospace' },
  { label: 'Lucida Console', value: '"Lucida Console", monospace' },
  { label: 'Source Code Pro', value: '"Source Code Pro", Consolas, monospace' },
]

/** 字体样式候选（Xshell 惯例首项显示 Normal，其余中文） */
const FONT_STYLES = [
  { label: 'Normal', value: 'normal' },
  { label: '粗体', value: 'bold' },
  { label: '斜体', value: 'italic' },
]

/** 解析 CSS 字体列表首项（去引号 trim），用于与候选 label 精确匹配 */
function firstFamily(css?: string): string {
  if (!css) return ''
  return css.split(',')[0]?.trim().replace(/^"|"$/g, '') ?? ''
}

/** 工具栏设置 store（显示/隐藏 + 图标/小图标模式，全局设置与右键菜单共用） */
const settings = useSettingsStore()

/** 工具栏右键菜单：显示模式三态（Windows 惯例勾选标记），target 定位在光标处 */
const ctxOpen = ref(false)
const ctxTarget = ref<[number, number]>([0, 0])
function onContextMenu(e: MouseEvent): void {
  ctxTarget.value = [e.clientX, e.clientY]
  ctxOpen.value = true
}
</script>

<template>
  <div
    v-if="settings.toolbarVisible"
    class="toolbar"
    :class="{ 'toolbar--small': settings.toolbarMode === 'small-icon' }"
    @contextmenu.prevent="onContextMenu"
  >
    <!-- 导航开关（最左）：按下态高亮 -->
    <v-btn variant="text" :active="props.navOpen" title="左导航 开/收" class="toolbar__titled" @click="emit('nav')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-view-sidebar" />
        <span>导航</span>
      </span>
    </v-btn>
    <v-divider vertical inset class="toolbar__divider" />
    <!-- 新建组 -->
    <v-btn variant="text" title="新建会话 (Ctrl+T)" class="toolbar__titled" @click="emit('new-session')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-plus" />
        <span>新建</span>
      </span>
    </v-btn>
    <v-btn variant="text" title="新建文件夹" class="toolbar__titled" @click="emit('new-folder')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-folder-plus-outline" />
        <span>文件夹</span>
      </span>
    </v-btn>
    <v-divider vertical inset class="toolbar__divider" />
    <!-- 连接组：连接/断开保留语义色（全工具栏唯一的彩色点缀） -->
    <v-btn variant="text" color="success" title="连接选中的会话" class="toolbar__titled" @click="emit('connect')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-lan-connect" />
        <span>连接</span>
      </span>
    </v-btn>
    <v-btn variant="text" color="error" title="断开当前会话" class="toolbar__titled" @click="emit('disconnect')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-lan-disconnect" />
        <span>断开</span>
      </span>
    </v-btn>
    <v-divider vertical inset class="toolbar__divider" />
    <!-- 传输 / 视图组 -->
    <v-btn variant="text" title="搜索会话" class="toolbar__titled" @click="emit('search')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-magnify" />
        <span>搜索</span>
      </span>
    </v-btn>
    <v-btn variant="text" title="传输队列" class="toolbar__titled" @click="emit('transfer')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-swap-vertical" />
        <span>传输</span>
      </span>
    </v-btn>
    <v-btn variant="text" title="SFTP 文件传输" class="toolbar__titled" @click="emit('sftp')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-folder-swap-outline" />
        <span>传文件</span>
      </span>
    </v-btn>
    <v-divider vertical inset class="toolbar__divider" />

    <!-- 字体：Xshell 式三下拉面板（字体家族/字体样式/字号），选择即生效 -->
    <v-menu location="bottom left" :close-on-content-click="false">
      <template #activator="{ props: act }">
        <v-btn v-bind="act" variant="text" class="toolbar__titled" title="终端字体">
          <span class="toolbar__titled__body">
            <v-icon icon="mdi-format-font" />
            <span>字体</span>
          </span>
        </v-btn>
      </template>
      <div class="toolbar__fontpanel">
        <div class="toolbar__fontpanel__row">
          <span class="toolbar__fontpanel__label">字体</span>
          <v-select
            :model-value="props.fontFamily ?? FONT_FAMILIES[0].value"
            :items="FONT_FAMILIES"
            item-title="label"
            item-value="value"
            density="compact"
            hide-details
            class="toolbar__fontpanel__select"
            @update:model-value="(v: string) => emit('font-family', v)"
          />
        </div>
        <div class="toolbar__fontpanel__row">
          <span class="toolbar__fontpanel__label">样式</span>
          <v-select
            :model-value="props.fontStyle ?? 'normal'"
            :items="FONT_STYLES"
            item-title="label"
            item-value="value"
            density="compact"
            hide-details
            class="toolbar__fontpanel__select"
            @update:model-value="(v: string) => emit('font-style', v)"
          />
        </div>
        <div class="toolbar__fontpanel__row">
          <span class="toolbar__fontpanel__label">字号</span>
          <v-select
            :model-value="props.fontSize ?? 14"
            :items="FONT_SIZES"
            density="compact"
            hide-details
            class="toolbar__fontpanel__select"
            @update:model-value="(v: number) => emit('font-size', v)"
          />
        </div>
      </div>
    </v-menu>

    <!-- 编码：当前会话编码快捷切换 -->
    <v-menu location="bottom end" :close-on-content-click="true">
      <template #activator="{ props: act }">
        <v-btn v-bind="act" variant="text" class="toolbar__titled" title="当前会话编码">
          <span class="toolbar__titled__body">
            <v-icon icon="mdi-translate" />
            <span>编码</span>
          </span>
        </v-btn>
      </template>
      <v-list density="compact" class="toolbar__menu toolbar__menu--scroll">
        <v-list-item v-for="enc in ENCODINGS" :key="enc" :value="enc" @click="emit('encoding', enc)">
          <template #prepend>
            <span class="toolbar__checkmark">
              <v-icon v-if="enc === props.encoding" icon="mdi-check" size="13" />
            </span>
          </template>
          <v-list-item-title>{{ enc }}</v-list-item-title>
        </v-list-item>
      </v-list>
    </v-menu>

    <!-- 配色：打开配色方案对话框 -->
    <v-btn variant="text" class="toolbar__titled" title="选择配色方案" @click="emit('scheme')">
      <span class="toolbar__titled__body">
        <v-icon icon="mdi-palette" />
        <span>配色</span>
      </span>
    </v-btn>

    <!-- 右键菜单：查看子菜单（图标/小图标，与全局设置共用同一 store 状态） -->
    <v-menu v-model="ctxOpen" :target="ctxTarget" location="bottom left">
      <v-list density="compact">
        <!-- 查看子菜单：显示模式二选一 -->
        <v-menu location="right" :close-on-content-click="true">
          <template #activator="{ props: act }">
            <v-list-item v-bind="act" append-icon="mdi-chevron-right" title="查看" />
          </template>
          <v-list density="compact">
            <v-list-item value="icon-title" @click="settings.setToolbarMode('icon-title')">
              <template #prepend>
                <span class="toolbar__checkmark">
                  <v-icon v-if="settings.toolbarMode === 'icon-title'" icon="mdi-check" size="13" />
                </span>
              </template>
              <v-list-item-title>图标</v-list-item-title>
            </v-list-item>
            <v-list-item value="small-icon" @click="settings.setToolbarMode('small-icon')">
              <template #prepend>
                <span class="toolbar__checkmark">
                  <v-icon v-if="settings.toolbarMode === 'small-icon'" icon="mdi-check" size="13" />
                </span>
              </template>
              <v-list-item-title>小图标</v-list-item-title>
            </v-list-item>
          </v-list>
        </v-menu>
      </v-list>
    </v-menu>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  min-height: 26px; /* 与 MenuBar/StatusBar 26px 节奏一致（原 20px 比相邻条带矮 6px） */
  padding: 0.5em 4px; /* 上下各 0.5 字符（em 随字号缩放） */
  gap: 6px; /* 图标间隔增加 1 字符（原相邻贴靠） */
  background: var(--fy-chrome-bg);
  user-select: none;
}

.toolbar__divider {
  height: 18px;
  flex-shrink: 0;
}

/* 覆盖 Vuetify 竖向 inset divider 默认 margin-block 8px：工具栏行高收紧到内容高 */
.toolbar :deep(.v-divider--inset) {
  margin-block: 0;
}

/* 图标按钮：20px 按钮 + 17px 图标（Vuetify 默认 20px 图标偏粗糙），细腻清晰 */
.toolbar :deep(.v-btn .v-icon) {
  font-size: 17px;
}

/* 导航开关按下态：浅色背景高亮 */
.toolbar :deep(.v-btn--active) {
  background: rgb(var(--v-theme-on-surface) / 0.1);
}

/* 图标+底部汉字的快捷按钮：覆盖 v-btn 默认 64px min-width 与固定行高 */
.toolbar :deep(.toolbar__titled) {
  min-width: 0;
  height: auto;
  min-height: 0;
  padding: 2px 8px;
}

.toolbar__titled__body {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1px;
}

.toolbar__titled__body > span:last-child {
  font-size: 11px;
  line-height: 1.2;
}

/* 小图标模式：仅显示图标（标题隐藏），按钮 padding 收窄（title 工具提示保留） */
.toolbar--small .toolbar__titled__body > span:last-child {
  display: none;
}

.toolbar--small :deep(.toolbar__titled) {
  padding: 2px 4px;
}

/* 下拉菜单勾选列：固定宽度占位保持标题对齐（与 MenuBar 勾选列惯例一致） */
.toolbar__checkmark {
  display: inline-block;
  width: 14px;
}

.toolbar__menu--scroll {
  max-height: 320px;
  overflow-y: auto;
}

/* 字体三下拉面板（Xshell 风格：label 左置 + 下拉框右排） */
.toolbar__fontpanel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  background: rgb(var(--v-theme-surface));
  min-width: 260px;
}

.toolbar__fontpanel__row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.toolbar__fontpanel__label {
  font-size: 13px;
  width: 56px;
  flex-shrink: 0;
  color: rgb(var(--v-theme-on-surface) / 0.75);
}

.toolbar__fontpanel__select {
  flex: 1;
  min-width: 0;
}
</style>
