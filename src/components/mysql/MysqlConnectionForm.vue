<template>
  <v-dialog
    :model-value="modelValue"
    width="520"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card class="mysql-conn-form">
      <v-card-title class="d-flex align-center">
        <v-icon size="small" class="mr-2">mdi-database-outline</v-icon>
        连接 MySQL
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
      <v-card-text class="mysql-conn-form__body">
        <v-form ref="formRef" @submit.prevent="submit">
          <v-row dense>
            <v-col cols="7">
              <div class="fy-field-row">
                <span class="fy-field-row__label">主机</span>
                <v-text-field v-model="host" density="compact" :rules="[rules.required]" />
              </div>
            </v-col>
            <v-col cols="5">
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
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">用户名</span>
                <v-text-field v-model="username" density="compact" :rules="[rules.required]" />
              </div>
            </v-col>
            <v-col cols="12">
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
            <v-col cols="12">
              <div class="fy-field-row">
                <span class="fy-field-row__label">数据库（可选）</span>
                <v-text-field
                  v-model="schema"
                  density="compact"
                  clearable
                  placeholder="留空则连接后选择库"
                />
              </div>
            </v-col>
          </v-row>
        </v-form>

        <!-- 连接错误提示 -->
        <v-alert
          v-if="lastError"
          type="error"
          variant="tonal"
          density="compact"
          class="mt-2"
          closable
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
import { ref, watch } from 'vue'
import { useMysqlStore } from '@/stores/mysql'

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 连接成功后通知父组件（携带连接标签） */
  (e: 'connected', connLabel: string): void
}>()

const store = useMysqlStore()

const rules = {
  required: (v: string | number | null | undefined) =>
    (v !== null && v !== undefined && String(v).trim() !== '') || '必填项',
  port: (v: number) => (Number.isInteger(v) && v >= 1 && v <= 65535) || '端口需为 1-65535 的整数',
}

// ---------- 表单状态 ----------
const formRef = ref<{ validate: () => Promise<{ valid: boolean }> } | null>(null)
const host = ref('127.0.0.1')
const port = ref(3306)
const username = ref('root')
const password = ref('')
const schema = ref<string | null>(null)

const showPassword = ref(false)
const connecting = ref(false)
const lastError = ref('')

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      lastError.value = ''
    }
  }
)

/** 组装契约的 MySqlConnection（schema 为 null = 不选）并建立连接 */
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
      username: username.value.trim(),
      password: password.value,
      schema: schema.value?.trim() || null,
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
.mysql-conn-form__body {
  max-height: 60vh;
  overflow-y: auto;
  /* v-row dense 负 margin 有亚像素溢出，1px 即触发横向滚动条，直接隐藏 */
  overflow-x: hidden;
}
</style>
