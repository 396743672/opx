<template>
  <button @click="toggleTheme" class="theme-toggle">
    {{ isDark ? '☀️' : '🌙' }}
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'

const settingsStore = useSettingsStore()

const isDark = computed(() => {
  const theme = settingsStore.theme
  if (theme === 'dark') return true
  if (theme === 'light') return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
})

const toggleTheme = () => {
  const currentTheme = settingsStore.theme
  const newTheme = currentTheme === 'light' ? 'dark' : currentTheme === 'dark' ? 'light' : 'auto'
  settingsStore.updateSettings({ ...settingsStore.settings, theme: newTheme })
}
</script>

<style scoped>
.theme-toggle {
  background: none;
  border: none;
  font-size: 1.2rem;
  cursor: pointer;
  padding: 0.5rem;
  border-radius: 50%;
  transition: background 0.2s;
}

.theme-toggle:hover {
  background: var(--background);
}
</style>