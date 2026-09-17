<template>
  <v-dialog :model-value="modelValue" width="420" @update:model-value="close">
    <v-card>
      <v-card-title class="text-subtitle-1">
        {{ type === 'telnet' ? 'Telnet 连接' : '串口终端' }}
      </v-card-title>
      <v-card-text>
        <v-form ref="formRef" @submit.prevent="submit">
          <template v-if="type === 'telnet'">
            <v-text-field
              v-model="host"
              label="主机地址"
              placeholder="192.168.1.1"
              density="compact"
              autofocus
              :rules="[rules.required]"
              @keyup.enter="submit"
            />
            <v-text-field
              v-model="port"
              label="端口"
              density="compact"
              hint="默认 23"
              persistent-hint
              type="number"
            />
          </template>
          <template v-else>
            <v-select
              v-model="portName"
              :items="portItems"
              label="串口"
              density="compact"
              :loading="loadingPorts"
              :rules="[rules.required]"
            />
            <v-select
              v-model="baudRate"
              :items="baudRates"
              label="波特率"
              density="compact"
              hint="默认 115200"
              persistent-hint
            />
          </template>
        </v-form>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" @click="submit">连接</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * Telnet / 串口连接表单（byte-stream 终端新建入口的最小表单）：
 * - Telnet：host/port（默认 23）
 * - 串口：串口下拉（serial_list 枚举结果）+ 波特率（默认 115200）
 * 提交后 emit saved，由父级直接打开终端 Tab（不持久化到会话树）。
 */
import { ref, watch } from 'vue'
import { serialList, type SerialPortInfo } from '@/api/serial'

const props = defineProps<{
  /** 对话框可见性（v-model） */
  modelValue: boolean
  /** 连接类型：telnet / serial */
  type: 'telnet' | 'serial'
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void
  /** 提交成功：携带连接参数（由父级打开终端 Tab） */
  (
    e: 'saved',
    params: { type: 'telnet' | 'serial'; host?: string; port?: number; serialPort?: string; baudRate?: number },
  ): void
}>()

const rules = {
  required: (v: string | null | undefined) => (v !== null && v !== undefined && v.trim() !== '') || '必填项',
}

/** 对话框关闭（含遮罩点击/ESC）：v-model 同步 */
function close(value: boolean): void {
  emit('update:modelValue', value)
}

const formRef = ref<{ validate: () => Promise<{ valid: boolean }>; resetValidation: () => void } | null>(null)

// ---------- 表单状态 ----------
const host = ref('')
const port = ref('23')
const portName = ref<string | null>(null)
const baudRate = ref(115200)
const loadingPorts = ref(false)
const ports = ref<SerialPortInfo[]>([])

/** 常用波特率选项 */
const baudRates = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600]

/** 串口下拉项（显示 "COM3 (USB)"，值为串口名） */
const portItems = ref<{ title: string; value: string }[]>([])

// 打开对话框时：串口类型预枚举本机串口
async function loadPorts(): Promise<void> {
  if (props.type !== 'serial') return
  loadingPorts.value = true
  try {
    ports.value = await serialList()
    portItems.value = ports.value.map((p) => ({
      title: p.port_type === '未知' ? p.port_name : `${p.port_name} (${p.port_type})`,
      value: p.port_name,
    }))
    // 默认选中第一个可用串口
    if (!portName.value && portItems.value.length > 0) {
      portName.value = portItems.value[0]!.value
    }
  } catch (e) {
    console.error('[byte-stream-form] 枚举串口失败:', e)
    ports.value = []
    portItems.value = []
  } finally {
    loadingPorts.value = false
  }
}

// 对话框打开时枚举串口列表（仅串口类型）
watch(
  () => props.modelValue,
  (visible) => {
    if (visible) loadPorts()
  },
  { immediate: true },
)

async function submit(): Promise<void> {
  if (formRef.value) {
    const { valid } = await formRef.value.validate()
    if (!valid) return
  }
  if (props.type === 'telnet') {
    const hostValue = host.value.trim()
    if (!hostValue) return
    const portValue = Number(port.value) || 23
    emit('saved', { type: 'telnet', host: hostValue, port: portValue })
  } else {
    if (!portName.value) return
    emit('saved', { type: 'serial', serialPort: portName.value, baudRate: baudRate.value })
  }
  emit('update:modelValue', false)
}
</script>
