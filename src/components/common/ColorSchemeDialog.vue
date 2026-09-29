<script setup lang="ts">
/**
 * ColorSchemeDialog —— 配色方案对话框（工具栏「配色」入口）
 *
 * Xshell 风格：顶部预览区（方案背景色底，三行：文字样式 / 亮色 ANSI / 暗色 ANSI）、
 * 中部方案列表（选中即应用并持久化，useXterm watch 联动实时生效）、
 * 右侧方案管理按钮列。编辑模式以副本编辑（取消不落库），
 * 导入导出经 Rust 文件 IO + 前端 parseScheme 校验。
 */
import { computed, ref, watch } from 'vue'
import { localPickDialog, localSaveDialog } from '@/api/sftp'
import { useColorSchemeStore } from '@/stores/colorScheme'
import { useUiStore } from '@/stores/ui'
import { commands } from '@/bindings'
import { friendlyError as errText } from '@/utils/errors'
import { ANSI_KEYS, BUILTIN_SCHEMES, PREVIEW_STYLES, parseScheme, type ColorScheme } from '@/data/colorSchemes'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const store = useColorSchemeStore()
const ui = useUiStore()

/** 编辑中的方案副本（null = 非编辑模式） */
const editing = ref<ColorScheme | null>(null)

watch(
  () => props.modelValue,
  (v) => {
    if (v) {
      editing.value = null
      void store.ensureLoaded()
    }
  },
)

/** 预览用方案（编辑模式用编辑副本实时预览，否则用应用中方案） */
const previewScheme = computed(() => (editing.value ? editing.value : store.applied))

/** 预览色值读取（动态键索引，缺省白色） */
function colorOf(key: string): string {
  const scheme = previewScheme.value
  return scheme ? ((scheme as unknown as Record<string, string>)[key] ?? '#ffffff') : '#ffffff'
}

/** 预览行文字样式（kind → CSS） */
function previewClass(kind: 'normal' | 'bold' | 'underline' | 'reversed'): string {
  return `scheme-preview__item scheme-preview__item--${kind}`
}

/** 编辑网格字段（常规 5 项） */
const BASE_EDIT_KEYS: { key: keyof ColorScheme; label: string }[] = [
  { key: 'foreground', label: '前景' },
  { key: 'background', label: '背景' },
  { key: 'cursor', label: '光标' },
  { key: 'cursorAccent', label: '光标文字' },
  { key: 'selectionBackground', label: '选区' },
]

/** 编辑网格字段（ANSI 16 色：亮色 8 + 普通 8） */
const ANSI_EDIT_KEYS: { key: keyof ColorScheme; label: string }[] = [
  ...ANSI_KEYS.map(
    (k) => `bright${k[0].toUpperCase()}${k.slice(1)}` as keyof ColorScheme,
  ).map((k, i) => ({ key: k, label: `亮${ANSI_KEYS[i]}` })),
  ...ANSI_KEYS.map((k) => ({ key: k as keyof ColorScheme, label: k })),
]

/** 编辑副本色值更新（input type=color @input） */
function setEditColor(key: keyof ColorScheme, value: string): void {
  if (!editing.value) return
  ;(editing.value as Record<string, unknown>)[key] = value
}

/** 新建：以应用中方案（回退 FyShell 默认）为底本，生成唯一名称并进入编辑 */
function createScheme(): void {
  const base = store.applied ?? BUILTIN_SCHEMES[0]
  const draft: ColorScheme = { ...base, name: store.uniqueName('新建方案') }
  store.upsert(draft)
  store.select(draft.name)
  editing.value = { ...draft }
}

/** 编辑当前选中方案（副本编辑，保存时 upsert） */
function editScheme(): void {
  const src = store.applied
  if (!src) return
  editing.value = { ...src }
}

/** 另存为：当前方案复制为新名称（不进入编辑，保持 Xshell 惯例） */
function duplicateScheme(): void {
  const src = store.applied
  if (!src) return
  const draft: ColorScheme = { ...src, name: store.uniqueName(`${src.name} 副本`) }
  store.upsert(draft)
  store.select(draft.name)
  ui.toast(`已另存为「${draft.name}」`)
}

/** 删除选中方案（全局确认，删除应用中方案时回退首个剩余） */
async function removeScheme(): Promise<void> {
  const name = store.appliedName
  if (!name) return
  const ok = await ui.confirm({
    title: '删除配色方案',
    message: `确定删除配色方案「${name}」？`,
    danger: true,
  })
  if (ok) store.remove(name)
}

/** 导入 IO 中（系统对话框期间不置，读写过程防重复点击） */
const importing = ref(false)

/** 导入：系统对话框选文件 → Rust 读 → parseScheme 校验 → upsert */
async function importScheme(): Promise<void> {
  const path = await localPickDialog('send', {
    filterName: '配色方案',
    extensions: ['json'],
  })
  if (typeof path !== 'string') return
  importing.value = true
  try {
    const raw = await commands.schemeReadFile(path)
    const scheme = parseScheme(JSON.parse(raw))
    if (!scheme) {
      ui.toast('导入失败：文件不是有效的配色方案', 'error')
      return
    }
    const named: ColorScheme = { ...scheme, name: store.uniqueName(scheme.name) }
    store.upsert(named)
    ui.toast(`已导入配色方案「${named.name}」`)
  } catch (e) {
    ui.toast(`导入失败：${errText(e)}`, 'error')
  } finally {
    importing.value = false
  }
}

