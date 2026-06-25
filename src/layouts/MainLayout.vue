<template>
  <div class="h-screen flex flex-col overflow-hidden">
    <!-- 顶部导航栏 -->
    <header class="border-b border-border h-14 flex items-center px-4 justify-between">
      <div class="flex items-center gap-4">
        <h2 class="text-lg font-semibold">{{ currentTitle }}</h2>
      </div>
      <div class="flex items-center gap-2">
        <button
          @click="toggleTheme"
          class="p-2 rounded-md hover:bg-accent"
          title="Toggle theme"
        >
          <span class="iconify" :data-icon="isDark ? 'mdi:weather-sunny' : 'mdi:weather-night'"></span>
        </button>
      </div>
    </header>

    <!-- 主内容区 -->
    <div class="flex-1 flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 overflow-auto p-4">
        <RouterView />
      </main>
    </div>

    <!-- 底部状态栏 -->
    <footer v-show="systemInfo" class="border-t border-border h-10 flex items-center px-4 text-sm text-muted-foreground gap-6">
      <div>
        {{ t('cpu') }}: {{ systemInfo.cpu_usage.toFixed(1) }}%
      </div>
      <div>
        {{ t('memory') }}: {{ formatBytes(systemInfo.memory_used) }} / {{ formatBytes(systemInfo.memory_total) }} ({{ systemInfo.memory_usage.toFixed(1) }}%)
      </div>
      <div>
        {{ t('softwareDir') }}: {{ softwareRoot }}
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import Sidebar from './Sidebar.vue'
import { RouterView } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from 'vue-i18n'
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SystemInfo } from '@/models/system'

const { t } = useI18n()
const appStore = useAppStore()
const settingsStore = useSettingsStore()
const currentTitle = computed(() => {
  const title = appStore.currentTitle
  return title || 'OPX'
})

const systemInfo = ref<SystemInfo | null>(null)
const softwareRoot = computed(() => settingsStore.settings?.software_root ?? 'apps')

let refreshInterval: number | null = null

const isDark = ref(document.documentElement.classList.contains('dark'))

function toggleTheme() {
  document.documentElement.classList.toggle('dark')
  isDark.value = !isDark.value
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

async function refreshSystemInfo() {
  systemInfo.value = await invoke('system_info')
}

onMounted(() => {
  refreshSystemInfo()
  refreshInterval = window.setInterval(refreshSystemInfo, 2000)
})

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval)
  }
})
</script>

<style scoped>
</style>
