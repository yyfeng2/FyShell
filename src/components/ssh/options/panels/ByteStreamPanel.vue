<template>
  <div>
    <div class="settings-dialog__section-title">{{ panelTitle }}</div>

    <template v-if="cfg">
      <div class="settings-dialog__row">
        <span class="settings-dialog__row-title">会话名称</span>
        <div class="settings-dialog__field-row">
          <v-text-field v-model="name" density="compact" hide-details />
        </div>
      </div>

      <!-- 串口：串口下拉 + 波特率（会话级字段，无 host/port） -->
      <template v-if="type === 'serial'">
        <div class="settings-dialog__row">
          <span class="settings-dialog__row-title">串口</span>
          <div class="settings-dialog__field-row">
            <v-select
              v-model="serialPort"
              :items="portItems"
              density="compact"
              hide-details
              :loading="loadingPorts"
              placeholder="选择串口"
            />
          </div>
        </div>
        <div class="settings-dialog__row">
          <span class="settings-dialog__row-title">波特率</span>
          <div class="settings-dialog__field-row">
            <v-select v-model="baudRate" :items="BAUD_RATES" density="compact" hide-details />
          </div>
        </div>
      </template>
      <!-- Telnet / RLOGIN：主机 + 端口 -->
      <template v-else>
        <div class="settings-dialog__row">
          <span class="settings-dialog__row-title">主机地址</span>
          <div class="settings-dialog__field-row">
            <v-text-field v-model="host" density="compact" hide-details placeholder="192.168.1.1" />
          </div>
        </div>
        <div class="settings-dialog__row">
          <span class="settings-dialog__row-title">端口</span>
          <div class="settings-dialog__field-row">
            <v-text-field
              v-model.number="port"
              density="compact"
              hide-details
              type="number"
              :hint="type === 'rlogin' ? '默认 513' : '默认 23'"
              persistent-hint
            />
          </div>
        </div>
      </template>

      <div class="settings-dialog__hint">{{ hintText }}</div>

      <v-alert v-if="errorMsg" type="error" variant="tonal" density="compact" class="mt-2">
        {{ errorMsg }}
      </v-alert>

      <div class="settings-dialog__field-row" style="max-width: none; width: 100%">
        <v-btn color="primary" class="mt-4" :loading="saving" @click="save">保存</v-btn>
      </div>
    </template>
    <div v-else class="settings-dialog__hint">
      未找到会话配置（可能已被删除），无法编辑。
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * ByteStreamPanel —— Telnet / RLOGIN / 串口连接设置面板
 *
 * 编辑 byte-stream 会话的 SessionConfig 会话级字段（host/port 或 serial_port/baud_rate）
 * 并走 session_save（store.save 保存后刷新会话树）。RLOGIN 与 Telnet 协议兼容
 * （复用 services/telnet.rs，Rust 侧零改动），仅端口默认 513。
 */
import { computed, ref, watch } from 'vue'
import { serialList, type SerialPortInfo } from '@/api/serial'
import { useSessionStore, type SessionConfig } from '@/stores/session'

const props = defineProps<{
  /** 连接类型叶子：telnet / rlogin / serial */
  type: 'telnet' | 'rlogin' | 'serial'
  /** 会话模式：SessionConfig.id（仅会话模式可编辑会话字段） */
  sessionId: string
}>()

const emit = defineEmits<{
  /** 保存成功（父级提示 + 刷新会话树） */
  (e: 'saved'): void
}>()

const sessionStore = useSessionStore()

/** 常用波特率选项（串口会话使用） */
const BAUD_RATES = [9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600]

const cfg = ref<SessionConfig | null>(null)
const name = ref('')
const host = ref('')
const port = ref(0)
const serialPort = ref<string | null>(null)
const baudRate = ref(115200)
const saving = ref(false)
const errorMsg = ref('')

const loadingPorts = ref(false)
const portItems = ref<{ title: string; value: string }[]>([])

const panelTitle = computed(() =>
  props.type === 'telnet' ? 'TELNET 连接' : props.type === 'rlogin' ? 'RLOGIN 连接' : '串口连接',
)

const hintText = computed(() =>
  props.type === 'rlogin'
    ? 'RLOGIN 与 Telnet 协议兼容（FyShell 复用 Telnet 实现），端口默认 513。'
    : props.type === 'serial'
      ? '串口连接设置（会话级字段，重连后生效）。'
      : 'Telnet 连接设置（会话级字段，重连后生效）。',
)

/** 打开/切换会话时载入会话字段 */
function init(): void {
  const c = sessionStore.getSessionById(props.sessionId)
  if (!c) {
    cfg.value = null
    return
  }
  cfg.value = c
  name.value = c.name
  host.value = c.host
  port.value = c.port || (props.type === 'rlogin' ? 513 : 23)
  serialPort.value = c.serial_port ?? null
  baudRate.value = c.baud_rate ?? 115200
  errorMsg.value = ''
  if (props.type === 'serial') void loadPorts()
}

/** 枚举本机串口列表（串口类型专用；保留已选值，仅未选时默认选第一个） */
async function loadPorts(): Promise<void> {
  loadingPorts.value = true
  try {
    const ports: SerialPortInfo[] = await serialList()
    portItems.value = ports.map((p) => ({
      title: p.port_type === '未知' ? p.port_name : `${p.port_name} (${p.port_type})`,
      value: p.port_name,
    }))
    if (!serialPort.value && portItems.value.length > 0) {
      serialPort.value = portItems.value[0]!.value
    }
  } catch (e) {
    console.error('[byte-stream-panel] 枚举串口失败:', e)
    portItems.value = []
  } finally {
    loadingPorts.value = false
  }
}

watch(() => props.sessionId, init, { immediate: true })

async function save(): Promise<void> {
  const c = cfg.value
  if (!c) return
  saving.value = true
  errorMsg.value = ''
  try {
    // 串口会话无 host/port（SessionForm buildConfig 时为空值），保持原值不动
    const isSerial = props.type === 'serial'
    await sessionStore.save({
      ...c,
      name: name.value.trim(),
      host: isSerial ? c.host : host.value.trim(),
      port: isSerial ? c.port : port.value,
      serial_port: isSerial ? serialPort.value : c.serial_port,
      baud_rate: isSerial ? baudRate.value : c.baud_rate,
    })
    emit('saved')
  } catch (e) {
    errorMsg.value = `保存失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    saving.value = false
  }
}
</script>
