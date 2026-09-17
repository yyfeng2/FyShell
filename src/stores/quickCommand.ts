/**
 * 快捷命令 Pinia store（契约第 5.2 节：commands/quick_command.rs ↔ api/quickCommand.ts）
 *
 * 职责：
 * - 加载快捷命令树（qc_list，返回扁平全量列表，此处按 groupId/parentId 组装成树）
 * - 命令/文件夹 CRUD（qc_save_command / qc_save_folder / qc_delete，删除由组件层二次确认）
 * - 搜索过滤（按命令名/命令正文关键字递归过滤）
 *
 * 类型从 @/api/quickCommand 复用（该文件由另一智能体按契约创建）；
 * 运行时做归一化兜底，兼容 camelCase / snake_case 两种字段形态。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { qcList, qcSaveCommand, qcSaveFolder, qcDelete } from '@/api/quickCommand'
import type {
  QuickCommand,
  QuickCommandFolder,
  QuickCommandNode,
} from '@/api/types'

type UnknownRecord = Record<string, unknown>

/** 归一化取字符串字段：兼容 camelCase 与 snake_case */
function pickField(raw: UnknownRecord, camel: string, snake: string): string {
  const v = raw[camel] ?? raw[snake]
  return v == null ? '' : String(v)
}

/** 归一化单个节点：kind 缺失时按有无 children / commandText 推断 */
function normalizeNode(raw: unknown): QuickCommandNode | null {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as UnknownRecord
  const id = String(r.id ?? '')
  if (!id) return null

  const looksFolder = Array.isArray(r.children)
  const kind: 'folder' | 'command' =
    r.kind === 'folder' || r.kind === 'command'
      ? (r.kind as 'folder' | 'command')
      : looksFolder
        ? 'folder'
        : 'command'

  if (kind === 'folder') {
    return {
      kind: 'folder',
      id,
      name: String(r.name ?? ''),
      parent_id: (r.parent_id as string | null) ?? null,
    } as unknown as QuickCommandNode
  }
  return {
    kind: 'command',
    id,
    name: String(r.name ?? ''),
    command_text: pickField(r, 'commandText', 'command_text'),
    group_id: (r.groupId ?? r.group_id ?? null) as string | null,
  } as unknown as QuickCommandNode
}

/** 判断节点是否为文件夹（容错：kind / children 两种形态） */
export function isCommandFolder(node: QuickCommandNode): boolean {
  return (node as unknown as { kind?: string }).kind === 'folder'
}

/** 取命令节点的命令正文（文件夹节点返回空串） */
export function commandTextOf(node: QuickCommandNode): string {
  return isCommandFolder(node) ? '' : (node as unknown as QuickCommand).command_text
}

/** 文件夹节点的子节点（安全返回空数组） */
export function folderChildrenOf(node: QuickCommandNode): QuickCommandNode[] {
  const children = (node as unknown as UnknownRecord).children
  return Array.isArray(children) ? (children as QuickCommandNode[]) : []
}

/** 组树用的内部文件夹：带 children 的 QuickCommandFolder */
interface FolderWithChildren extends QuickCommandFolder {
  children: QuickCommandNode[]
}

/**
 * 扁平列表组树：文件夹按 parentId 嵌套、命令按 groupId 挂载，孤儿节点按根级处理。
 * 文件夹在前、命令在后，同级按名称排序。
 */
