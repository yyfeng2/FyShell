<template>
  <v-dialog
    :model-value="modelValue"
    width="520"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="redis-conn-form">
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-database-outline</v-icon>
        连接 Redis
      </v-card-title>
      <v-divider />
      <v-card-text class="redis-conn-form__body">
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
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
            <v-col cols="12">
              <v-text-field
                v-model="username"
                label="用户名（ACL，可空）"
                density="compact"
                clearable
                placeholder="留空 = 默认用户"
              />
            </v-col>
            <v-col cols="12">
              <v-text-field
                v-model="password"
                label="密码（可空）"
                density="compact"
                clearable
                :type="showPassword ? 'text' : 'password'"
                :append-inner-icon="showPassword ? 'mdi-eye-off' : 'mdi-eye'"
                @click:append-inner="showPassword = !showPassword"
              />
            </v-col>
            <v-col cols="12">
              <v-text-field
                v-model.number="db"
                label="数据库"
                type="number"
                density="compact"
                :rules="[rules.db]"
                hint="内存数据库编号 (0-15)"
                persistent-hint
              />
            </v-col>
          </v-row>
        </v-form>

        <!-- 连接错误提示（失败不关窗） -->
        <v-alert
          v-if="lastError"
          type="error"
          variant="tonal"
          density="compact"
          class="mt-2"
          closable
          @click:close="lastError = ''"
        >
          {{ lastError }}
        </v-alert>
      </v-card-text>
      <v-divider />
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" prepend-icon="mdi-lan-connect" :loading="connecting" @click="submit">
          连接
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * RedisConnectionForm —— Redis 连接表单对话框
 *
 * 对齐 MysqlConnectionForm：v-form 校验 → store.connect → 成功 emit('connected', connLabel)
 * 并关窗；失败展示 store.connError 且不关窗。host/port 必填，username/password 可空
 * （空串归一为 null = 无认证，透传给后端），db 为 0-15 整数（Redis 默认 16 个逻辑库）。
 */
import { ref, watch } from 'vue'
import { useRedisStore } from '@/stores/redis'

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 连接成功后通知父组件（携带连接标签） */
  (e: 'connected', connLabel: string): void
}>()

const store = useRedisStore()

const rules = {
  required: (v: string | number | null | undefined) =>
    (v !== null && v !== undefined && String(v).trim() !== '') || '必填项',
  port: (v: number) => (Number.isInteger(v) && v >= 1 && v <= 65535) || '端口需为 1-65535 的整数',
  db: (v: number | string | null | undefined) => {
    if (v === null || v === undefined || v === '') return '数据库编号 (0-15) 必填'
    const n = Number(v)
    return (Number.isInteger(n) && n >= 0 && n <= 15) || '数据库编号需为 0-15 的整数'
  },
}

// ---------- 表单状态 ----------
const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)
const host = ref('127.0.0.1')
const port = ref(6379)
const username = ref('')
const password = ref('')
const db = ref(0)

const showPassword = ref(false)
const connecting = ref(false)
const lastError = ref('')

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      lastError.value = ''
    }
  },
)

/** 组装契约的 RedisConnection（username/password 空串归一为 null）并建立连接 */
async function submit(): Promise<void> {
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  connecting.value = true
  try {
    await store.connect({
      host: host.value.trim(),
      port: port.value,
      username: username.value.trim() || null,
      password: password.value || null,
      db: db.value,
    })
    emit('connected', store.connLabel)
    emit('update:modelValue', false)
  } catch {
    // 连接错误已写入 store.connError，取最新值展示
    lastError.value = store.connError
  } finally {
    connecting.value = false
  }
}
</script>

<style scoped>
.redis-conn-form__body {
  max-height: 60vh;
  overflow-y: auto;
}
</style>
