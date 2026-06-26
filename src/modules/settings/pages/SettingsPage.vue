<template>
  <div class="space-y-8 max-w-2xl mx-auto p-4">
    <!-- 外观 -->
    <div class="bg-card rounded-lg border border-border p-6">
      <div class="mb-4">
        <h3 class="text-lg font-semibold">{{ $t('appearance') }}</h3>
      </div>
      <div class="space-y-4">
        <div>
          <label class="block text-sm font-medium mb-1">{{ $t('theme') }}</label>
          <select v-model="settings.theme" class="w-full rounded-md border border-border px-3 py-2 bg-background">
            <option value="auto">{{ $t('auto') }}</option>
            <option value="light">{{ $t('light') }}</option>
            <option value="dark">{{ $t('dark') }}</option>
          </select>
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">{{ $t('language') }}</label>
          <select
            v-model="settings.language"
            class="w-full rounded-md border border-border px-3 py-2 bg-background"
            @change="handleLanguageChange"
          >
            <option value="zh-CN">中文</option>
            <option value="en-US">English</option>
          </select>
        </div>
      </div>
    </div>

    <!-- 系统集成 -->
    <div class="bg-card rounded-lg border border-border p-6">
      <div class="mb-4">
        <h3 class="text-lg font-semibold">{{ $t('systemIntegration') }}</h3>
      </div>
      <div class="space-y-4">
        <div class="flex items-center gap-2">
          <input
            v-model="settings.register_as_system_service"
            type="checkbox"
            class="rounded border-border"
          />
          <span>{{ $t('registerAsService') }}</span>
        </div>
        <div class="flex items-center gap-2">
          <input
            v-model="settings.auto_start_managed_services"
            type="checkbox"
            class="rounded border-border"
          />
          <span>{{ $t('autoStartServices') }}</span>
        </div>
        <div>
          <label class="block text-sm font-medium mb-1">{{ $t('closeWindowAction') }}</label>
          <select v-model="settings.close_window_action" class="w-full rounded-md border border-border px-3 py-2 bg-background">
            <option value="minimize-to-tray">{{ $t('minimizeToTray') }}</option>
            <option value="exit">{{ $t('exitDirectly') }}</option>
            <option value="background-service">{{ $t('backgroundService') }}</option>
          </select>
        </div>
      </div>
    </div>

    <!-- 关于 -->
    <div class="bg-card rounded-lg border border-border p-6">
      <div class="mb-4">
        <h3 class="text-lg font-semibold">{{ $t('about') }}</h3>
      </div>
      <div class="space-y-2 text-sm">
        <div class="flex justify-between">
          <span>{{ $t('version') }}</span>
          <span class="font-medium">__VERSION__</span>
        </div>
      </div>
    </div>

    <div class="flex justify-end">
      <button @click="handleSave" class="px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90" :disabled="!settings">
        {{ $t('save') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { ref, onMounted, watch } from 'vue'
import { storeToRefs } from 'pinia'
import type { AppSettings } from '@/models/settings'

const { t, global } = useI18n()
const settingsStore = useSettingsStore()

// 使用storeToRefs保持响应式
const { settings } = storeToRefs<ReturnType<typeof useSettingsStore>>(settingsStore)

// 确保设置已加载
if (!settings.value) {
  settingsStore.loadSettings()
}

const { setCurrentTitle } = useAppStore()
setCurrentTitle(t('settings'))

// 主题切换逻辑提取为函数
const updateTheme = (theme: string) => {
  document.documentElement.classList.remove('dark')
  if (theme === 'dark') {
    document.documentElement.classList.add('dark')
  }
}

// 语言切换处理
function handleLanguageChange() {
  if (settings.value?.language) {
    // 直接切换i18n语言
    global.locale.value = settings.value.language
    // 可以将语言设置保存到localStorage或其他地方
    localStorage.setItem('preferred-locale', settings.value.language)
  }
}

onMounted(() => {
  // 同步主题
  const currentTheme = settings.value?.theme ?? 'auto'
  updateTheme(currentTheme)

  // 监听主题变化
  watch(() => settings.value?.theme, (newTheme) => {
    if (newTheme) {
      updateTheme(newTheme)
    }
  })
})

async function handleSave() {
  if (!settings.value) return

  try {
    settingsStore.updateSettings(settings.value)
    await settingsStore.saveSettings()
    // apply theme change
    updateTheme(settings.value.theme)
    // i18n language change will be handled on app level
    // 可以添加成功提示
    alert(t('saveSuccess'))
  } catch (error) {
    console.error('保存设置失败:', error)
    alert(t('saveFailed'))
  }
}
</script>