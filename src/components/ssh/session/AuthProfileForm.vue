<template>
  <v-dialog
    :model-value="modelValue"
    width="560"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="auth-profile-form">
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">{{ editing ? 'mdi-pencil-box-outline' : 'mdi-key-chain-variant' }}</v-icon>
        {{ editing ? '编辑认证配置文件' : '认证配置文件' }}
      <v-spacer />
      <v-btn
        icon="mdi-close"
        size="x-small"
        variant="text"
        title="关闭"
        @click="emit('update:modelValue', false)"
      />
      </v-card-title>
      <!-- 列表态错误才显示在对话框顶部；编辑态错误移入表单字段附近 -->
      <v-alert
        v-if="!editing && listError"
        type="error"
        variant="tonal"
        density="compact"
        class="mx-4 mt-2"
        closable
        @click:close="listError = null"
      >
        {{ listError }}
      </v-alert>
      <v-divider />

      <!-- 列表态：配置文件列表（名称 + 认证方式摘要） -->
      <v-card-text v-if="!editing" class="auth-profile-form__body">
        <v-list density="compact" class="auth-profile-form__list">
          <v-list-item
            v-for="p in profiles"
            :key="p.id"
            :title="p.name"
            :subtitle="describeAuth(p.auth_type)"
          >
            <template #prepend>
              <v-icon size="small">mdi-key-variant</v-icon>
            </template>
            <template #append>
              <v-btn icon="mdi-pencil" size="16" variant="text" title="编辑" @click="startEdit(p)" />
              <v-btn icon="mdi-delete" size="16" variant="text" title="删除" @click="remove(p)" />
            </template>
          </v-list-item>
          <div v-if="profiles.length === 0" class="auth-profile-form__empty">
            暂无配置文件，点击下方"新建"添加
          </div>
        </v-list>
      </v-card-text>

      <!-- 表单态：名称 + 认证方式（与 SessionForm 认证字段结构一致） -->
      <v-card-text v-else class="auth-profile-form__body">
        <v-alert
          v-if="formError"
          type="error"
          variant="tonal"
          density="compact"
          class="mb-3"
          closable
          @click:close="formError = null"
        >
          {{ formError }}
        </v-alert>
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">名称</span>
                <v-text-field v-model="name" density="compact" :rules="[rules.required]" />
              </div>
            </v-col>
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">认证方式</span>
                <v-select
                  v-model="authType"
                  density="compact"
                  :items="AUTH_OPTIONS"
                  item-title="title"
                  item-value="value"
                />
              </div>
            </v-col>

            <!-- 密码 / 交互式 -->
            <v-col v-if="authType === 'password' || authType === 'interactive'" cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">密码</span>
                <v-text-field
                  v-model="password"
                  density="compact"
                  :type="showPassword ? 'text' : 'password'"
                  :rules="[rules.required]"
                  :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                  @click:append-inner="showPassword = !showPassword"
                />
              </div>
            </v-col>

            <!-- 私钥 -->
            <template v-if="authType === 'publicKey'">
              <v-col cols="12">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">私钥路径</span>
                  <v-text-field
                    v-model="privateKeyPath"
                    density="compact"
                    placeholder="例如 C:\Users\you\.ssh\id_rsa"
                    :rules="[rules.required]"
                  />
                </div>
              </v-col>
              <v-col cols="12">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">私钥口令（可选）</span>
                  <v-text-field
                    v-model="passphrase"
                    density="compact"
                    :type="showPassphrase ? 'text' : 'password'"
                    :append-inner-icon="showPassphrase ? 'mdi-eye-off' : 'mdi-eye'"
                    @click:append-inner="showPassphrase = !showPassphrase"
                  />
                </div>
              </v-col>
            </template>

            <!-- 不验证 -->
            <v-col v-if="authType === 'noAuth'" cols="12">
              <v-alert type="info" variant="tonal" density="compact">
                该方式不进行身份验证，仅在服务器允许匿名访问时可用。
              </v-alert>
            </v-col>
          </v-row>
        </v-form>
      </v-card-text>

      <v-divider />
      <v-card-actions>
        <v-btn v-if="!editing" color="primary" variant="text" prepend-icon="mdi-plus" @click="startCreate">
          新建
        </v-btn>
        <v-spacer />
        <v-btn variant="text" @click="cancel">关闭</v-btn>
        <v-btn v-if="editing" color="primary" :loading="saving" @click="submit">保存</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * AuthProfileForm —— 认证配置文件管理对话框
 *
 * 功能（契约 5.2 auth_profile_* / 架构 §3.2）：
 * - 配置文件列表（名称 + 认证方式摘要）、新建 / 编辑 / 删除（useUiStore().confirm 二次确认）
 * - 表单：名称 + 认证方式（密码/私钥/交互式/不验证），认证字段结构与 SessionForm 一致
 * - 一处改全局生效：会话表单通过 profile_id 引用配置文件，修改后保存即全局生效
 *
 * 组件自包含：内部调 @/api/authProfile 封装层，禁止直接 invoke。
 */
