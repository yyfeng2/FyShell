<template>
  <v-dialog
    :model-value="modelValue"
    max-width="480"
    @update:model-value="(v: boolean) => emit('update:modelValue', v)"
  >
    <v-card>
      <v-card-title class="d-flex align-center">
        <v-icon icon="mdi-lock-outline" size="small" class="mr-2" />
        {{ isModifyMode ? '修改主密码' : '设置主密码' }}
      </v-card-title>
      <v-card-text>
        <p class="text-body-2 mb-3">
          主密码用于保护本地存储的敏感凭据。首次设置后请牢记口令，遗忘将无法找回。
        </p>

        <!-- 修改模式：旧密码须先通过校验，防止未验证直接覆盖 -->
        <v-text-field
          v-if="isModifyMode"
          v-model="oldPassword"
          label="旧密码"
          type="password"
          density="compact"
          class="mb-2"
          :hint="oldVerified === false ? '旧密码不正确' : undefined"
          :error="oldVerified === false"
          autofocus
          @keydown.enter="verifyOld"
        >
          <template #append-inner>
            <v-btn
              size="x-small"
              variant="text"
              :disabled="!oldPassword"
              title="校验旧密码"
              @click="verifyOld"
            >
              校验
            </v-btn>
          </template>
        </v-text-field>

        <v-text-field
          v-model="newPassword"
          label="新密码"
          type="password"
          density="compact"
          class="mb-2"
          :autofocus="!isModifyMode"
          @keydown.enter="submit"
        />
        <v-text-field
          v-model="confirmPassword"
          label="确认新密码"
          type="password"
          density="compact"
          :error="!!confirmPassword && newPassword !== confirmPassword"
          @keydown.enter="submit"
        />
      </v-card-text>
      <v-card-actions>
        <v-spacer />
        <v-btn variant="text" @click="emit('update:modelValue', false)">取消</v-btn>
        <v-btn color="primary" :loading="submitting" @click="submit">确定</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
/**
 * MasterPasswordDialog —— 主密码设置对话框
 *
 * 首次设置与修改/校验二合一，打开时查询 master_password_status 决定模式：
 * - 未设置（Rust 侧返回 false）：输入新密码 + 确认密码即可设置
 * - 已设置：需先输入旧密码（可单独点"校验"即时反馈），通过后才能设置新密码
 */
import { computed, ref, watch } from 'vue'
import {
  masterPasswordSet,
  masterPasswordStatus,
  masterPasswordVerify,
} from '@/api/masterPassword'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{ (e: 'update:modelValue', value: boolean): void }>()

const ui = useUiStore()

/** 是否已设置主密码（决定首次设置/修改模式） */
const hasMaster = ref(false)
/** 提交中（防重复点击） */
const submitting = ref(false)

const oldPassword = ref('')
const newPassword = ref('')
const confirmPassword = ref('')

/** 旧密码校验结果（null=未校验） */
const oldVerified = ref<boolean | null>(null)

const isModifyMode = computed(() => hasMaster.value)

/** 打开时重置表单并查询主密码状态，决定对话框模式 */
watch(
  () => props.modelValue,
  async (value) => {
    if (!value) return
    oldPassword.value = ''
    newPassword.value = ''
    confirmPassword.value = ''
    oldVerified.value = null
    try {
      hasMaster.value = await masterPasswordStatus()
    } catch (e) {
      ui.toast(`查询主密码状态失败：${String(e)}`, 'error')
      emit('update:modelValue', false)
    }
  },
)

/** 校验旧密码（仅修改模式）：即时反馈旧密码是否正确 */
async function verifyOld(): Promise<void> {
  if (!oldPassword.value) return
  try {
    const ok = await masterPasswordVerify(oldPassword.value)
    oldVerified.value = ok
    ui.toast(ok ? '旧密码校验通过' : '旧密码不正确', ok ? 'success' : 'error')
  } catch (e) {
    ui.toast(`校验失败：${String(e)}`, 'error')
  }
}

/** 提交：两次输入一致且旧密码已通过校验后，设置新密码 */
async function submit(): Promise<void> {
  if (submitting.value) return
  if (!newPassword.value) {
    ui.toast('主密码不能为空', 'warning')
    return
  }
  if (newPassword.value !== confirmPassword.value) {
    ui.toast('两次输入的密码不一致', 'error')
    return
  }
  if (isModifyMode.value && oldVerified.value !== true) {
    ui.toast('请先校验旧密码', 'warning')
    return
  }
  submitting.value = true
  try {
    await masterPasswordSet(newPassword.value, isModifyMode.value ? oldPassword.value : undefined)
    ui.toast(isModifyMode.value ? '主密码已更新' : '主密码已设置', 'success')
    emit('update:modelValue', false)
  } catch (e) {
    ui.toast(`设置主密码失败：${String(e)}`, 'error')
  } finally {
    submitting.value = false
  }
}
</script>
