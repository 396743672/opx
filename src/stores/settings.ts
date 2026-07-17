import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import type { AppSettings, ThemeMode, Language } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'
import { i18n } from '@/utils/i18n'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

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
  const language = computed<Language>(() => settings.value?.language ?? 'zh-CN')

  const isDark = computed(
    () =>
      theme.value === 'dark' ||
      (theme.value === 'auto' && systemPrefersDark.value)
  )
  const sidebarCollapsed = computed(() => settings.value?.sidebar_collapsed ?? false)

  function applyTheme() {
    const isDark = theme.value === 'dark' ||
      (theme.value === 'auto' && systemPrefersDark.value)
    const isWarm = theme.value === 'warm'
    document.documentElement.classList.toggle('dark', isDark)
    document.documentElement.classList.toggle('warm', isWarm)
  }

  function applyLanguage() {
    const lang = language.value
    if (i18n.global.locale.value !== lang) {
      i18n.global.locale.value = lang
    }
    // 同步到 localStorage，下次启动时 i18n.ts 直接读取，避免启动遮罩显示错误语言
    try {
      localStorage.setItem('opx_language', lang)
    } catch {
      // localStorage 不可用时静默
    }
  }

  watch(isDark, applyTheme, { immediate: true })

  async function loadSettings() {
    settings.value = await invoke('get_settings')
    applyTheme()
    applyLanguage()
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

  async function setSidebarCollapsed(v: boolean) {
    if (!settings.value) return
    settings.value.sidebar_collapsed = v
    await saveSettings()
  }

  async function setLanguage(lang: Language) {
    if (!settings.value) return
    settings.value.language = lang
    applyLanguage()
    await saveSettings()
  }

  return {
    settings,
    theme,
    language,
    isDark,
    sidebarCollapsed,
    systemPrefersDark,
    loadSettings,
    saveSettings,
    setTheme,
    setSidebarCollapsed,
    setLanguage,
  }
})
