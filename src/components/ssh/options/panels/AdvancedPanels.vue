<template>
  <!-- 跟踪：SSH 连接诊断日志 -->
  <template v-if="page === 'trace'">
    <div class="settings-dialog__section-title">跟踪</div>
    <v-switch
      :model-value="opts.traceEnabled"
      label="SSH 连接跟踪日志"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setTraceEnabled(!!v)"
    />
    <div class="settings-dialog__hint">
      开启后在 stderr 输出连接过程详细诊断日志（连接发起、kex、认证、PTY 就绪等）；排查连接问题时使用。
    </div>
  </template>

  <!-- 响铃：终端 bell 样式 -->
  <template v-else-if="page === 'bell'">
    <div class="settings-dialog__section-title">响铃</div>
    <div class="fy-field-row">
      <span class="fy-field-row__label">响铃方式</span>
      <v-select
        :model-value="opts.bellStyle"
        :items="BELL_STYLES"
        item-title="title"
        item-value="value"
        class="settings-dialog__field"
        @update:model-value="(v: unknown) => opts.setBellStyle(v as 'off' | 'sound' | 'visual' | 'both')"
      />
    </div>
    <div class="settings-dialog__hint">
      终端收到 BEL 字符（\a）时的行为：声音提示 / 屏幕闪烁 / 两者 / 关闭；实时生效。
    </div>
  </template>

  <!-- 日志记录：新会话自动落盘 -->
  <template v-else-if="page === 'logging'">
    <div class="settings-dialog__section-title">日志记录</div>
    <v-switch
      :model-value="opts.logAutoStart"
      label="新会话自动开始记录日志"
      color="primary"
      density="compact"
      hide-details
      class="mb-2"
      @update:model-value="(v: unknown) => opts.setLogAutoStart(!!v)"
    />
    <div class="settings-dialog__hint">
      开启后新连接的 SSH 会话自动将终端输出写入日志文件（app_data/logs/&lt;会话&gt;/&lt;日期&gt;.log）；已有会话可在日志查看器中单独开关。
    </div>
  </template>
</template>

<script setup lang="ts">
/**
 * AdvancedPanels —— 跟踪 / 响铃 / 日志记录（SSH 选项）
 */
import { useSshOptionsStore } from '@/stores/sshOptions'

/** 展示页（trace | bell | logging） */
defineProps<{ page: string }>()

const opts = useSshOptionsStore()

const BELL_STYLES: { value: 'off' | 'sound' | 'visual' | 'both'; title: string }[] = [
  { value: 'off', title: '关闭' },
  { value: 'sound', title: '声音' },
  { value: 'visual', title: '视觉（屏幕闪烁）' },
  { value: 'both', title: '声音 + 视觉' },
]
</script>
