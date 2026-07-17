<template>
  <div class="animate-fade-in max-w-2xl">
    <PageHeader
      icon="mdi:cog"
      :title="$t('settings')"
      :subtitle="$t('appearance')"
    />

    <!-- 外观 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader :title="$t('appearance')" hide-refresh />
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('themeLabel') }}</span>
          <select
            v-model="themeValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="auto">{{ $t('auto') }}</option>
            <option value="light">{{ $t('light') }}</option>
            <option value="warm">{{ $t('warm') }}</option>
            <option value="dark">{{ $t('dark') }}</option>
          </select>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('languageLabel') }}</span>
          <select
            v-model="languageValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="zh-CN">中文</option>
            <option value="en-US">English</option>
          </select>
        </div>
      </div>
    </div>

    <!-- 关闭行为 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader :title="$t('closeBehavior')" hide-refresh />
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('closeBehavior') }}</span>
          <select
            v-model="closeActionValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary cursor-pointer"
          >
            <option value="CloseToTray">{{ $t('closeToTray') }}</option>
            <option value="Exit">{{ $t('exitProgram') }}</option>
          </select>
        </div>
        <label class="flex items-center justify-between cursor-pointer">
          <span class="text-sm">{{ $t('askOnClose') }}</span>
          <input v-model="askOnCloseValue" type="checkbox" class="accent-primary w-4 h-4" />
        </label>
      </div>
    </div>

    <!-- 下载代理 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader title="下载代理" hide-refresh />
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">GitHub 加速代理</span>
          <input v-model="githubProxyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" placeholder="https://ghfast.top" />
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm">全局代理</span>
          <input v-model="proxyValue" class="h-8 px-2 w-72 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono" placeholder="http://127.0.0.1:7890（空=直连）" />
        </div>
      </div>
    </div>

    <div class="flex justify-end">
      <button
        class="px-4 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors cursor-pointer"
        @click="save"
      >
        {{ $t('save') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { CloseWindowAction, type ThemeMode, type Language } from '@/models/settings'
import PageHeader from '@/components/PageHeader.vue'
import CardHeader from '@/components/CardHeader.vue'

useI18n()
const settingsStore = useSettingsStore()

const themeValue = ref<ThemeMode>('auto')
const languageValue = ref<Language>('zh-CN')
const closeActionValue = ref<CloseWindowAction>(CloseWindowAction.CloseToTray)
const askOnCloseValue = ref(true)
const githubProxyValue = ref('')
const proxyValue = ref('')

watch(
  () => settingsStore.settings,
  (s) => {
    if (s) {
      themeValue.value = (s.theme as ThemeMode) || 'auto'
      languageValue.value = (s.language as Language) || 'zh-CN'
      closeActionValue.value = s.close_window_action
      askOnCloseValue.value = s.ask_on_close
      githubProxyValue.value = s.github_proxy_url || ''
      proxyValue.value = s.proxy_url || ''
    }
  },
  { immediate: true }
)

watch(languageValue, (lang) => {
  settingsStore.setLanguage(lang)
})

watch(themeValue, (mode) => {
  settingsStore.setTheme(mode)
})

async function save() {
  if (!settingsStore.settings) return
  settingsStore.settings.close_window_action = closeActionValue.value
  settingsStore.settings.ask_on_close = askOnCloseValue.value
  settingsStore.settings.github_proxy_url = githubProxyValue.value
  settingsStore.settings.proxy_url = proxyValue.value
  await settingsStore.saveSettings()
}
</script>
