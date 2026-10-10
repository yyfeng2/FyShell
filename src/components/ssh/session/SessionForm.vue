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
              title="设置标签颜色"
            >
              <span
                class="session-form__color-chip"
                :class="{ 'session-form__color-chip--empty': !color }"
                :style="{ backgroundColor: color ?? 'transparent' }"
              />
              标签颜色
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
        <v-btn
          icon="mdi-close"
          size="x-small"
          variant="text"
          title="关闭"
          @click="emit('update:modelValue', false)"
        />
      </v-card-title>
      <v-divider />
      <v-card-text class="session-form__body">
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
            <v-col cols="12">
              <div class="fy-field-row fy-field-row--required">
                <span class="fy-field-row__label">名称</span>
                <v-text-field v-model="name" density="compact" :rules="[rules.required]" />
              </div>
            </v-col>
            <v-col cols="12">
              <!-- 会话类型：SSH 或数据库（数据库会话连接后进入 MySQL 工作台） -->
              <div class="fy-field-row">
                <span class="fy-field-row__label">会话类型</span>
                <v-select
                  v-model="sessionKind"
                  density="compact"
                  :items="SESSION_KINDS"
                  item-title="title"
                  item-value="value"
                />
              </div>
            </v-col>
            <v-col v-if="(props.folderOptions?.length ?? 0) > 0" cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">所属文件夹</span>
                <v-select
                  v-model="folderId"
                  density="compact"
                  :items="props.folderOptions ?? []"
                  item-title="title"
                  item-value="value"
                  clearable
                  placeholder="根级"
                />
              </div>
            </v-col>
            <v-col v-if="sessionKind !== 'serial'" cols="12" sm="8">
              <div class="fy-field-row">
                <span class="fy-field-row__label">主机</span>
                <v-text-field v-model="host" density="compact" :rules="[rules.required]" />
              </div>
            </v-col>
            <v-col v-if="sessionKind !== 'serial'" cols="12" sm="4">
              <div class="fy-field-row">
                <span class="fy-field-row__label">端口</span>
                <v-text-field
                  v-model.number="port"
                  type="number"
                  density="compact"
                  :rules="[rules.required, rules.port]"
                />
              </div>
            </v-col>
            <v-col v-if="sessionKind === 'ssh' || sessionKind === 'sftp' || sessionKind === 'mysql' || sessionKind === 'redis'" cols="12" sm="6">
              <!-- Redis 用户名可空（RedisConnection.username 为 string|null），ssh/sftp/mysql 仍必填 -->
              <div class="fy-field-row" :class="{ 'fy-field-row--required': sessionKind !== 'redis' }">
                <span class="fy-field-row__label">用户名</span>
                <v-text-field
                  v-model="username"
                  density="compact"
                  placeholder="例如 root"
                  :rules="sessionKind === 'redis' ? [] : [rules.required]"
                />
              </div>
            </v-col>
            <v-col v-if="sessionKind === 'ssh' || sessionKind === 'sftp'" cols="12" sm="6">
              <div class="fy-field-row">
                <span class="fy-field-row__label">认证方式</span>
                <v-select
                  v-model="authType"
                  density="compact"
                  :items="AUTH_OPTIONS"
                  item-title="title"
                  item-value="value"
                  :disabled="profileLocked"
                />
              </div>
            </v-col>

            <!-- 串口会话：串口下拉 + 波特率（host/port 不适用） -->
            <v-col v-if="sessionKind === 'serial'" cols="12">
              <div class="fy-field-row fy-field-row--required">
                <span class="fy-field-row__label">串口</span>
                <v-select
                  v-model="serialPortName"
                  :items="portItems"
                  density="compact"
                  :loading="loadingPorts"
                  :rules="[rules.required]"
                />
              </div>
            </v-col>
            <v-col v-if="sessionKind === 'serial'" cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">波特率</span>
                <v-select
                  v-model="serialBaud"
                  :items="baudRates"
                  density="compact"
                  hint="默认 115200"
                  persistent-hint
                />
              </div>
            </v-col>

            <v-col v-if="(sessionKind === 'ssh' || sessionKind === 'sftp') && authType === 'publicKey'" cols="12">
              <!-- 认证配置文件（P1）：仅私钥认证时显示；选择后认证方式由配置文件接管，一处改全局生效 -->
              <div class="d-flex align-start">
                <div class="fy-field-row mr-2 session-form__profile-row">
                  <span class="fy-field-row__label">认证配置文件</span>
                  <v-select
                    v-model="profileId"
                    :items="profiles"
                    item-title="name"
                    item-value="id"
                    clearable
                    hint="选择后认证方式由配置文件接管；留空则手动填写"
                    persistent-hint
                  />
                </div>
                <v-btn size="small" variant="outlined" class="profile-manage-btn" @click="showProfileForm = true">
                  管理
                </v-btn>
              </div>
            </v-col>

            <!-- 密码 / 交互式 -->
            <v-col
              v-if="sessionKind === 'mysql' || sessionKind === 'redis' || ((sessionKind === 'ssh' || sessionKind === 'sftp') && (authType === 'password' || authType === 'interactive'))"
              cols="12"
            >
              <div class="fy-field-row">
                <span class="fy-field-row__label">密码</span>
                <v-text-field
                  v-model="password"
                  density="compact"
                  :type="showPassword ? 'text' : 'password'"
                  :rules="sessionKind === 'redis' ? [] : [rules.required]"
                  :disabled="profileLocked"
                  :hint="capsLockOn ? '大写锁定已开启' : ''"
                  persistent-hint
                  :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                  @click:append-inner="showPassword = !showPassword"
                  @keydown="checkCapsLock"
                  @keyup="checkCapsLock"
                />
              </div>
            </v-col>

            <!-- 私钥 -->
            <template v-if="sessionKind === 'ssh' && authType === 'publicKey'">
              <v-col cols="12">
                <div class="fy-field-row fy-field-row--required">
                  <span class="fy-field-row__label">私钥路径</span>
                  <v-text-field
                    v-model="privateKeyPath"
                    density="compact"
                    placeholder="例如 C:\Users\you\.ssh\id_rsa"
                    :rules="[rules.required]"
                    :disabled="profileLocked"
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
                    :disabled="profileLocked"
                    :hint="capsLockOn ? '大写锁定已开启' : ''"
                    persistent-hint
                    :append-inner-icon="showPassphrase ? 'mdi-eye-off' : 'mdi-eye'"
                    @click:append-inner="showPassphrase = !showPassphrase"
                    @keydown="checkCapsLock"
                    @keyup="checkCapsLock"
                  />
                </div>
              </v-col>
            </template>

            <!-- 不验证 -->
            <v-col v-if="sessionKind === 'ssh' && authType === 'noAuth'" cols="12">
              <v-alert type="info" variant="tonal" density="compact">
                该方式不进行身份验证，仅在服务器允许匿名访问时可用。
              </v-alert>
            </v-col>

            <!-- 跳板机 -->
            <v-col v-if="sessionKind === 'ssh' && authType === 'jump'" cols="12">
              <div class="fy-field-row fy-field-row--required">
                <span class="fy-field-row__label">跳板机会话</span>
                <v-select
                  v-model="jumpSessionId"
                  density="compact"
                  :items="jumpCandidates"
                  item-title="title"
                  item-value="id"
                  :rules="[rules.required]"
                  :disabled="profileLocked"
                />
              </div>
            </v-col>

            <template v-if="sessionKind === 'ssh' || sessionKind === 'sftp'">
              <v-col cols="12" sm="6">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">编码</span>
                  <v-select v-model="encoding" density="compact" :items="ENCODINGS" />
                </div>
              </v-col>
              <v-col cols="12" sm="6">
                <div class="fy-field-row">
                  <span class="fy-field-row__label">保活间隔（秒）</span>
                  <v-text-field
                    v-model.number="keepalive"
                    type="number"
                    density="compact"
                    :rules="[rules.required, rules.keepaliveRange]"
                  />
                </div>
              </v-col>
            </template>

            <!-- 备注/说明（可选，打开会话列表"说明"列展示） -->
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">说明（可选）</span>
                <v-text-field v-model="description" density="compact" />
              </div>
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
import { mysqlConnect, mysqlDisconnect } from '@/api/mysql'
import { redisTest } from '@/api/redis'
import { telnetConnect, telnetDisconnect } from '@/api/telnet'
import { serialConnect, serialDisconnect, serialList, type SerialPortInfo } from '@/api/serial'
import type { AuthProfile } from '@/api/types'
import AuthProfileForm from '@/components/ssh/session/AuthProfileForm.vue'
import { useSessionStore, ENCODINGS, type SessionConfig, type AuthType } from '@/stores/session'
import { useUiStore } from '@/stores/ui'
import { friendlyError as errText } from '@/utils/errors'