import { ref, watch } from 'vue'
import { authProfileList, authProfileSave, authProfileDelete } from '@/api/authProfile'
import type { AuthProfile, AuthType } from '@/api/types'
import { useUiStore } from '@/stores/ui'

type AuthTypeKind = AuthType extends { type: infer T } ? T : never

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 列表发生变更（保存/删除成功），父组件可借此刷新配置文件选择器 */
  (e: 'changed'): void
}>()

const ui = useUiStore()

const rules = {
  required: (v: string | null | undefined) => (v !== null && v !== undefined && v.trim() !== '') || '必填项',
}

// ---------- 列表 ----------
const profiles = ref<AuthProfile[]>([])
const listError = ref<string | null>(null)
/** 表单保存错误（编辑态显示在表单字段附近，而非对话框顶部） */
const formError = ref<string | null>(null)

/** 认证方式摘要（列表副标题展示） */
function describeAuth(auth: AuthType): string {
  switch (auth.type) {
    case 'password':
      return '密码认证'
    case 'publicKey':
      return `私钥：${auth.private_key_path}`
    case 'interactive':
      return '交互式（keyboard-interactive）'
    case 'noAuth':
      return '不验证'
    case 'jump':
      return `跳板机：${auth.jump_session_id}`
  }
}

async function loadProfiles(): Promise<void> {
  listError.value = null
  formError.value = null
  try {
    profiles.value = await authProfileList()
  } catch (err) {
    profiles.value = []
    listError.value = `加载配置文件失败：${typeof err === 'string' ? err : String(err)}`
  }
}

// ---------- 表单 ----------
const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)
const editing = ref(false)
/** 正在编辑的配置文件 id；null 表示新建 */
const editingId = ref<string | null>(null)
const name = ref('')
const authType = ref<AuthTypeKind>('password')
const password = ref('')
const privateKeyPath = ref('')
const passphrase = ref('')

const showPassword = ref(false)
/** 私钥口令明文开关（与密码框独立） */
const showPassphrase = ref(false)
const saving = ref(false)

const AUTH_OPTIONS: { value: AuthTypeKind; title: string }[] = [
  { value: 'password', title: '密码' },
  { value: 'publicKey', title: '私钥' },
  { value: 'interactive', title: '交互式' },
  { value: 'noAuth', title: '不验证' },
]

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      editing.value = false
      editingId.value = null
      void loadProfiles()
    }
  }
)

function startCreate(): void {
  editing.value = true
  editingId.value = null
  formError.value = null
  name.value = ''
  authType.value = 'password'
  password.value = ''
  privateKeyPath.value = ''
  passphrase.value = ''
}

function startEdit(p: AuthProfile): void {
  editing.value = true
  editingId.value = p.id
  formError.value = null
  name.value = p.name
  authType.value = p.auth_type.type
  password.value = ''
  privateKeyPath.value = ''
  passphrase.value = ''
  if (p.auth_type.type === 'password' || p.auth_type.type === 'interactive') {
    password.value = p.auth_type.password
  } else if (p.auth_type.type === 'publicKey') {
    privateKeyPath.value = p.auth_type.private_key_path
    passphrase.value = p.auth_type.passphrase ?? ''
  }
}

/** 按当前表单状态组装认证方式（可辨识联合），与 SessionForm 的 auth_type 结构一致 */
function buildAuth(): AuthType {
  switch (authType.value) {
    case 'password':
      return { type: 'password', password: password.value }
    case 'publicKey':
      return { type: 'publicKey', private_key_path: privateKeyPath.value, passphrase: passphrase.value || null }
    case 'interactive':
      return { type: 'interactive', password: password.value }
    case 'noAuth':
      return { type: 'noAuth' }
    default:
      return { type: 'noAuth' }
  }
}

function cancel(): void {
  editing.value = false
  editingId.value = null
  emit('update:modelValue', false)
}

async function submit(): Promise<void> {
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  saving.value = true
  try {
    // 新配置文件传 id 空字符串，Rust 侧生成 uuid 并返回完整对象
    await authProfileSave({ id: editingId.value ?? '', name: name.value.trim(), auth_type: buildAuth() })
    editing.value = false
    editingId.value = null
    formError.value = null
    await loadProfiles()
    emit('changed')
  } catch (err) {
    formError.value = `保存失败：${typeof err === 'string' ? err : String(err)}`
  } finally {
    saving.value = false
  }
}

async function remove(p: AuthProfile): Promise<void> {
  const ok = await ui.confirm({
    title: '删除认证配置文件',
    message: `确定删除「${p.name}」吗？引用该配置文件的会话将回退为手动填写认证信息。`,
    danger: true,
  })
  if (!ok) return
  try {
    await authProfileDelete(p.id)
    await loadProfiles()
    emit('changed')
  } catch (err) {
    listError.value = `删除失败：${typeof err === 'string' ? err : String(err)}`
  }
}
</script>

<style scoped>
.auth-profile-form__body {
  max-height: 50vh;
  overflow-y: auto;
}

.auth-profile-form__list {
  background: transparent;
}

.auth-profile-form__empty {
  padding: 12px 8px;
  font-size: 14px;
  color: rgba(var(--v-theme-on-surface), 0.5);
  text-align: center;
}
</style>
