import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SystemInfo, ProcessInfo, HistoryPoint } from '@/models/system'

export function useSystemMonitor() {
  const systemInfo = ref<SystemInfo | null>(null)
  const processList = ref<ProcessInfo[]>([])
  const history = ref<HistoryPoint[]>([])
  let refreshInterval: number | null = null

  async function refresh() {
    systemInfo.value = await invoke('system_info')
  }

  async function loadProcessList() {
    processList.value = await invoke('process_list')
  }

  async function loadHistory() {
    history.value = await invoke('system_history')
  }

  async function killPid(pid: number): Promise<boolean> {
    try {
      await invoke('kill_process', { pid })
      await loadProcessList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  onMounted(() => {
    refresh()
    loadProcessList()
    loadHistory()
    refreshInterval = window.setInterval(refresh, 2000)
  })

  onUnmounted(() => {
    if (refreshInterval) {
      clearInterval(refreshInterval)
    }
  })

  return {
    systemInfo,
    processList,
    history,
    refresh,
    loadProcessList,
    killPid,
  }
}