type AuthTypeKind = AuthType extends { type: infer T } ? T : never

/** 会话类型：SSH 终端 / 数据库（MySQL/Redis）/ Telnet 兼容（Telnet/RLOGIN）/ 串口 */
type SessionKind = 'ssh' | 'mysql' | 'redis' | 'telnet' | 'rlogin' | 'serial' | 'sftp'

/** 契约 P1：SessionConfig 可选携带 profile_id（认证配置文件引用） */
type SessionConfigWithProfile = SessionConfig & { profile_id?: string | null }

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
  /** 编辑目标；null = 新建会话 */
  session?: SessionConfig | null
  /** 新建会话时预设的所属文件夹 id */
  folderId?: string | null
  /** 所属文件夹候选（值为文件夹 id，按树序带层级缩进；不含根级——根级由清空表达）。
      空数组时隐藏「所属文件夹」控件（如组件无会话树数据源的调用场景） */
  folderOptions?: { value: string; title: string }[]
  /** 新建会话时预设的主机（快速连接地址栏预填） */
  presetHost?: string
  /** 新建会话时预设的会话类型（如 SFTP 独立会话入口；未指定缺省 SSH） */
  defaultType?: 'ssh' | 'sftp'
  /** 保存入口覆盖（编辑已保存连接等非会话场景）：提供时替代 session store 落盘 */
  saveHandler?: (cfg: SessionConfigWithProfile) => Promise<SessionConfig>
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  (e: 'saved', config: SessionConfig): void
}>()

