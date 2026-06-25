import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { AppSettings } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

  // 计算属性
  const theme = computed(() => settings.value?.theme ?? 'auto')
  const language = computed(() => settings.value?.language ?? 'zh-CN')

  // 方法
  async function loadSettings() {
    try {
      settings.value = await invoke('get_settings')
    } catch (error) {
      console.error('加载设置失败:', error)
      // 设置默认值
      settings.value = {
        theme: 'auto',
        language: 'zh-CN',
        sidebarCollapsed: false,
        register_as_system_service: false,
        auto_start_managed_services: false,
        close_window_action: 'minimize-to-tray'
      }
    }
  }

  async function saveSettings() {
    if (!settings.value) return

    try {
      await invoke('save_settings', { settings: settings.value })
    } catch (error) {
      console.error('保存设置失败:', error)
      throw error
    }
  }

  function updateSettings(newSettings: AppSettings) {
    settings.value = newSettings
  }

  return {
    settings,
    theme,
    language,
    loadSettings,
    saveSettings,
    updateSettings
  }
})