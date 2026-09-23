<template>
  <v-dialog
    :model-value="modelValue"
    max-width="420"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-lock-outline" size="small" class="mr-2" />
        解锁凭据保险库
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
      <v-card-text>
        <p class="text-body-2 mb-3">
          已存连接密码受主密码保护，输入主密码解锁后方可查看与保存连接。
        </p>
        <div class="fy-field-row">
          <span class="fy-field-row__label">主密码</span>
          <v-text-field
            v-model="password"
            type="password"
            density="compact"
            autofocus
            :error="!!errorText"
            :persistent-hint="!!errorText"
            :hint="errorText || undefined"
            @keydown.enter="submit"
          />
        </div>
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" :loading="submitting" @click="submit">解锁</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * VaultUnlockDialog —— 凭据保险库解锁对话框
 *
 * MySQL 已存连接在设置主密码后转为密文存储，此时连接入口不可读（store.vaultLocked），
 * 用户在此输入主密码 → 后端解包 DEK（存 Rust 内存）→ 重新加载已存连接列表。
 * 密码错误时不关闭，由后端错误文案就地提示。
 */
import { ref } from 'vue'
import { useUiStore } from '@/stores/ui'
import { useMysqlStore } from '@/stores/mysql'
import { friendlyError as errText } from '@/utils/errors'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const ui = useUiStore()
const password = ref('')
const submitting = ref(false)
const errorText = ref('')

/** 提交解锁：成功后刷新已存连接列表并关闭；密码错误就地提示 */
async function submit(): Promise<void> {
  if (submitting.value) return
  if (!password.value) {
    ui.toast('请输入主密码', 'warning')
    return
  }
  submitting.value = true
  errorText.value = ''
  try {
    await useMysqlStore().unlockVault(password.value)
    ui.toast('保险库已解锁', 'success')
    password.value = ''
    emit('update:modelValue', false)
  } catch (e) {
    errorText.value = errText(e)
  } finally {
    submitting.value = false
  }
}
</script>

<style scoped>
/* rem 换算非整数档修复：text-body-2 12.25px → 12px 整数档 */
.text-body-2 {
  font-size: 12px !important;
}
</style>