const store = useSessionStore()
const ui = useUiStore()
const theme = useTheme()

/** 当前主题 primary 色值（颜色选择器未设置时的默认值，与应用主题色关联） */
const themePrimary = computed<string>(() => theme.current.value.colors.primary ?? '#2E6FDB')

const AUTH_OPTIONS: { value: AuthTypeKind; title: string }[] = [
  { value: 'password', title: '密码' },
  { value: 'publicKey', title: '私钥' },
  { value: 'interactive', title: '交互式' },
  { value: 'noAuth', title: '不验证' },
  { value: 'jump', title: '跳板机' },
]

/** 会话类型：SSH 终端 / 独立 SFTP / 数据库（MySQL/Redis）/ Telnet（含 RLOGIN 兼容）/ 串口 */
const SESSION_KINDS: { value: SessionKind; title: string }[] = [
  { value: 'ssh', title: 'SSH' },
  { value: 'sftp', title: 'SFTP 文件传输' },
  { value: 'mysql', title: '数据库 (MySQL)' },
  { value: 'redis', title: '数据库 (Redis)' },
  { value: 'telnet', title: 'Telnet' },
  { value: 'rlogin', title: 'RLOGIN' },
  { value: 'serial', title: '串口' },
]

const rules = {
  required: (v: string | number | null | undefined) =>
    (v !== null && v !== undefined && String(v).trim() !== '') || '必填项',
  port: (v: number) => (Number.isInteger(v) && v >= 1 && v <= 65535) || '端口需为 1-65535 的整数',
  keepaliveRange: (v: number) =>
    (Number.isInteger(v) && v >= 0 && v <= 3600) || '保活间隔需为 0-3600 秒的整数',
}

