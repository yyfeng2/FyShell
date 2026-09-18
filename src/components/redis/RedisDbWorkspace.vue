<template>
  <v-card class="redis-ws" flat>
    <!-- 未连接：连接入口占位区（标题卡片 + 错误 alert + 已存连接列表 + vaultLocked + 新建连接） -->
    <div v-if="!store.isConnected" class="redis-ws__placeholder">
      <div class="redis-ws__connect-entry">
        <v-card class="redis-ws__title-card" flat variant="tonal">
          <div class="d-flex align-center">
            <v-icon size="small" color="primary" class="mr-2">mdi-database</v-icon>
            <span class="text-subtitle-1">Redis</span>
          </div>
          <div class="text-caption text-medium-emphasis mt-1">
            浏览 Keys、查看值与执行命令；选择已有连接进入，或新建连接。
          </div>
        </v-card>
        <v-alert
          v-if="store.connError"
          type="error"
          variant="tonal"
          density="compact"
          closable
          class="mt-3"
          max-width="420"
        >
          {{ store.connError }}
        </v-alert>
        <v-list density="compact" class="redis-ws__connect-list mt-3">
          <v-list-item v-for="c in store.savedConnections" :key="c.id">
            <template #prepend>
              <v-icon size="small" color="primary">mdi-database-outline</v-icon>
            </template>
            <v-list-item-title class="text-body-2">{{ c.name }}</v-list-item-title>
            <v-list-item-subtitle class="text-caption">
              {{ c.host }}:{{ c.port }} / {{ c.username ?? 'default' }}（DB {{ c.db }}）
            </v-list-item-subtitle>
            <template #append>
              <v-btn
                size="x-small"
                color="primary"
                variant="tonal"
                prepend-icon="mdi-lan-connect"
                :loading="store.connecting"
                @click.stop="connectFromSaved(c.id)"
              >
                连接
              </v-btn>
              <v-btn
                icon="mdi-delete-outline"
                size="x-small"
                variant="text"
                color="error"
                class="ml-1"
                title="删除该连接"
                @click.stop="removeSaved(c)"
              />
            </template>
          </v-list-item>
          <v-list-item v-if="store.vaultLocked">
            <template #prepend>
              <v-icon size="small">mdi-lock-outline</v-icon>
            </template>
            <v-list-item-title class="text-caption text-medium-emphasis">
              已存连接密码受主密码保护，解锁后方可查看
            </v-list-item-title>
            <template #append>
              <v-btn
                size="x-small"
                color="primary"
                variant="tonal"
                prepend-icon="mdi-lock-open-variant-outline"
                @click="showUnlockDialog = true"
              >
                解锁
              </v-btn>
            </template>
          </v-list-item>
          <v-list-item v-else-if="store.savedConnections.length === 0">
            <v-list-item-title class="text-caption text-medium-emphasis">
              暂无已保存的连接
            </v-list-item-title>
          </v-list-item>
        </v-list>
        <v-btn
          color="primary"
          variant="tonal"
          prepend-icon="mdi-plus"
          class="mt-2"
          @click="showConnForm = true"
        >
          新建连接
        </v-btn>
      </div>
    </div>

    <!-- 已连接：工作台主区（工具条 + Keys 面板 + 底部命令执行条） -->
    <template v-else>
      <div class="redis-ws__toolbar">
        <v-select
          :model-value="store.db"
          :items="dbItems"
          label="数据库"
          density="compact"
          variant="outlined"
          single-line
          hide-details
          prepend-inner-icon="mdi-database"
          class="redis-ws__db-select mr-2"
          @update:model-value="onSelectDb"
        />
        <v-btn
          size="small"
          variant="text"
          prepend-icon="mdi-refresh"
          :loading="store.keysLoading"
          title="刷新 Keys"
          @click="refreshKeys"
        >
          刷新 Keys
        </v-btn>
        <v-spacer />
        <v-btn size="small" variant="text" color="error" prepend-icon="mdi-lan-disconnect" @click="disconnectCurrent">
          断开
        </v-btn>
      </div>
      <v-divider />
      <RedisKeysPanel class="redis-ws__panel" />
      <v-divider />
      <RedisCommandBar class="redis-ws__cmdbar" />
    </template>

    <!-- 新建连接对话框 -->
    <RedisConnectionForm v-model="showConnForm" @connected="onFormConnected" />

    <!-- 凭据保险库解锁（复用通用组件） -->
    <VaultUnlockDialog v-model="showUnlockDialog" />
  </v-card>
</template>

<script setup lang="ts">
/**
 * RedisDbWorkspace —— Redis 工作台 Tab 主体
 *
 * 未连接：标题卡片 + 错误 alert + 已存连接列表（连接/删除）+ vaultLocked 解锁 + 新建连接
 * 已连接：DB(0-15) 选择器 + 刷新 Keys + 断开；Keys 面板（浏览/查看/删除 key）；底部命令执行条
 * 全部经 store（useRedisStore，契约）调用，组件不直接 invoke。
 *
 * 解锁接线说明：通用 VaultUnlockDialog 内部固定走 useMysqlStore().unlockVault（后端 vault
 * 全局共享，解锁即全局生效），仅 emit update:modelValue。因此本组件 watch 其关闭后调用
 * redis store.loadSavedConnections() 刷新已存连接与 vaultLocked（成功=加载解密的连接，
 * 取消/失败=维持锁定），无需自建 redis 解锁对话框。
 */
