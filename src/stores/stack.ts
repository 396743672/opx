// B 扩展（一键启动栈 Stack）Pinia store
// 封装 8 个 Tauri 命令的 invoke、运行态缓存，并订阅 stack-status-changed 事件刷新 UI。

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/utils/ipc'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  Stack,
  StackItem,
  CreateStackPayload,
  UpdateStackPayload,
  StackStartPlan,
  StackMemberRuntime,
  StackStatusEvent,
} from '@/models/stack'
import type { InstalledSoftware } from '@/models/software'
import type { SpringBootApp } from '@/models/springboot'

export const useStackStore = defineStore('stack', () => {
  const stacks = ref<Stack[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  // 各栈的运行态成员快照，key = stack_id
  const runtimes = ref<Record<string, StackMemberRuntime[]>>({})

  // 可作为栈成员的候选列表（用于编辑对话框）
  const installedSoftware = ref<InstalledSoftware[]>([])
  const springbootApps = ref<SpringBootApp[]>([])

  let unlisten: UnlistenFn | null = null

  // ------------------------- 查询 -------------------------

  async function loadStacks() {
    loading.value = true
    error.value = null
    try {
      stacks.value = await invoke<Stack[]>('list_stacks')
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function getStack(id: string): Promise<Stack | null> {
    try {
      return await invoke<Stack>('get_stack', { id })
    } catch (e) {
      error.value = String(e)
      return null
    }
  }

  async function loadCandidates() {
    try {
      installedSoftware.value = await invoke<InstalledSoftware[]>(
        'list_installed_software'
      )
    } catch {
      installedSoftware.value = []
    }
    try {
      springbootApps.value = await invoke<SpringBootApp[]>('list_springboot_apps')
    } catch {
      springbootApps.value = []
    }
  }

  // ------------------------- 写操作 -------------------------

  async function createStack(payload: CreateStackPayload): Promise<Stack> {
    return await invoke<Stack>('create_stack', { payload })
  }

  async function updateStack(
    id: string,
    payload: UpdateStackPayload
  ): Promise<Stack> {
    return await invoke<Stack>('update_stack', { id, payload })
  }

  async function deleteStack(id: string): Promise<void> {
    await invoke('delete_stack', { id })
    stacks.value = stacks.value.filter((s) => s.id !== id)
    delete runtimes.value[id]
  }

  // ------------------------- 编排 -------------------------

  async function startStack(id: string): Promise<StackStartPlan | null> {
    try {
      const plan = await invoke<StackStartPlan>('start_stack', { id })
      return plan
    } catch (e) {
      error.value = String(e)
      return null
    }
  }

  async function stopStack(id: string): Promise<void> {
    await invoke('stop_stack', { id })
  }

  async function restartStack(id: string): Promise<StackStartPlan | null> {
    try {
      return await invoke<StackStartPlan>('restart_stack', { id })
    } catch (e) {
      error.value = String(e)
      return null
    }
  }

  async function exportStack(id: string, path: string): Promise<void> {
    await invoke('export_stack', { id, path })
  }

  async function importStack(path: string): Promise<Stack | null> {
    try {
      const stack = await invoke<Stack>('import_stack', { path })
      await loadStacks()
      return stack
    } catch (e) {
      error.value = String(e)
      return null
    }
  }

  // ------------------------- 事件订阅 -------------------------

  /** 订阅 stack-status-changed，刷新对应栈的成员运行态。返回取消订阅函数。 */
  async function subscribe(): Promise<void> {
    if (unlisten) return
    unlisten = await listen<StackStatusEvent>(
      'stack-status-changed',
      (event) => {
        const payload = event.payload
        runtimes.value[payload.stack_id] = payload.members
      }
    )
  }

  function unsubscribe() {
    if (unlisten) {
      unlisten()
      unlisten = null
    }
  }

  /** 获取某栈当前运行态；若尚未有事件推送则为空数组 */
  function getRuntime(stackId: string): StackMemberRuntime[] {
    return runtimes.value[stackId] ?? []
  }

  /** 依据 ref_id 解析成员显示名（软件名 / Spring Boot 应用名） */
  function resolveName(item: StackItem): string {
    if (item.ref_type === 'software') {
      return (
        installedSoftware.value.find((s) => s.id === item.ref_id)?.name ??
        item.ref_id
      )
    }
    return (
      springbootApps.value.find((a) => a.id === item.ref_id)?.name ??
      item.ref_id
    )
  }

  return {
    stacks,
    loading,
    error,
    runtimes,
    installedSoftware,
    springbootApps,
    loadStacks,
    getStack,
    loadCandidates,
    createStack,
    updateStack,
    deleteStack,
    startStack,
    stopStack,
    restartStack,
    exportStack,
    importStack,
    subscribe,
    unsubscribe,
    getRuntime,
    resolveName,
  }
})