// ---------- 表单状态 ----------
const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)
const name = ref('')
const host = ref('')
const port = ref(22)
/** 所属文件夹 id（null = 根级）；保存时写入 SessionConfig.folder_id */
const folderId = ref<string | null>(null)
/** 会话类型（新建缺省 SSH；编辑从 session_type 恢复） */
const sessionKind = ref<SessionKind>('ssh')
const username = ref('')
const authType = ref<AuthTypeKind>('password')
const password = ref('')
const privateKeyPath = ref('')
const passphrase = ref('')
const jumpSessionId = ref<string | null>(null)
const encoding = ref('UTF-8')
const color = ref<string | null>(null)
const keepalive = ref(30)
/** 备注/说明（可选，打开会话列表"说明"列展示） */
const description = ref('')

// ---------- 串口会话字段（sessionKind === 'serial' 时生效） ----------
const serialPortName = ref<string | null>(null)
const serialBaud = ref(115200)
const loadingPorts = ref(false)
const ports = ref<SerialPortInfo[]>([])
/** 串口下拉项（显示 "COM3 (USB)"，值为串口名） */
const portItems = ref<{ title: string; value: string }[]>([])
/** 常用波特率选项 */
const baudRates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600]

const showPassword = ref(false)
/** 私钥口令明文开关（与密码框独立，避免共用一个 ref 联动切换） */
const showPassphrase = ref(false)
/** Caps Lock 开启提醒（密码/私钥口令共享，同一时刻仅一个聚焦） */
const capsLockOn = ref(false)
/** 键事件读修饰键状态：开启时 hint 显示提醒 */
function checkCapsLock(e: KeyboardEvent): void {
  capsLockOn.value = e.getModifierState?.('CapsLock') ?? false
}
const testing = ref(false)
const saving = ref(false)
const testResult = ref<{ ok: boolean; message: string } | null>(null)

/** 统一写入测试结果：alert 展示详细消息 + toast 即时通知 */
function setTestResult(result: { ok: boolean; message: string } | null): void {
  testResult.value = result
  if (result) ui.toast(result.message, result.ok ? 'success' : 'error')
}

// ---------- 认证配置文件（P1） ----------
/** 已存配置文件列表（authProfileList） */
const profiles = ref<AuthProfile[]>([])
/** 当前选中的配置文件 id（null = 手动填写认证） */
const profileId = ref<string | null>(null)
/** 配置文件管理对话框 */
const showProfileForm = ref(false)
/** 选择配置文件（且处于私钥认证）时认证字段从配置解析，禁用手动编辑；
    认证方式切走后字段隐藏且不接管，用户可自由改回 */
const profileLocked = computed(() => !!findProfile() && authType.value === 'publicKey')

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

/** 新建时切换类型联动默认端口（编辑模式端口由用户掌控；串口无端口） */
watch(sessionKind, (kind) => {
  if (props.session) return
  if (kind === 'serial') return
  port.value =
    kind === 'mysql' ? 3306 : kind === 'redis' ? 6379 : kind === 'telnet' ? 23 : kind === 'rlogin' ? 513 : 22
})

/** session_type 字符串 → 表单类型（未知类型回退 SSH） */
function mapSessionKind(t: string | null | undefined): SessionKind {
  if (t === 'mysql' || t === 'redis' || t === 'telnet' || t === 'rlogin' || t === 'serial' || t === 'sftp') return t
  return 'ssh'
}

