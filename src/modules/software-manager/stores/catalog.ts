import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { SoftwareCategory, type CatalogEntry } from '@/models/software'

export const useCatalogStore = defineStore('catalog', () => {
  const entries = ref<CatalogEntry[]>([])
  const loading = ref(false)

  const groupedEntries = computed(() => {
    const groups: Record<SoftwareCategory, CatalogEntry[]> = {
      [SoftwareCategory.Database]: [],
      [SoftwareCategory.Runtime]: [],
      [SoftwareCategory.Cache]: [],
      [SoftwareCategory.WebServer]: [],
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
    try {
      entries.value = await invoke('list_available_software') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to load catalog:', e)
    } finally {
      loading.value = false
    }
  }

  async function refreshCatalog() {
    loading.value = true
    try {
      entries.value = await invoke('refresh_catalog') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to refresh catalog:', e)
    } finally {
      loading.value = false
    }
  }

  return {
    entries,
    loading,
    groupedEntries,
    loadCatalog,
    refreshCatalog,
  }
})
