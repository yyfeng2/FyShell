<template>
  <div class="settings-dialog__section-title">代理</div>
  <v-switch
    :model-value="opts.proxyEnabled"
    label="经代理连接 SSH"
    color="primary"
    density="compact"
    hide-details
    class="mb-2"
    @update:model-value="(v: unknown) => opts.setProxyEnabled(!!v)"
  />
  <v-row dense>
    <v-col cols="6">
      <div class="fy-field-row">
        <span class="fy-field-row__label">代理类型</span>
        <v-select
          :model-value="opts.proxyType"
          :items="PROXY_TYPES"
          item-title="title"
          item-value="value"
          density="compact"
          :disabled="!opts.proxyEnabled"
          @update:model-value="(v: unknown) => opts.setProxyType(v as 'socks5' | 'http')"
        />
      </div>
    </v-col>
    <v-col cols="6">
      <div class="fy-field-row">
        <span class="fy-field-row__label">代理端口</span>
        <v-text-field
          :model-value="opts.proxyPort"
          type="number"
          min="1"
          max="65535"
          density="compact"
          :disabled="!opts.proxyEnabled"
          @change="onProxyPortChange"
        />
      </div>
    </v-col>
    <v-col cols="12">
      <div class="fy-field-row">
        <span class="fy-field-row__label">代理主机</span>
        <v-text-field
          :model-value="opts.proxyHost"
          density="compact"
          placeholder="例如 127.0.0.1"
          :disabled="!opts.proxyEnabled"
          @change="onProxyHostChange"
        />
      </div>
    </v-col>
    <v-col cols="6">
      <div class="fy-field-row">
        <span class="fy-field-row__label">用户名（可选）</span>
        <v-text-field
          :model-value="opts.proxyUsername"
          density="compact"
          :disabled="!opts.proxyEnabled"
          @change="onProxyUsernameChange"
        />
      </div>
    </v-col>
    <v-col cols="6">
      <div class="fy-field-row">
        <span class="fy-field-row__label">密码（可选）</span>
        <v-text-field
          :model-value="opts.proxyPassword"
          type="password"
          density="compact"
          :disabled="!opts.proxyEnabled"
          @change="onProxyPasswordChange"
        />
      </div>
    </v-col>
  </v-row>
  <div class="settings-dialog__hint">
    所有 SSH 连接经该代理建立（SOCKS5 支持用户名/密码认证，HTTP CONNECT 支持 Basic 认证）；新连接生效。
  </div>
</template>

<script setup lang="ts">
/**
 * ProxyPanel —— SSH 连接代理（SSH 选项）
 */
import { useSshOptionsStore } from '@/stores/sshOptions'

const opts = useSshOptionsStore()

const PROXY_TYPES: { value: 'socks5' | 'http'; title: string }[] = [
  { value: 'socks5', title: 'SOCKS5' },
  { value: 'http', title: 'HTTP CONNECT' },
]

function onProxyHostChange(e: Event): void {
  opts.setProxyHost((e.target as HTMLInputElement).value.trim())
}
function onProxyPortChange(e: Event): void {
  const v = Number((e.target as HTMLInputElement).value)
  if (Number.isFinite(v) && v >= 1 && v <= 65535) opts.setProxyPort(v)
}
function onProxyUsernameChange(e: Event): void {
  opts.setProxyUsername((e.target as HTMLInputElement).value)
}
function onProxyPasswordChange(e: Event): void {
  opts.setProxyPassword((e.target as HTMLInputElement).value)
}
</script>
