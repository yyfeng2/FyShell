/**
 * 键位映射 Pinia store
 *
 * 职责：
 * - 持有全部键位映射（key_combo -> KeyMapping），启动时从后端 SQLite 加载（key_mapping_list）
 * - 供 useXterm 的键位拦截器读取最新映射（对话框编辑后实时生效）
 * - CRUD 动作：新增/编辑走 key_mapping_save，删除走 key_mapping_delete
 */
import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import {
  keyMappingDelete,
  keyMappingList,
  keyMappingSave,
  type KeyMapping,
} from '@/api/keyMapping'

export const useKeyMappingStore = defineStore('keyMapping', () => {
  /** 全量映射列表（顺序与后端一致：按 key_combo 排序） */
  const mappings = ref<KeyMapping[]>([])
  /** key_combo -> 映射（供拦截器 O(1) 命中查询） */
  /** key_combo -> 映射（供拦截器 O(1) 命中查询） */
  const comboIndex = computed(() => {
    const idx = new Map<string, KeyMapping>()
    for (const m of mappings.value) {
      if (!idx.has(m.key_combo)) idx.set(m.key_combo, m)
    }
    return idx
  })

  let loadPromise: Promise<void> | null = null

  /** 启动时从后端加载键位映射（幂等：仅在首次调用时发起请求） */
  function ensureLoaded(): Promise<void> {
    if (loadPromise) return loadPromise
    loadPromise = (async () => {
      try {
        mappings.value = await keyMappingList()
      } catch {
        // 加载失败不阻塞 UI：保持空列表，后续变更时重新拉取
      }
    })()
    return loadPromise
  }

  /** 保存映射（新增或编辑），写回后端并刷新列表 */
  async function save(mapping: KeyMapping): Promise<void> {
    await keyMappingSave(mapping)
    await reload()
  }

  /** 删除映射，写回后端并重新拉取 */
  async function remove(id: string): Promise<void> {
    await keyMappingDelete(id)
    await reload()
  }

  /** 重新从后端加载（确保拦截器读到最新映射） */
  async function reload(): Promise<void> {
    try {
      mappings.value = await keyMappingList()
    } catch {
      // 失败时保留旧列表，不阻塞 UI
    }
  }

  return { mappings, comboIndex, ensureLoaded, save, remove, reload }
})