/** 导出 IO 中（系统对话框期间不置，读写过程防重复点击） */
const exporting = ref(false)

/** 导出：系统对话框选路径 → Rust 写（JSON 序列化当前方案） */
async function exportScheme(): Promise<void> {
  const src = store.applied
  if (!src) return
  const path = await localSaveDialog(`${src.name}.json`, {
    filterName: '配色方案',
    extensions: ['json'],
  })
  if (typeof path !== 'string') return
  exporting.value = true
  try {
    await commands.schemeWriteFile(path, JSON.stringify(src, null, 2))
    ui.toast(`已导出到 ${path}`)
  } catch (e) {
    ui.toast(`导出失败：${errText(e)}`, 'error')
  } finally {
    exporting.value = false
  }
}

/** 保存编辑副本（upsert 持久化并保持应用） */
function saveEditing(): void {
  const draft = editing.value
  if (!draft) return
  store.upsert({ ...draft })
  store.select(draft.name)
  editing.value = null
}

/** 取消编辑（丢弃副本） */
function cancelEditing(): void {
  editing.value = null
}
</script>

<template>
  <v-dialog
    :model-value="modelValue"
    width="560"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card density="compact">
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-palette-outline" size="small" class="mr-2" />
        选择配色方案
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

      <!-- 非编辑模式：预览区 + 方案列表 + 管理按钮列 -->
      <template v-if="!editing">
        <div
          class="scheme-preview"
          :style="{ background: previewScheme?.background ?? '#000000', color: previewScheme?.foreground ?? '#ffffff' }"
        >
          <div class="scheme-preview__line">
            <span :class="previewClass('normal')">Normal</span>
            <span :class="previewClass('bold')">Bold</span>
            <span :class="previewClass('underline')">Underline</span>
            <span
              :class="previewClass('reversed')"
              :style="{ background: previewScheme?.foreground ?? '#ffffff', color: previewScheme?.background ?? '#000000' }"
            >Reversed</span>
            <span
              class="scheme-preview__cursor"
              title="光标"
              :style="{ backgroundColor: previewScheme?.cursor ?? '#ffffff' }"
            />
          </div>
          <div class="scheme-preview__line">
            <span
              v-for="key in ANSI_KEYS"
              :key="`bright-${key}`"
              :style="{ color: colorOf(`bright${key[0].toUpperCase()}${key.slice(1)}`) }"
            >{{ key }}</span>
          </div>
          <div class="scheme-preview__line">
            <span
              v-for="key in ANSI_KEYS"
              :key="`normal-${key}`"
              :style="{ color: colorOf(key) }"
            >{{ key }}</span>
          </div>
        </div>
        <div class="scheme-body">
          <div class="scheme-list">
            <div
              v-for="s in store.schemes"
              :key="s.name"
              class="scheme-list__row"
              :class="{ 'scheme-list__row--active': s.name === store.appliedName }"
              :title="`背景 ${s.background}`"
              @click="store.select(s.name)"
            >
              <span
                class="scheme-list__swatch"
                :style="{ backgroundColor: s.background, borderColor: s.foreground }"
              />
              <span class="scheme-list__name">{{ s.name }}</span>
              <v-icon
                v-if="s.name === store.appliedName"
                icon="mdi-check"
                size="14"
                class="scheme-list__check"
              />
            </div>
          </div>
          <div class="scheme-actions">
            <v-btn size="small" variant="text" block class="scheme-actions__btn" @click="createScheme">
              新建<span class="scheme-actions__key">(N)</span>
            </v-btn>
            <v-btn size="small" variant="text" block class="scheme-actions__btn" @click="editScheme">
              编辑<span class="scheme-actions__key">(E)</span>...
            </v-btn>
            <v-btn size="small" variant="text" block class="scheme-actions__btn" @click="duplicateScheme">
              另存为<span class="scheme-actions__key">(S)</span>
            </v-btn>
            <v-btn size="small" variant="text" block class="scheme-actions__btn" @click="removeScheme">
              删除<span class="scheme-actions__key">(D)</span>
            </v-btn>
            <v-btn size="small" variant="text" block class="scheme-actions__btn" :loading="importing" @click="importScheme">
              导入<span class="scheme-actions__key">(I)</span>...
            </v-btn>
            <v-btn size="small" variant="text" block class="scheme-actions__btn" :loading="exporting" @click="exportScheme">
              导出<span class="scheme-actions__key">(X)</span>...
            </v-btn>
          </div>
        </div>
      </template>

      <!-- 编辑模式：色值输入网格（编辑副本实时预览） -->
      <template v-else>
        <div class="scheme-edit">
          <v-text-field
            :model-value="editing.name"
            label="方案名称"
            density="compact"
            hide-details
            class="scheme-edit__name"
            @update:model-value="(v: string) => setEditColor('name', v)"
          />
          <div class="scheme-edit__group">常规</div>
          <div class="scheme-edit__grid">
            <label v-for="f in BASE_EDIT_KEYS" :key="f.key" class="scheme-edit__row">
              <span class="scheme-edit__label">{{ f.label }}</span>
              <input
                type="color"
                class="scheme-edit__color"
                :value="(editing[f.key] as string)"
                @input="setEditColor(f.key, ($event.target as HTMLInputElement).value)"
              />
              <span class="scheme-edit__hex">{{ editing[f.key] }}</span>
            </label>
          </div>
          <div class="scheme-edit__group">ANSI 调色板</div>
          <div class="scheme-edit__grid scheme-edit__grid--ansi">
            <label v-for="f in ANSI_EDIT_KEYS" :key="f.key" class="scheme-edit__row">
              <span class="scheme-edit__label">{{ f.label }}</span>
              <input
                type="color"
                class="scheme-edit__color"
                :value="(editing[f.key] as string)"
                @input="setEditColor(f.key, ($event.target as HTMLInputElement).value)"
              />
              <span class="scheme-edit__hex">{{ editing[f.key] }}</span>
            </label>
          </div>
        </div>
        <v-card-actions>
          <v-spacer />
          <v-btn size="small" variant="text" @click="cancelEditing">取消</v-btn>
          <v-btn size="small" color="primary" variant="flat" @click="saveEditing">保存</v-btn>
        </v-card-actions>
      </template>
    </v-card>
  </v-dialog>
