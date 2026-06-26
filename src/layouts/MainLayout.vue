<template>
  <div class="flex flex-col h-screen w-screen overflow-hidden">
    <!-- 顶部导航栏 -->
    <header class="h-14 border-b border-border bg-card flex items-center px-4 justify-between">
      <div class="flex items-center gap-2">
        <button @click="toggleSidebar" class="p-2 rounded-md hover:bg-muted transition-colors">
          <iconify-icon icon="mdi:menu" class="text-xl" />
        </button>
        <h1 class="text-lg font-semibold">OPX</h1>
      </div>
      <div class="flex items-center gap-2">
        <n-button quaternary @click="toggleTheme">
          <iconify-icon :icon="isDark ? 'mdi:weather-sunny' : 'mdi:moon-waning-crescent'" class="text-xl" />
        </n-button>
        <n-button quaternary @click="goToSettings">
          <iconify-icon icon="mdi:cog" class="text-xl" />
        </n-button>
      </div>
    </header>

    <!-- 主体内容 -->
    <div class="flex flex-1 overflow-hidden">
      <!-- 侧边栏 -->
      <Sidebar :collapsed="sidebarCollapsed" />
      <!-- 主内容区域 -->
      <main class="flex-1 overflow-auto p-4 bg-background">
        <router-view />
      </main>
    </div>

    <!-- 底部状态栏 -->
    <footer class="h-8 border-t border-border bg-card flex items-center px-4 text-sm text-muted-foreground">
      <div class="flex items-center gap-6 w-full">
        <div>
          {{ $t('cpu') }}: {{ systemInfo?.cpu_usage.toFixed(1) }}%
        </div>
        <div>
          {{ $t('memory') }}: {{ formatMemory(systemInfo?.memory_used || 0) }} / {{ formatMemory(systemInfo?.memory_total || 0) }} ({{ systemInfo?.memory_usage.toFixed(1) }}%)
        </div>
        <div class="ml-auto">
          {{ $t('softwareDir') }}: {{ softwareRoot }}
        </div>
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import Sidebar from './Sidebar.vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { NButton } from 'naive-ui'

useI18n()
const router = useRouter()
const appStore = useAppStore()
const settingsStore = useSettingsStore()

const sidebarCollapsed = computed(() => appStore.sidebarCollapsed)
const toggleSidebar = () => appStore.toggleSidebar()

const isDark = ref(false)

const systemInfo = ref<{
  cpu_usage: number
  memory_used: number
  memory_total: number
  memory_usage: number
} | null>(null)

const softwareRoot = computed(() => settingsStore.settings?.software_root || 'apps')

const toggleTheme = () => {
  const html = document.documentElement
  if (html.classList.contains('dark')) {
    html.classList.remove('dark')
    isDark.value = false
  } else {
    html.classList.add('dark')
    isDark.value = true
  }
}

const goToSettings = () => {
  router.push('/settings')
}

const updateSystemInfo = async () => {
  systemInfo.value = await invoke('system_info')
}

const formatMemory = (bytes: number): string => {
  const mb = bytes / (1024 * 1024)
  if (mb < 1024) {
    return `${mb.toFixed(1)} MB`
  } else {
    const gb = mb / 1024
    return `${gb.toFixed(1)} GB`
  }
}

let interval: number | null = null

onMounted(() => {
  updateSystemInfo()
  interval = window.setInterval(updateSystemInfo, 2000)

  // 初始化主题
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  if (prefersDark) {
    document.documentElement.classList.add('dark')
    isDark.value = true
  }
})

onUnmounted(() => {
  if (interval) {
    clearInterval(interval)
  }
})
</script>

<style scoped>
</style>
