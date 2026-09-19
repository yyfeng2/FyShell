<template>
  <div class="settings-dialog__section-title">登录脚本</div>
  <v-switch
    :model-value="opts.scriptEnabled"
    label="登录后自动执行脚本"
    color="primary"
    density="compact"
    hide-details
    class="mb-2"
    @update:model-value="(v: unknown) => opts.setScriptEnabled(!!v)"
  />
  <div class="fy-field-row">
    <span class="fy-field-row__label">脚本内容（每行一条命令）</span>
    <v-textarea
      :model-value="opts.scriptContent"
      density="compact"
      rows="6"
      class="settings-dialog__field"
      placeholder="# 注释行跳过&#10;cd /data&#10;./start.sh"
      @change="onContentChange"
    />
  </div>
  <div class="fy-field-row">
    <span class="fy-field-row__label">每行间隔（毫秒）</span>
    <v-text-field
      :model-value="opts.scriptDelay"
      type="number"
      min="50"
      max="10000"
      density="compact"
      class="settings-dialog__field"
      @change="onDelayChange"
    />
  </div>
  <div class="settings-dialog__hint">
    连接成功并进入 shell 后按行间隔逐条发送；新会话下次连接时生效。
  </div>
</template>

<script setup lang="ts">
/**
 * LoginScriptPanel —— 登录脚本（SSH 选项）
 */
import { useSshOptionsStore } from '@/stores/sshOptions'

const opts = useSshOptionsStore()

function onContentChange(e: Event): void {
  opts.setScriptContent((e.target as HTMLTextAreaElement).value)
}
function onDelayChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 50 && v <= 10000) opts.setScriptDelay(v)
}
</script>
