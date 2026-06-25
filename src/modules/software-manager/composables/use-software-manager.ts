import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SoftwareMeta, InstalledSoftware, InstallParams } from '@/models/software'

export function useSoftwareManager() {
  const availableSoftware = ref<SoftwareMeta[]>([])
  const installedSoftware = ref<InstalledSoftware[]>([])

  async function loadAvailable() {
    availableSoftware.value = await invoke('list_available_software')
  }

  async function loadInstalled() {
    installedSoftware.value = await invoke('list_installed_software')
  }

  async function installSoftware(params: InstallParams) {
    try {
      await invoke('install_software', params)
      await loadInstalled()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function startSoftware(id: string) {
    try {
      await invoke('start_software', { id })
      await loadInstalled()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function stopSoftware(id: string) {
    try {
      await invoke('stop_software', { id })
      await loadInstalled()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  async function uninstallSoftware(id: string) {
    try {
      await invoke('uninstall_software', { id })
      await loadInstalled()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  onMounted(() => {
    loadAvailable()
    loadInstalled()
  })

  return {
    availableSoftware,
    installedSoftware,
    loadAvailable,
    loadInstalled,
    installSoftware,
    startSoftware,
    stopSoftware,
    uninstallSoftware,
  }
}