</template>

<style scoped>
/* ---------- 预览区（方案背景色底，inline :style 注入——v-dialog Teleport 内 CSS v-bind 不生效） ---------- */
.scheme-preview {
  padding: 10px 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-bottom: 1px solid rgba(var(--v-theme-on-surface), 0.12);
}

.scheme-preview__line {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
  font-family: var(--fy-mono);
}

.scheme-preview__item--bold {
  font-weight: 700;
}

.scheme-preview__item--underline {
  text-decoration: underline;
}

.scheme-preview__item--reversed {
  padding: 0 4px;
}

/* 光标块（方案光标色方块，模拟终端光标） */
.scheme-preview__cursor {
  display: inline-block;
  width: 10px;
  height: 14px;
  border-radius: 1px;
}

/* ---------- 方案列表 + 右侧按钮列 ---------- */
.scheme-body {
  display: flex;
  min-height: 0;
}

.scheme-list {
  flex: 1;
  min-width: 0;
  max-height: 264px;
  overflow-y: auto;
  padding: 6px;
}

.scheme-list__row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 26px;
  padding: 0 8px;
  border-radius: 4px;
  cursor: pointer;
  user-select: none;
}

.scheme-list__row:hover {
  background: rgba(var(--v-theme-on-surface), 0.06);
}

.scheme-list__row--active {
  background: rgb(var(--v-theme-primary));
}

.scheme-list__row--active:hover {
  background: rgb(var(--v-theme-primary));
}

.scheme-list__swatch {
  display: inline-block;
  width: 14px;
  height: 14px;
  border-radius: 2px;
  border: 2px solid;
  flex-shrink: 0;
}

.scheme-list__name {
  font-size: 12px;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scheme-list__row--active .scheme-list__name,
.scheme-list__row--active .scheme-list__check {
  color: rgb(var(--v-theme-on-primary));
}

.scheme-list__check {
  flex-shrink: 0;
}

.scheme-actions {
  width: 116px;
  border-left: 1px solid rgba(var(--v-theme-on-surface), 0.12);
  padding: 6px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.scheme-actions__btn {
  justify-content: flex-start;
  text-transform: none;
  letter-spacing: 0;
}

.scheme-actions__key {
  font-size: 12px;
  opacity: 0.6;
  margin-left: 1px;
}

/* ---------- 编辑模式 ---------- */
.scheme-edit {
  padding: 10px 14px;
  max-height: 420px;
  overflow-y: auto;
}

.scheme-edit__name {
  max-width: 240px;
  margin-bottom: 8px;
}

.scheme-edit__group {
  font-size: 12px;
  color: rgba(var(--v-theme-on-surface), 0.6);
  margin-bottom: 6px;
}

.scheme-edit__grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 16px;
  margin-bottom: 10px;
}

.scheme-edit__grid--ansi {
  grid-template-columns: 1fr 1fr 1fr 1fr;
}

.scheme-edit__row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 26px;
  cursor: pointer;
}

.scheme-edit__label {
  font-size: 12px;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scheme-edit__color {
  width: 32px;
  height: 22px;
  padding: 0;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.24);
  border-radius: 3px;
  background: transparent;
  cursor: pointer;
}

.scheme-edit__hex {
  font-size: 12px;
  font-family: var(--fy-mono);
  color: rgba(var(--v-theme-on-surface), 0.6);
  width: 62px;
}
</style>
