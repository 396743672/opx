import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

type InstallPhase = 'downloading' | 'extracting' | 'completed' | 'failed'

interface InstallTask {
  id: string
  key: string
  name: string
  phase: InstallPhase
  downloaded: number
  total: number | null
  percent: number | null
  error: string | null
  installedId: string | null
}

export const useInstallStore = defineStore('install', () => {
  const tasks = ref<Record<string, InstallTask>>({})

  const activeTasks = computed(() => {
    return Object.values(tasks.value).filter(
      (t) => t.phase === 'downloading' || t.phase === 'extracting',
    )
  })

  function hasActiveTask(key: string): boolean {
    return activeTasks.value.some((t) => t.key === key)
  }

  function createTask(id: string, key: string, name: string) {
    tasks.value[id] = {
      id,
      key,
      name,
      phase: 'downloading',
      downloaded: 0,
      total: null,
      percent: null,
      error: null,
      installedId: null,
    }
  }

  function updateTask(id: string, payload: any) {
    const task = tasks.value[id]
    if (!task) return

    task.phase = payload.phase
    if (payload.phase === 'downloading') {
      task.downloaded = payload.downloaded ?? 0
      task.total = payload.total ?? null
      task.percent = payload.percent ?? null
    } else if (payload.phase === 'extracting') {
      task.percent = payload.percent ?? 0
    } else if (payload.phase === 'completed') {
      task.installedId = payload.installed_id ?? null
      // 完成 3 秒后清理
      setTimeout(() => {
        delete tasks.value[id]
      }, 3000)
    } else if (payload.phase === 'failed') {
      task.error = payload.error ?? '未知错误'
    }
  }

  let unlistenFn: UnlistenFn | null = null

  async function initEvents() {
    if (unlistenFn) return
    unlistenFn = await listen('install-progress', (event) => {
      const payload = event.payload as any
      const id = payload.install_id
      if (tasks.value[id]) {
        updateTask(id, payload)
      }
    })
  }

  function cleanup() {
    if (unlistenFn) {
      unlistenFn()
      unlistenFn = null
    }
  }

  return {
    tasks,
    activeTasks,
    hasActiveTask,
    createTask,
    updateTask,
    initEvents,
    cleanup,
  }
})
