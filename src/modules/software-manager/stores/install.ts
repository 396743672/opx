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
  // 竞态缓冲：后端在 createTask 之前 emit 的事件先暂存，createTask 时回放，避免丢事件
  const pendingEvents = ref<Record<string, any[]>>({})

  const activeTasks = computed(() => {
    return Object.values(tasks.value).filter(
      (t) =>
        t.phase === 'downloading' ||
        t.phase === 'extracting' ||
        t.phase === 'failed',
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
    // 回放在 createTask 之前到达的缓冲事件（后端可能已开始下载/失败）
    const buffered = pendingEvents.value[id]
    if (buffered) {
      for (const p of buffered) {
        updateTask(id, p)
      }
      delete pendingEvents.value[id]
    }
  }

  function removeTask(id: string) {
    delete tasks.value[id]
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
      // 失败 8 秒后清理，给用户时间查看错误信息
      setTimeout(() => {
        delete tasks.value[id]
      }, 8000)
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
      } else {
        // 任务尚未 createTask（后端已抢先 emit），缓冲待回放
        if (!pendingEvents.value[id]) pendingEvents.value[id] = []
        pendingEvents.value[id].push(payload)
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
    removeTask,
    initEvents,
    cleanup,
  }
})
