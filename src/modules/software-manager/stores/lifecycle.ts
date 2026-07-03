import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { SoftwareStatus, type SoftwareStatusEvent } from '@/models/software'

/**
 * 软件生命周期 Pinia store
 *
 * 监听后端 `software-status-changed` 事件，维护三个字典：
 * - statuses: installed_id → SoftwareStatus
 * - errors:   installed_id → string | null
 * - pids:     installed_id → number | null
 *
 * 组件用法：
 * - onMounted:  await lifecycleStore.initListener()
 * - onBeforeUnmount: lifecycleStore.destroyListener()
 * - getStatus(id) / getError(id) / getPid(id) 取实时状态
 */
export const useLifecycleStore = defineStore('software-lifecycle', () => {
  const statuses = ref<Record<string, SoftwareStatus>>({})
  const errors = ref<Record<string, string | null>>({})
  const pids = ref<Record<string, number | null>>({})
  let unlisten: UnlistenFn | null = null

  function setStatus(
    id: string,
    status: SoftwareStatus,
    pid?: number,
    error?: string | null,
  ) {
    statuses.value[id] = status
    // pid 传 undefined 时保留旧值；传 null 时清除
    if (pid !== undefined) {
      pids.value[id] = pid
    }
    // error 传 undefined 时保留旧值；传 null/string 时覆盖
    if (error !== undefined) {
      errors.value[id] = error
    }
  }

  function getStatus(id: string): SoftwareStatus {
    return statuses.value[id] ?? SoftwareStatus.Unknown
  }

  function getError(id: string): string | null {
    return errors.value[id] ?? null
  }

  function getPid(id: string): number | null {
    return pids.value[id] ?? null
  }

  async function initListener() {
    if (unlisten) return
    unlisten = await listen<SoftwareStatusEvent>(
      'software-status-changed',
      (e) => {
        const { installed_id, status, pid, error } = e.payload
        setStatus(installed_id, status as SoftwareStatus, pid ?? undefined, error ?? undefined)
      },
    )
  }

  function destroyListener() {
    if (unlisten) {
      unlisten()
      unlisten = null
    }
  }

  return {
    statuses,
    errors,
    pids,
    setStatus,
    getStatus,
    getError,
    getPid,
    initListener,
    destroyListener,
  }
})
