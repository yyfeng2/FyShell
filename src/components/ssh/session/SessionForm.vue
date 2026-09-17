<template>
  <v-dialog
    :model-value="modelValue"
    width="min(560px, 92vw)"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="session-form">
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">{{ session ? 'mdi-pencil-box-outline' : 'mdi-plus-box-outline' }}</v-icon>
        {{ session ? '编辑会话' : '新建会话' }}
        <v-spacer />
        <!-- 标签颜色：收进标题行，节省表单一整行 -->
        <v-menu :close-on-content-click="false" location="end bottom">
          <template #activator="{ props: activatorProps }">
            <v-btn
              v-bind="activatorProps"
              size="x-small"
              variant="text"
              class="session-form__color-btn"
              title="标签颜色"
            >
              <span
                class="session-form__color-chip"
                :style="{ backgroundColor: color ?? 'transparent' }"
              />
              {{ color ? '颜色' : '未设置' }}
            </v-btn>
            <v-btn v-if="color" variant="text" size="x-small" @click="color = null">清除</v-btn>
          </template>
          <v-color-picker
            :model-value="color ?? themePrimary"
            mode="hex"
            elevation="8"
            @update:model-value="onColorChange"
          />
        </v-menu>
      </v-card-title>
      <v-divider />
      <v-card-text class="session-form__body">
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
            <v-col cols="12">
              <v-text-field v-model="name" label="名称" density="compact" :rules="[rules.required]" />
            </v-col>
            <v-col cols="8">
              <v-text-field v-model="host" label="主机" density="compact" :rules="[rules.required]" />
            </v-col>
            <v-col cols="4">
              <v-text-field
                v-model.number="port"
                label="端口"
                type="number"
                density="compact"
                :rules="[rules.required, rules.port]"
              />
            </v-col>
            <v-col cols="6">
              <v-text-field v-model="username" label="用户名" density="compact" :rules="[rules.required]" />
            </v-col>
            <v-col cols="6">
              <v-select
                v-model="authType"
                label="认证方式"
                density="compact"
                :items="AUTH_OPTIONS"
                item-title="title"
                item-value="value"
                :disabled="profileLocked"
              />
            </v-col>

            <v-col cols="12">
              <!-- 认证配置文件（P1）：选择后认证方式由配置文件接管，一处改全局生效 -->
              <div class="d-flex align-center">
                <v-select
                  v-model="profileId"
                  label="认证配置文件"
                  density="compact"
                  :items="profiles"
                  item-title="name"
                  item-value="id"
                  clearable
                  hint="选择后认证方式由配置文件接管；留空则手动填写"
                  persistent-hint
                  class="mr-2"
                />
                <v-btn size="small" variant="outlined" class="profile-manage-btn" @click="showProfileForm = true">
                  管理
                </v-btn>
              </div>
            </v-col>

            <!-- 密码 / 交互式 -->
            <v-col v-if="authType === 'password' || authType === 'interactive'" cols="12">
              <v-text-field
                v-model="password"
                label="密码"
                density="compact"
                :type="showPassword ? 'text' : 'password'"
                :rules="[rules.required]"
                :disabled="profileLocked"
                :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                @click:append-inner="showPassword = !showPassword"
              />
            </v-col>

            <!-- 私钥 -->
            <template v-if="authType === 'publicKey'">
              <v-col cols="12">
                <v-text-field
                  v-model="privateKeyPath"
                  label="私钥路径"
                  density="compact"
                  placeholder="例如 C:\Users\you\.ssh\id_rsa"
                  :rules="[rules.required]"
                  :disabled="profileLocked"
                />
              </v-col>
              <v-col cols="12">
                <v-text-field
                  v-model="passphrase"
                  label="私钥口令（可选）"
                  density="compact"
                  :type="showPassphrase ? 'text' : 'password'"
                  :disabled="profileLocked"
                  :append-inner-icon="showPassphrase ? 'mdi-eye-off' : 'mdi-eye'"
                  @click:append-inner="showPassphrase = !showPassphrase"
                />
              </v-col>
            </template>

            <!-- 不验证 -->
            <v-col v-if="authType === 'noAuth'" cols="12">
              <v-alert type="info" variant="tonal" density="compact">
                该方式不进行身份验证，仅在服务器允许匿名访问时可用。
              </v-alert>
            </v-col>

            <!-- 跳板机 -->
            <v-col v-if="authType === 'jump'" cols="12">
              <v-select
                v-model="jumpSessionId"
                label="跳板机会话"
                density="compact"
                :items="jumpCandidates"
                item-title="title"
                item-value="id"
                :rules="[rules.required]"
                :disabled="profileLocked"
              />
            </v-col>

            <v-col cols="6">
              <v-select v-model="encoding" label="编码" density="compact" :items="ENCODINGS" />
            </v-col>
            <v-col cols="6">
              <v-text-field
                v-model.number="keepalive"
                label="保活间隔（秒）"
                type="number"
                density="compact"
                :rules="[rules.required, rules.nonNegative]"
              />
            </v-col>
          </v-row>
        </v-form>

        <v-alert
          v-if="testResult"
          :type="testResult.ok ? 'success' : 'error'"
          variant="tonal"
          density="compact"
          class="mt-2"
          closable
        >
          {{ testResult.message }}
        </v-alert>
      </v-card-text>
      <v-divider />
      <v-card-actions>
        <v-btn
          variant="text"
          color="primary"
          prepend-icon="mdi-lan-connect"
          :loading="testing"
          @click="runTest"
        >
          测试连接
        </v-btn>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" :loading="saving" @click="submit">保存</v-btn>
      </v-card-actions>

      <!-- 认证配置文件管理对话框（P1），列表变更后刷新选择器 -->
      <AuthProfileForm v-model="showProfileForm" @changed="loadProfiles" />
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useTheme } from 'vuetify'
import { sessionTest } from '@/api/session'
import { authProfileList } from '@/api/authProfile'
import type { AuthProfile } from '@/api/types'
import AuthProfileForm from '@/components/ssh/session/AuthProfileForm.vue'
import { useSessionStore, type SessionConfig, type AuthType } from '@/stores/session'

