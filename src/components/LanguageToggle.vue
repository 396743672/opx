<template>
  <button @click="toggleLanguage" class="language-toggle">
    {{ currentLanguage === 'zh-CN' ? 'EN' : '中' }}
  </button>
</template>

<script setup lang="ts">
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from 'vue-i18n'

const settingsStore = useSettingsStore()
const { locale } = useI18n()

const currentLanguage = computed(() => settingsStore.language)

const toggleLanguage = () => {
  const newLang = currentLanguage.value === 'zh-CN' ? 'en-US' : 'zh-CN'
  settingsStore.updateSettings({ ...settingsStore.settings, language: newLang })
  locale.value = newLang
}
</script>

<style scoped>
.language-toggle {
  background: none;
  border: none;
  font-size: 1rem;
  cursor: pointer;
  padding: 0.5rem 1rem;
  border-radius: 4px;
  transition: background 0.2s;
}

.language-toggle:hover {
  background: var(--background);
}
</style>