import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { AppSettings } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

  const theme = computed(() => settings.value?.theme ?? 'auto')
  const language = computed(() => settings.value?.language ?? 'zh-CN')

  async function loadSettings() {
    settings.value = await invoke('get_settings')
  }

  async function saveSettings() {
    if (settings.value) {
      await invoke('save_settings', { settings: settings.value })
    }
  }

  return {
    settings,
    theme,
    language,
    loadSettings,
    saveSettings,
    updateSettings: (newSettings: AppSettings) => {
      settings.value = newSettings
    }
  }
})