type AuthTypeKind = AuthType extends { type: infer T } ? T : never

/** 契约 P1：SessionConfig 可选携带 profile_id（认证配置文件引用） */
type SessionConfigWithProfile = SessionConfig & { profile_id?: string | null }

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
  /** 编辑目标；null = 新建会话 */
  session?: SessionConfig | null
  /** 新建会话时预设的所属文件夹 id */
  folderId?: string | null
  /** 新建会话时预设的主机（快速连接地址栏预填） */
  presetHost?: string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'saved', config: SessionConfig): void
}>()

const store = useSessionStore()
const theme = useTheme()

/** 当前主题 primary 色值（颜色选择器未设置时的默认值，与应用主题色关联） */
const themePrimary = computed<string>(() => theme.current.value.colors.primary ?? '#4F8CFF')

const ENCODINGS = ['UTF-8', 'GBK', 'GB18030', 'Big5', 'Shift_JIS', 'EUC-JP', 'EUC-KR', 'ISO-8859-1', 'Windows-1251', 'KOI8-R']

const AUTH_OPTIONS: { value: AuthTypeKind; title: string }[] = [
  { value: 'password', title: '密码' },
  { value: 'publicKey', title: '私钥' },
  { value: 'interactive', title: '交互式' },
  { value: 'noAuth', title: '不验证' },
  { value: 'jump', title: '跳板机' },
]

const rules = {
  required: (v: string | number | null | undefined) =>
    (v !== null && v !== undefined && String(v).trim() !== '') || '必填项',
  port: (v: number) => (Number.isInteger(v) && v >= 1 && v <= 65535) || '端口需为 1-65535 的整数',
  nonNegative: (v: number) => (Number.isInteger(v) && v >= 0) || '需为非负整数',
}

// ---------- 表单状态 ----------
const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)
const name = ref('')
const host = ref('')
const port = ref(22)
const username = ref('')
const authType = ref<AuthTypeKind>('password')
const password = ref('')
const privateKeyPath = ref('')
const passphrase = ref('')
const jumpSessionId = ref<string | null>(null)
const encoding = ref('UTF-8')
const color = ref<string | null>(null)
const keepalive = ref(30)

const showPassword = ref(false)
/** 私钥口令明文开关（与密码框独立，避免共用一个 ref 联动切换） */
const showPassphrase = ref(false)
const testing = ref(false)
const saving = ref(false)
const testResult = ref<{ ok: boolean; message: string } | null>(null)

// ---------- 认证配置文件（P1） ----------
/** 已存配置文件列表（authProfileList） */
const profiles = ref<AuthProfile[]>([])
/** 当前选中的配置文件 id（null = 手动填写认证） */
const profileId = ref<string | null>(null)
/** 配置文件管理对话框 */
const showProfileForm = ref(false)
/** 选择配置文件后认证字段从配置解析，禁用手动编辑 */
const profileLocked = computed(() => !!findProfile())

function findProfile(): AuthProfile | undefined {
  return profiles.value.find((p) => p.id === profileId.value)
}

async function loadProfiles(): Promise<void> {
  try {
    profiles.value = await authProfileList()
  } catch {
    profiles.value = []
  }
}

watch(profileId, (id) => {
  const prof = profiles.value.find((p) => p.id === id)
  if (!prof) return
  // 从配置文件解析认证方式（深拷贝，避免与会话配置共享引用）
  const auth = JSON.parse(JSON.stringify(prof.auth_type)) as AuthType
  authType.value = auth.type
  password.value = ''
  privateKeyPath.value = ''
  passphrase.value = ''
  jumpSessionId.value = null
  if (auth.type === 'password' || auth.type === 'interactive') {
    password.value = auth.password
  } else if (auth.type === 'publicKey') {
    privateKeyPath.value = auth.private_key_path
    passphrase.value = auth.passphrase ?? ''
  } else if (auth.type === 'jump') {
    jumpSessionId.value = auth.jump_session_id
  }
})

watch(
  () => props.modelValue,
  (open) => {
    if (open) initForm()
  }
)

