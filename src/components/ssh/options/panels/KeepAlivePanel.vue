<template>
  <div class="settings-dialog__section-title">保持活动状态</div>
  <v-switch
    :model-value="opts.keepaliveEnabled"
    label="发送 keepalive 保活包"
    color="primary"
    density="compact"
    hide-details
    class="mb-2"
    @update:model-value="(v: unknown) => opts.setKeepaliveEnabled(!!v)"
  />
  <v-text-field
    :model-value="opts.keepaliveInterval"
    label="保活间隔（秒）"
    type="number"
    min="1"
    max="3600"
    density="compact"
    class="settings-dialog__field"
    @change="onIntervalChange"
  />
  <div class="settings-dialog__hint">
    关闭后所有 SSH 连接不再发送 keepalive（空闲也可能被服务器断开）；间隔为新建会话的默认值。
  </div>
</template>

<script setup lang="ts">
/**
 * KeepAlivePanel —— 保持活动状态（SSH 选项）
 */
import { useSshOptionsStore } from '@/stores/sshOptions'

const opts = useSshOptionsStore()

function onIntervalChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 1 && v <= 3600) opts.setKeepaliveInterval(v)
}
</script>