/** 枚举本机串口列表（串口类型表单下拉用；编辑模式保留已选值，仅未选时默认选第一个） */
async function loadSerialPorts(): Promise<void> {
  if (sessionKind.value !== 'serial') return
  loadingPorts.value = true
  try {
    ports.value = await serialList()
    portItems.value = ports.value.map((p) => ({
      title: p.port_type === '未知' ? p.port_name : `${p.port_name} (${p.port_type})`,
      value: p.port_name,
    }))
    if (!serialPortName.value && portItems.value.length > 0) {
      serialPortName.value = portItems.value[0]!.value
    }
  } catch (e) {
    console.error('[session-form] 枚举串口失败:', e)
    ports.value = []
    portItems.value = []
  } finally {
    loadingPorts.value = false
  }
}

function initForm(): void {
  testResult.value = null
  // 重置明文开关，避免上次打开的明文状态带入下次
  showPassword.value = false
  showPassphrase.value = false
  const cfg = props.session
  if (cfg) {
    sessionKind.value = mapSessionKind(cfg.session_type)
    name.value = cfg.name
    host.value = cfg.host
    port.value = cfg.port
    username.value = cfg.username
    encoding.value = cfg.encoding || 'UTF-8'
    color.value = cfg.color ?? null
    keepalive.value = cfg.keepalive_interval
    serialPortName.value = cfg.serial_port ?? null
    serialBaud.value = cfg.baud_rate ?? 115200
    description.value = cfg.description ?? ''
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
    folderId.value = cfg.folder_id ?? null
    void loadProfiles()
  } else {
    name.value = ''
    host.value = props.presetHost ?? ''
    port.value = 22
    sessionKind.value = props.defaultType ?? 'ssh'
    // 默认留空避免无脑 root（placeholder 提示输入格式，必填校验拦截空值）
    username.value = ''
    authType.value = 'password'
    password.value = ''
    privateKeyPath.value = ''
    passphrase.value = ''
    jumpSessionId.value = null
    encoding.value = 'UTF-8'
    color.value = null
    keepalive.value = 30
    serialPortName.value = null
    serialBaud.value = 115200
    description.value = ''
    profileId.value = null
    folderId.value = props.folderId ?? null
    void loadProfiles()
  }
  void loadSerialPorts()
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
  /** 配置文件仅在私钥认证下生效（与字段显示时机一致，切走后手动认证生效） */
  const profileActive = prof !== undefined && (sessionKind.value === 'ssh' || sessionKind.value === 'sftp') && authType.value === 'publicKey'
  let auth: AuthType
  if (profileActive) {
    // 认证方式从配置文件解析（深拷贝，避免与会话配置共享引用）
    auth = JSON.parse(JSON.stringify(prof.auth_type)) as AuthType
  } else if (sessionKind.value === 'mysql' || sessionKind.value === 'redis') {
    // 数据库会话（MySQL/Redis）：认证即用户名/密码（auth_type 仅作存储载体）
    auth = { type: 'password', password: password.value }
  } else if (sessionKind.value !== 'ssh' && sessionKind.value !== 'sftp') {
    // byte-stream 会话（telnet/rlogin/serial）：无 SSH 认证概念，auth_type 仅作存储载体
    auth = { type: 'noAuth' }
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
  const isSerial = sessionKind.value === 'serial'
  const config: SessionConfigWithProfile = {
    id: props.session?.id ?? '',
    name: name.value.trim(),
    folder_id: folderId.value,
    host: isSerial ? '' : host.value.trim(),
    port: isSerial ? 0 : port.value,
    username: isSerial ? '' : username.value.trim(),
    auth_type: auth,
    encoding: encoding.value,
    color: color.value,
    keepalive_interval: keepalive.value,
    profile_id: profileActive ? prof.id : null,
    session_type: sessionKind.value,
    serial_port: isSerial ? serialPortName.value : null,
    baud_rate: isSerial ? serialBaud.value : null,
    description: description.value.trim() || null,
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
  // 与 submit 同源的必填/范围校验：空用户名/坏端口直接提示，不发无效连接
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  // 数据库会话：mysql_connect 建连后立即断开作为测试
  if (sessionKind.value === 'mysql') {
    testing.value = true
    testResult.value = null
    try {
      const cfg = buildConfig()
      const connId = await mysqlConnect({
        host: cfg.host,
        port: cfg.port,
        username: cfg.username,
        password: password.value,
        schema: null,
        charset: null,
      })
      await mysqlDisconnect(connId)
      setTestResult({ ok: true, message: '连接成功' })
    } catch (e) {
      setTestResult({ ok: false, message: `连接失败：${errText(e)}` })
    } finally {
      testing.value = false
    }
    return
  }
  // 数据库会话（Redis）：redis_test 建连即断（后端内部自断，无需手动断开）
  if (sessionKind.value === 'redis') {
    testing.value = true
    testResult.value = null
    try {
      const cfg = buildConfig()
      // RedisConnection：用户名/密码可空（空串归一化为 null）
      await redisTest({
        host: cfg.host,
        port: cfg.port,
        username: username.value.trim() ? username.value.trim() : null,
        password: password.value ? password.value : null,
        db: 0,
      })
      setTestResult({ ok: true, message: '连接成功' })
    } catch (e) {
      setTestResult({ ok: false, message: `连接失败：${errText(e)}` })
    } finally {
      testing.value = false
    }
    return
  }
  // byte-stream 会话（telnet/rlogin/serial）：telnet/serial 建连后立即断开作为测试
  //（独立 SFTP 会话不进此分支：SFTP 基于 SSH，落末尾的 sessionTest 走 SSH 测试路径）
  if (sessionKind.value !== 'ssh' && sessionKind.value !== 'sftp') {
    testing.value = true
    testResult.value = null
    const id = `test-${Date.now()}`
    try {
      if (sessionKind.value === 'serial') {
        if (!serialPortName.value) throw new Error('未选择串口')
        await serialConnect(id, serialPortName.value, serialBaud.value, () => {})
        await serialDisconnect(id)
      } else {
        await telnetConnect(id, host.value.trim(), port.value, () => {})
        await telnetDisconnect(id)
      }
      setTestResult({ ok: true, message: '连接成功' })
    } catch (e) {
      setTestResult({ ok: false, message: `连接失败：${errText(e)}` })
    } finally {
      testing.value = false
    }
    return
  }
  testing.value = true
  testResult.value = null
  try {
    setTestResult(await sessionTest(buildConfig()))
  } catch (e) {
    setTestResult({ ok: false, message: `测试失败：${errText(e)}` })
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
    // saveHandler 覆盖时走调用方保存链路（编辑已保存连接等非会话场景），否则落 session store
    const saved = props.saveHandler ? await props.saveHandler(buildConfig()) : await store.save(buildConfig())
    emit('saved', saved)
    // 保存成功后自动关闭对话框
    emit('update:modelValue', false)
  } catch (e) {
    testResult.value = { ok: false, message: `保存失败：${errText(e)}` }
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
  /* 裁剪 v-row dense 负左右 margin 造成的 8px 溢出（横向滚动条根因） */
  overflow-x: hidden;
}

.session-form__color-btn {
  text-transform: none;
}

/* 管理按钮：与认证配置文件下拉框顶对齐（固定 26px 高 ≈ 27px 字段框；
   Vuetify small+compact 高度计算 24-8=16px 太矮，且 flex-end 对齐 hint 错位） */
.profile-manage-btn {
  align-self: flex-start;
  height: 26px;
  text-transform: none;
}

/* 认证配置文件行：fy-field-row 在 d-flex 内撑满剩余宽度（管理按钮在右） */
.session-form__profile-row {
  flex: 1 1 auto;
  min-width: 0;
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

/* 未设置颜色：虚线边框（与复选框区分，表达"未配置"语义） */
.session-form__color-chip--empty {
  border: 1px dashed rgba(var(--v-theme-on-surface), 0.4);
}
</style>
