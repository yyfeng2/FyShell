<template>
  <!-- 用户身份验证：新建会话默认认证方式与认证配置文件（全局默认值） -->
  <template v-if="page === 'auth'">
    <div class="settings-dialog__section-title">用户身份验证</div>
    <div class="fy-field-row">
      <span class="fy-field-row__label">默认认证方式</span>
      <v-select
        :model-value="opts.defaultAuthType"
        :items="AUTH_OPTIONS"
        item-title="title"
        item-value="value"
        class="settings-dialog__field"
        @update:model-value="(v: unknown) => opts.setDefaultAuthType(String(v))"
      />
    </div>
    <div class="fy-field-row">
      <span class="fy-field-row__label">默认认证配置文件</span>
      <v-select
        :model-value="opts.defaultProfileId || null"
        :items="profileItems"
        item-title="title"
        item-value="value"
        clearable
        class="settings-dialog__field"
        @update:model-value="(v: unknown) => opts.setDefaultProfileId(v as string | null)"
      />
    </div>
    <div class="settings-dialog__hint">
      新建 SSH 会话时按以上默认值预填认证方式与配置文件；单个会话仍可在连接属性中覆盖。
    </div>
  </template>

  <!-- 登录提示符：自动响应 login:/password: 提示 -->
  <template v-else-if="page === 'login-prompt'">
    <div class="settings-dialog__section-title">登录提示符</div>
    <v-switch
      :model-value="opts.promptAutoRespond"
      label="自动响应登录提示符"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setPromptAutoRespond(!!v)"
    />
    <div class="fy-field-row">
      <span class="fy-field-row__label">自动发送的用户名</span>
      <v-text-field
        :model-value="opts.promptUsername"
        density="compact"
        class="settings-dialog__field"
        @change="onUsernameChange"
      />
    </div>
    <div class="fy-field-row">
      <span class="fy-field-row__label">自动发送的密码</span>
      <v-text-field
        :model-value="opts.promptPassword"
        type="password"
        density="compact"
        class="settings-dialog__field"
        @change="onPasswordChange"
      />
    </div>
    <div class="fy-field-row">
      <span class="fy-field-row__label">最大响应次数</span>
      <v-text-field
        :model-value="opts.promptMaxAttempts"
        type="number"
        min="1"
        max="10"
        density="compact"
        class="settings-dialog__field"
        @change="onMaxAttemptsChange"
      />
    </div>
    <div class="settings-dialog__hint">
      终端出现行尾「login:」/「username:」/「password:」提示时自动发送以上内容；密码不回显。
    </div>
  </template>
</template>

<script setup lang="ts">
/**
 * ConnectionAuthPanels —— 用户身份验证 + 登录提示符（SSH 选项）
 */
import { computed, onMounted, ref } from 'vue'
import { authProfileList } from '@/api/authProfile'
import { useSshOptionsStore } from '@/stores/sshOptions'

/** 展示页（auth | login-prompt） */
defineProps<{ page: string }>()

const opts = useSshOptionsStore()

const AUTH_OPTIONS: { value: string; title: string }[] = [
  { value: 'password', title: '密码' },
  { value: 'publicKey', title: '私钥' },
  { value: 'interactive', title: '交互式' },
  { value: 'noAuth', title: '不验证' },
  { value: 'jump', title: '跳板机' },
]

/** 认证配置文件候选（含"不使用"空项） */
const profileItems = computed(() => [
  { value: null, title: '（不使用配置文件）' },
  ...profiles.value.map((p) => ({ value: p.id, title: p.name })),
])
const profiles = ref<{ id: string; name: string }[]>([])

onMounted(async () => {
  try {
    profiles.value = await authProfileList()
  } catch {
    profiles.value = []
  }
})

function onUsernameChange(e: Event): void {
  opts.setPromptUsername((e.target as HTMLInputElement).value)
}
function onPasswordChange(e: Event): void {
  opts.setPromptPassword((e.target as HTMLInputElement).value)
}
function onMaxAttemptsChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 1 && v <= 10) opts.setPromptMaxAttempts(v)
}
</script>