import { onMounted, ref, watch } from 'vue'
import { useRedisStore } from '@/stores/redis'
import { useUiStore } from '@/stores/ui'
import RedisConnectionForm from './RedisConnectionForm.vue'
import RedisKeysPanel from './RedisKeysPanel.vue'
import RedisCommandBar from './RedisCommandBar.vue'
import VaultUnlockDialog from '@/components/common/VaultUnlockDialog.vue'
import type { SavedRedisConnection } from '@/stores/redis'

const store = useRedisStore()
const ui = useUiStore()

function errText(err: unknown): string {
  return typeof err === 'string' ? err : String(err)
}

/** Redis 默认 16 个逻辑库（0-15），固定列出全部 */
const dbItems = Array.from({ length: 16 }, (_, i) => i)

const showConnForm = ref(false)
/** 凭据保险库解锁对话框（已存连接被主密码保护时触发） */
const showUnlockDialog = ref(false)

onMounted(() => {
  void store.loadSavedConnections()
})

/** 切换 DB：store.selectDb(n) 内部已刷新键列表（Keys 面板按 store.db watch 自动复位选中） */
async function onSelectDb(n: number | null): Promise<void> {
  if (n === null || n === store.db) return
  try {
    await store.selectDb(n)
    ui.toast(`已切换到 DB ${n}`, 'success')
  } catch (err) {
    ui.toast(errText(err), 'error')
  }
}

async function refreshKeys(): Promise<void> {
  await store.refreshKeys()
}

/** 连接指定已保存连接（已连接时 store 内部先断开再连）；失败信息已写入 store.connError */
async function connectFromSaved(id: string): Promise<void> {
  if (store.connecting) return
  try {
    await store.connectSaved(id)
  } catch {
    store.activeSavedId = null
  }
}

/** 断开当前连接：危险操作二次确认后断开（回到连接入口占位区） */
async function disconnectCurrent(): Promise<void> {
  if (!store.isConnected) return
  const ok = await ui.confirm({
    title: '断开连接',
    message: '确定断开当前 Redis 连接吗？',
    confirmText: '断开',
    danger: true,
  })
  if (!ok) return
  try {
    await store.disconnect()
    ui.toast('已断开 Redis 连接', 'info')
  } catch (err) {
    ui.toast(errText(err), 'error')
  }
}

/**
 * 新建连接成功（store 连接成功且未抛错）：按 host+port+username 去重回填已保存列表
 * （store.lastConfig 由 connect 写入，saveConnection 返回 id 供主动态关联）
 */
function onFormConnected(): void {
  if (store.lastConfig) {
    const cfg = store.lastConfig
    store.activeSavedId = store.saveConnection(`${cfg.username ?? ''}@${cfg.host}`, cfg)
  }
  ui.toast(`已连接 ${store.connLabel}`, 'success')
}

/** 删除已保存连接：二次确认后移除（仅本地持久化列表，不影响后端） */
async function removeSaved(c: SavedRedisConnection): Promise<void> {
  const ok = await ui.confirm({
    title: '删除连接',
    message: `确定删除已保存的连接「${c.name}」（${c.host}:${c.port}）吗？`,
    confirmText: '删除',
    danger: true,
  })
  if (!ok) return
  store.removeConnection(c.id)
  ui.toast(`已删除连接「${c.name}」`, 'success')
}

/** 解锁对话框关闭（成功/取消）后同步 redis store 的已存连接与 vaultLocked */
watch(
  () => showUnlockDialog.value,
  (open) => {
    if (!open) void store.loadSavedConnections()
  },
)
</script>

<style scoped>
.redis-ws {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

/* 顶部工具条：DB 选择器 + 刷新 + 断开 */
.redis-ws__toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  padding: 4px 8px;
  gap: 2px;
}

.redis-ws__db-select {
  flex: 0 0 130px;
  max-width: 130px;
  min-width: 110px;
}

/* Keys 面板：占满剩余高度 */
.redis-ws__panel {
  flex: 1 1 auto;
  min-height: 0;
}

/* 命令执行条：底部固定高（与 keys 面板不争夺生长空间，超高内部滚动/可压缩） */
.redis-ws__cmdbar {
  flex: 0 1 220px;
  min-height: 0;
}

/* 未连接占位：居中布局（对齐 MysqlDbWorkspace） */
.redis-ws__placeholder {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 120px;
  overflow-y: auto;
}

.redis-ws__connect-entry {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  min-width: 320px;
  max-width: 480px;
  padding: 16px 8px;
}

/* 标题卡片：轻量提示块 */
.redis-ws__title-card {
  width: 100%;
  padding: 12px 16px;
  border: 1px solid rgba(var(--v-theme-primary), 0.4);
}

/* 已保存连接列表：限高可滚动 */
.redis-ws__connect-list {
  width: 100%;
  max-height: 280px;
  overflow-y: auto;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.08);
  border-radius: 4px;
}
</style>