function buildTree(list: QuickCommandNode[]): QuickCommandNode[] {
  const folders = new Map<string, QuickCommandFolder>()
  const commands: QuickCommand[] = []

  for (const raw of list) {
    const node = normalizeNode(raw) ?? (raw as unknown as QuickCommandNode)
    if (isCommandFolder(node)) {
      const f = node as unknown as QuickCommandFolder
      folders.set(f.id, { id: f.id, name: f.name, parent_id: f.parent_id ?? null })
    } else {
      const c = node as unknown as QuickCommand
      commands.push({
        id: c.id,
        name: c.name,
        command_text: c.command_text ?? '',
        group_id: c.group_id ?? null,
      })
    }
  }

  const roots: QuickCommandNode[] = []
  const rootFolders: FolderWithChildren[] = []
  const folderChildren = new Map<string, FolderWithChildren[]>()

  // 文件夹按 parentId 分桶；parentId 指向不存在的文件夹时按根级处理，避免节点丢失
  for (const folder of folders.values()) {
    const parent = folder.parent_id ? folders.get(folder.parent_id) : undefined
    const bucket: FolderWithChildren = { ...folder, children: [] }
    if (parent) {
      const children = folderChildren.get(parent.id) ?? []
      children.push(bucket)
      folderChildren.set(parent.id, children)
    } else {
      rootFolders.push(bucket)
    }
  }

  // 命令按 groupId 挂载；group_id 悬空（分组已删/不存在）时按根级处理
  const commandByGroup = new Map<string, QuickCommand[]>()
  const rootCommands: QuickCommand[] = []
  for (const cmd of commands) {
    const owner = cmd.group_id ? folders.get(cmd.group_id) : undefined
    if (owner) {
      const list = commandByGroup.get(owner.id) ?? []
      list.push(cmd)
      commandByGroup.set(owner.id, list)
    } else {
      rootCommands.push(cmd)
    }
  }

  const byName = (a: { name: string }, b: { name: string }): number =>
    a.name.localeCompare(b.name, 'zh')

  /** 递归填充文件夹的 children（子文件夹在前、命令在后，同级按名称排序） */
  const fillChildren = (folder: FolderWithChildren): void => {
    const subFolders = (folderChildren.get(folder.id) ?? [])
      .slice()
      .sort((a, b) => byName(a, b))
      .map((f) => ({ ...f, children: [] as QuickCommandNode[] }))
    for (const sub of subFolders) fillChildren(sub)
    const cmds = (commandByGroup.get(folder.id) ?? [])
      .slice()
      .sort(byName)
      .map((c) => ({ kind: 'command' as const, ...c }) as unknown as QuickCommandNode)
    folder.children = [...subFolders, ...cmds] as unknown as QuickCommandNode[]
  }

  for (const root of rootFolders) {
    fillChildren(root)
    roots.push({
      kind: 'folder',
      id: root.id,
      name: root.name,
      parent_id: root.parent_id,
      children: root.children,
    } as unknown as QuickCommandNode)
  }
  // 根级命令追加在根级文件夹之后，同级按名称排序
  for (const cmd of rootCommands.sort(byName)) {
    roots.push({
      kind: 'command',
      id: cmd.id,
      name: cmd.name,
      command_text: cmd.command_text,
      group_id: cmd.group_id ?? null,
    } as unknown as QuickCommandNode)
  }
  return roots
}

export const useQuickCommandStore = defineStore('quickCommand', () => {
  // ---------- state ----------
  /** 快捷命令树（qc_list 归一化组树后） */
  const nodes = ref<QuickCommandNode[]>([])
  const loading = ref(false)
  /** 搜索关键字（实时过滤命令名/命令正文） */
  const search = ref('')

  // ---------- getters ----------
  /** 按关键字递归过滤：文件夹保留「子树含匹配项」的分支 */
  const filteredNodes = computed<QuickCommandNode[]>(() => {
    const keyword = (search.value ?? '').trim().toLowerCase()
    if (!keyword) return nodes.value
    const walk = (list: QuickCommandNode[]): QuickCommandNode[] => {
      const out: QuickCommandNode[] = []
      for (const node of list) {
        if (isCommandFolder(node)) {
          const children = walk(folderChildrenOf(node))
          if (children.length > 0 || node.name.toLowerCase().includes(keyword)) {
            out.push({ ...node, children } as unknown as QuickCommandNode)
          }
        } else {
          const cmd = node as unknown as QuickCommand
          const hit =
            cmd.name.toLowerCase().includes(keyword) ||
            (cmd.command_text ?? '').toLowerCase().includes(keyword)
          if (hit) out.push(node)
        }
      }
      return out
    }
    return walk(nodes.value)
  })

  /** 全部命令的平铺列表（供跨分组场景） */
  const allCommands = computed<QuickCommand[]>(() => {
    const out: QuickCommand[] = []
    const walk = (list: QuickCommandNode[]): void => {
      for (const node of list) {
        if (isCommandFolder(node)) {
          walk(folderChildrenOf(node))
        } else {
          out.push(node as unknown as QuickCommand)
        }
      }
    }
    walk(nodes.value)
    return out
  })

  /** 按 id 取命令 */
  function getCommandById(id: string): QuickCommand | undefined {
    return allCommands.value.find((c) => c.id === id)
  }

  // ---------- actions ----------
  /** 加载快捷命令树（qc_list） */
  async function load(): Promise<void> {
    loading.value = true
    try {
      const list = await qcList()
      const normalized = (list ?? [])
        .map(normalizeNode)
        .filter((n): n is QuickCommandNode => n !== null)
      nodes.value = buildTree(normalized)
    } catch (err) {
      console.error('[quickCommand] 加载快捷命令树失败:', err)
    } finally {
      loading.value = false
    }
  }

  /** 保存命令（qc_save_command），成功后刷新树 */
  async function saveCommand(cmd: QuickCommand): Promise<QuickCommand> {
    const saved = await qcSaveCommand(cmd)
    await load()
    return saved
  }

  /** 保存文件夹（qc_save_folder），成功后刷新树 */
  async function saveFolder(folder: QuickCommandFolder): Promise<QuickCommandFolder> {
    const saved = await qcSaveFolder(folder)
    await load()
    return saved
  }

  /** 删除命令或文件夹（qc_delete），成功后刷新树 */
  async function remove(id: string, isFolder: boolean): Promise<void> {
    await qcDelete(id, isFolder)
    await load()
  }

  return {
    nodes,
    loading,
    search,
    filteredNodes,
    allCommands,
    getCommandById,
    load,
    saveCommand,
    saveFolder,
    remove,
  }
})