function initForm(): void {
  testResult.value = null
  // 重置明文开关，避免上次打开的明文状态带入下次
  showPassword.value = false
  showPassphrase.value = false
  const cfg = props.session
  if (cfg) {
    name.value = cfg.name
    host.value = cfg.host
    port.value = cfg.port
    username.value = cfg.username
    encoding.value = cfg.encoding || 'UTF-8'
    color.value = cfg.color ?? null
    keepalive.value = cfg.keepalive_interval
    const auth = cfg.auth_type
    authType.value = auth.type
    if (auth.type === 'password' || auth.type === 'interactive') {
      password.value = auth.password
    } else if (auth.type === 'publicKey') {
      privateKeyPath.value = auth.private_key_path
      passphrase.value = auth.passphrase ?? ''
    } else if (auth.type === 'jump') {
      jumpSessionId.value = auth.jump_session_id
    }
    // 契约 P1：恢复已保存的认证配置文件引用（并加载配置文件列表供选择器展示）
    profileId.value = (cfg as SessionConfigWithProfile).profile_id ?? null
    void loadProfiles()
  } else {
    name.value = ''
    host.value = props.presetHost ?? ''
    port.value = 22
    username.value = 'root'
    authType.value = 'password'
    password.value = ''
    privateKeyPath.value = ''
    passphrase.value = ''
    jumpSessionId.value = null
    encoding.value = 'UTF-8'
    color.value = null
    keepalive.value = 30
    profileId.value = null
    void loadProfiles()
  }
}

/** 跳板机候选：全部已存会话，排除自身（避免自引用） */
const jumpCandidates = computed(() =>
  store.allConfigs
    .filter((c) => !props.session || c.id !== props.session.id)
    .map((c) => ({ id: c.id, title: `${c.name}（${c.username}@${c.host}）` }))
)

/** 按当前表单状态组装契约的 SessionConfig（auth_type 为可辨识联合；选中配置文件时 profile_id 一并写入） */
function buildConfig(): SessionConfigWithProfile {
  const prof = findProfile()
  let auth: AuthType
  if (prof) {
    // 认证方式从配置文件解析（深拷贝，避免与会话配置共享引用）
    auth = JSON.parse(JSON.stringify(prof.auth_type)) as AuthType
  } else {
    switch (authType.value) {
      case 'password':
        auth = { type: 'password', password: password.value }
        break
      case 'publicKey':
        auth = { type: 'publicKey', private_key_path: privateKeyPath.value, passphrase: passphrase.value || null }
        break
      case 'interactive':
        auth = { type: 'interactive', password: password.value }
        break
      case 'noAuth':
        auth = { type: 'noAuth' }
        break
      case 'jump':
        auth = { type: 'jump', jump_session_id: jumpSessionId.value ?? '' }
        break
    }
  }
  const config: SessionConfigWithProfile = {
    id: props.session?.id ?? '',
    name: name.value.trim(),
    folder_id: props.session ? props.session.folder_id : (props.folderId ?? null),
    host: host.value.trim(),
    port: port.value,
    username: username.value.trim(),
    auth_type: auth,
    encoding: encoding.value,
    color: color.value,
    keepalive_interval: keepalive.value,
    profile_id: prof ? prof.id : null,
  }
  return config
}

function onColorChange(value: string | Record<string, number>) {
  // mode="hex" 下为字符串；防御性处理对象形态
  if (typeof value === 'string') {
    color.value = value
  } else if (value && typeof value.r === 'number') {
    color.value =
      '#' + [value.r, value.g, value.b].map((c) => Math.round(c).toString(16).padStart(2, '0')).join('')
  }
}

async function runTest(): Promise<void> {
  testing.value = true
  testResult.value = null
  try {
    testResult.value = await sessionTest(buildConfig())
  } catch (err) {
    testResult.value = { ok: false, message: `测试失败：${typeof err === 'string' ? err : String(err)}` }
  } finally {
    testing.value = false
  }
}

async function submit(): Promise<void> {
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  saving.value = true
  try {
    const saved = await store.save(buildConfig())
    emit('saved', saved)
    // 保存成功后自动关闭对话框
    emit('update:modelValue', false)
  } catch (err) {
    testResult.value = { ok: false, message: `保存失败：${typeof err === 'string' ? err : String(err)}` }
  } finally {
    saving.value = false
  }
}
</script>

<style scoped>
/* 响应式宽度由 v-dialog width="min(560px, 92vw)" 控制（写在 overlay content 上才能正确居中） */

.session-form__body {
  max-height: 56vh;
  overflow-y: auto;
}

.session-form__color-btn {
  text-transform: none;
}

/* 管理按钮：底边对齐 hint 基线（persistent-hint 占据底部一行），不用顶部魔法数字 */
.profile-manage-btn {
  align-self: flex-end;
  margin-bottom: 6px;
  text-transform: none;
}

.session-form__color-chip {
  display: inline-block;
  width: 14px;
  height: 14px;
  border-radius: 4px;
  margin-right: 6px;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.15);
  vertical-align: middle;
}
</style>
