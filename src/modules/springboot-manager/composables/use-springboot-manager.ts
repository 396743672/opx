import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SpringApp, SpringAppList, AppGroup } from '@/models/springboot'

export function useSpringBootManager() {
  const applications = ref<SpringApp[]>([])
  const groups = ref<AppGroup[]>([])

  async function loadList() {
    const data = await invoke<SpringAppList>('list_applications')
    applications.value = data.applications
    groups.value = data.groups
  }

  async function saveApp(app: SpringApp) {
    await invoke('save_application', { app })
    await loadList()
  }

  async function deleteApp(id: string) {
    await invoke('delete_application', { id })
    await loadList()
  }

  async function startApp(id: string) {
    try {
      await invoke('start_application', { id })
      await loadList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function stopApp(id: string) {
    try {
      await invoke('stop_application', { id })
      await loadList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function restartApp(id: string) {
    try {
      await invoke('restart_application', { id })
      await loadList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function startAll() {
    try {
      await invoke('start_all_applications')
      await loadList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function stopAll() {
    try {
      await invoke('stop_all_applications')
      await loadList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  onMounted(() => {
    loadList()
  })

  return {
    applications,
    groups,
    loadList,
    saveApp,
    deleteApp,
    startApp,
    stopApp,
    restartApp,
    startAll,
    stopAll,
  }
}