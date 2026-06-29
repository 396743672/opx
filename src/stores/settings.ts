import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import type { AppSettings, ThemeMode } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

  /** 系统当前是否偏好深色（响应式） */
  const systemPrefersDark = ref(
    typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches
  )

  if (typeof window !== 'undefined') {
    const mql = window.matchMedia('(prefers-color-scheme: dark)')
    mql.addEventListener('change', (e) => {
      systemPrefersDark.value = e.matches
    })
  }

  const theme = computed<ThemeMode>(
    () => (settings.value?.theme as ThemeMode) ?? 'auto'
  )
  const language = computed(() => settings.value?.language ?? 'zh-CN')

  const isDark = computed(
    () => theme.value === 'dark' || (theme.value === 'auto' && systemPrefersDark.value)
  )

  /** 将当前主题应用到 <html> */
  function applyTheme() {
    document.documentElement.classList.toggle('dark', isDark.value)
  }

  watch(isDark, applyTheme, { immediate: true })

  async function loadSettings() {
    settings.value = await invoke('get_settings')
    applyTheme()
  }

  async function saveSettings() {
    if (settings.value) {
      await invoke('save_settings', { settings: settings.value })
    }
  }

  async function setTheme(mode: ThemeMode) {
    if (!settings.value) return
    settings.value.theme = mode
    applyTheme()
    await saveSettings()
  }

  return {
    settings,
    theme,
    language,
    isDark,
    systemPrefersDark,
    loadSettings,
    saveSettings,
    setTheme,
    updateSettings: (newSettings: AppSettings) => {
      settings.value = newSettings
    },
  }
})
