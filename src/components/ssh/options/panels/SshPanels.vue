<template>
  <!-- SSH 安全性：主机 key 严格模式 + 压缩 -->
  <template v-if="page === 'ssh-security'">
    <div class="settings-dialog__section-title">SSH：安全性</div>
    <v-switch
      :model-value="opts.hostkeyStrict"
      label="主机 key 严格模式"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setHostkeyStrict(!!v)"
    />
    <div class="settings-dialog__hint">
      开启后未信任主机直接拒绝连接（不弹确认窗），仅可连接已知主机列表中的服务器；关闭时首次连接与主机 key 变更均弹窗确认。
    </div>
    <v-switch
      :model-value="opts.compression"
      label="SSH 压缩"
      color="primary"
      density="compact"
      hide-details
      class="mb-2 mt-2"
      @update:model-value="(v: unknown) => opts.setCompression(!!v)"
    />
    <div class="settings-dialog__hint">
      连接时优先协商 zlib 压缩算法，低速网络下可减少传输量，CPU 占用略增。
    </div>
  </template>

  <!-- SSH 隧道：全局自动启动 + 默认监听地址 -->
  <template v-else-if="page === 'ssh-tunnel'">
    <div class="settings-dialog__section-title">SSH：隧道</div>
    <v-switch
      :model-value="opts.tunnelAutoStart"
      label="会话连接时自动启动已启用的隧道"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setTunnelAutoStart(!!v)"
    />
    <div class="fy-field-row">
      <span class="fy-field-row__label">新隧道规则默认监听地址</span>
      <v-text-field
        :model-value="opts.tunnelListenHost"
        density="compact"
        class="settings-dialog__field"
        @change="onListenHostChange"
      />
    </div>
    <div class="settings-dialog__hint">
      全局总开关控制新会话连接时是否自动拉起已启用的隧道规则；隧道规则本身在隧道视图中管理。
    </div>
  </template>

  <!-- SSH SFTP：默认下载目录（与全局设置同一 key） -->
  <template v-else-if="page === 'ssh-sftp'">
    <div class="settings-dialog__section-title">SSH：SFTP</div>
    <div class="settings-dialog__row">
      <div class="settings-dialog__field-row">
        <div class="fy-field-row">
          <span class="fy-field-row__label">默认下载目录</span>
          <v-text-field
            :model-value="settings.sftpDownloadDir"
            density="compact"
            placeholder="留空使用系统下载目录"
            hide-details
            @change="onDownloadDirChange"
          />
        </div>
        <v-btn size="small" variant="tonal" prepend-icon="mdi-folder-open" title="选择目录" @click="pickDownloadDir">
          浏览
        </v-btn>
      </div>
      <div class="settings-dialog__hint">SFTP 下载时默认保存到该目录（与全局设置中的「SFTP」为同一项）。</div>
    </div>
  </template>
</template>

<script setup lang="ts">
/**
 * SshPanels —— SSH：安全性 / 隧道 / SFTP（SSH 选项）
 */
import { open } from '@tauri-apps/plugin-dialog'
import { useSshOptionsStore } from '@/stores/sshOptions'
import { useSettingsStore } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'
import { friendlyError } from '@/utils/errors'

/** 展示页（ssh-security | ssh-tunnel | ssh-sftp） */
defineProps<{ page: string }>()

const opts = useSshOptionsStore()
const settings = useSettingsStore()
const ui = useUiStore()

function onListenHostChange(e: Event): void {
  opts.setTunnelListenHost((e.target as HTMLInputElement).value.trim())
}

function onDownloadDirChange(e: Event): void {
  settings.setSftpDownloadDir((e.target as HTMLInputElement).value)
}

async function pickDownloadDir(): Promise<void> {
  try {
    const selected = await open({ directory: true, multiple: false, title: '选择默认下载目录' })
    if (typeof selected === 'string' && selected) {
      settings.setSftpDownloadDir(selected)
    }
  } catch (e) {
    ui.toast(`选择目录失败：${friendlyError(e)}`, 'error')
  }
}
</script>
