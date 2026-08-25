import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { SoftwareCategory, type CatalogEntry } from '@/models/software'

export const useCatalogStore = defineStore('catalog', () => {
  const entries = ref<CatalogEntry[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  const groupedEntries = computed(() => {
    const groups: Record<SoftwareCategory, CatalogEntry[]> = {
      [SoftwareCategory.Database]: [],
      [SoftwareCategory.Runtime]: [],
      [SoftwareCategory.Cache]: [],
      [SoftwareCategory.WebServer]: [],
      [SoftwareCategory.Registry]: [],
      [SoftwareCategory.Storage]: [],
      [SoftwareCategory.MessageQueue]: [],
      [SoftwareCategory.Search]: [],
      [SoftwareCategory.TimeSeries]: [],
    }
    entries.value.forEach((e) => {
      if (groups[e.category]) {
        groups[e.category].push(e)
      }
    })
    return groups
  })

  async function loadCatalog() {
    loading.value = true
    error.value = null
    try {
      entries.value = await invoke('list_available_software') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to load catalog:', e)
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function refreshCatalog() {
    loading.value = true
    error.value = null
    try {
      entries.value = await invoke('refresh_catalog') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to refresh catalog:', e)
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  return {
    entries,
    loading,
    error,
    groupedEntries,
    loadCatalog,
    refreshCatalog,
  }
})
