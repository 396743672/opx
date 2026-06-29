<template>
  <div class="flex flex-col h-screen w-screen overflow-hidden">
    <!-- 顶部导航栏 -->
    <header
      class="h-14 border-b border-border bg-card flex items-center px-4 justify-between flex-shrink-0"
    >
      <div class="flex items-center gap-3">
        <button
          @click="toggleSidebar"
          class="p-2 rounded-md hover:bg-muted transition-colors cursor-pointer"
          :aria-label="$t('toggleSidebar')"
        >
          <Icon icon="mdi:menu" class="text-xl" />
        </button>
        <div class="flex items-center gap-2">
          <div
            class="flex items-center justify-center w-7 h-7 rounded-md bg-primary text-primary-foreground"
          >
            <Icon icon="mdi:chart-variant" class="text-lg" />
          </div>
          <span class="text-base font-semibold tracking-tight">OPX</span>
        </div>
        <div class="hidden sm:block h-5 w-px bg-border mx-1"></div>
        <span class="hidden sm:block text-sm text-muted-foreground">{{
          $t(currentTitle)
        }}</span>
      </div>

      <div class="flex items-center gap-3">
        <!-- 实时 CPU / 内存 迷你指示 -->
        <div class="hidden md:flex items-center gap-4 mr-1">
          <div class="flex items-center gap-2">
            <Icon icon="mdi:cpu-64-bit" class="text-base text-muted-foreground" />
            <div class="w-16">
              <ProgressBar
                :value="systemStore.cpuUsage"
                :show-label="false"
                variant="linear"
              />
            </div>
            <span class="text-xs tnum w-10 text-right">{{ systemStore.cpuUsage.toFixed(0) }}%</span>
          </div>
          <div class="flex items-center gap-2">
            <Icon icon="mdi:memory" class="text-base text-muted-foreground" />
            <div class="w-16">
              <ProgressBar
                :value="systemStore.memoryUsage"
                :show-label="false"
                variant="linear"
              />
            </div>
            <span class="text-xs tnum w-10 text-right">{{ systemStore.memoryUsage.toFixed(0) }}%</span>
          </div>
        </div>

        <button
          @click="cycleTheme"
          class="p-2 rounded-md hover:bg-muted transition-colors cursor-pointer"
          :aria-label="$t('theme')"
          :title="$t(settingsStore.theme)"
        >
          <Icon :icon="themeIcon" class="text-xl" />
        </button>
        <button
          @click="goToSettings"
          class="p-2 rounded-md hover:bg-muted transition-colors cursor-pointer"
          :aria-label="$t('settings')"
        >
          <Icon icon="mdi:cog" class="text-xl" />
        </button>
      </div>
    </header>

    <!-- 主体内容 -->
    <div class="flex flex-1 overflow-hidden">
      <Sidebar :collapsed="sidebarCollapsed" />
      <main class="flex-1 overflow-auto bg-background">
        <div class="p-6 max-w-[1600px] mx-auto">
          <router-view />
        </div>
      </main>
    </div>

    <!-- 底部状态栏 -->
    <footer
      class="h-7 border-t border-border bg-card flex items-center px-4 text-xs text-muted-foreground flex-shrink-0"
    >
      <div class="flex items-center gap-4 w-full">
        <span class="flex items-center gap-1.5">
          <Icon icon="mdi:laptop" class="text-sm" />
          {{ systemInfo?.os_name }} {{ systemInfo?.os_version }}
        </span>
        <span class="hidden sm:flex items-center gap-1.5">
          <Icon icon="mdi:server" class="text-sm" />
          {{ systemInfo?.hostname }}
        </span>
        <span class="flex items-center gap-3 ml-auto">
          <span class="tnum">{{ $t('cpu') }}: {{ systemStore.cpuUsage.toFixed(1) }}%</span>
          <span class="tnum">{{ $t('memory') }}: {{ formatBytes(systemInfo?.memory_used || 0) }} / {{ formatBytes(systemInfo?.memory_total || 0) }}</span>
          <span class="hidden lg:inline tnum">{{ $t('uptime') }}: {{ uptime }}</span>
          <span class="hidden lg:inline">{{ $t('softwareDir') }}: {{ softwareRoot }}</span>
        </span>
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import type { ThemeMode } from '@/models/settings'
import Sidebar from './Sidebar.vue'
import ProgressBar from '@/components/ProgressBar.vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useRouter, useRoute } from 'vue-router'
import { formatBytes, formatUptime } from '@/utils/format'

const { t } = useI18n()
void t
const router = useRouter()
const route = useRoute()
const appStore = useAppStore()
const settingsStore = useSettingsStore()
const systemStore = useSystemStore()

const sidebarCollapsed = computed(() => appStore.sidebarCollapsed)
const toggleSidebar = () => appStore.toggleSidebar()

const systemInfo = computed(() => systemStore.systemInfo)
const softwareRoot = computed(() => settingsStore.settings?.software_root || 'apps')

const currentTitle = computed(() => (route.meta.title as string) || 'systemMonitor')

/* —— 主题切换（auto → light → dark 循环）—— */
const themeOrder: ThemeMode[] = ['auto', 'light', 'dark']
const themeIconMap: Record<ThemeMode, string> = {
  auto: 'mdi:theme-light-dark',
  light: 'mdi:weather-sunny',
  dark: 'mdi:moon-waning-crescent',
}
const themeIcon = computed(() => themeIconMap[settingsStore.theme])
function cycleTheme() {
  const i = themeOrder.indexOf(settingsStore.theme)
  settingsStore.setTheme(themeOrder[(i + 1) % themeOrder.length])
}

/* —— 运行时长 —— */
const nowTick = ref(Date.now())
let tickTimer: number | null = null
const uptime = computed(() => {
  const boot = systemInfo.value?.boot_time
  if (!boot) return '-'
  return formatUptime(Math.floor(nowTick.value / 1000) - boot)
})

const goToSettings = () => router.push('/settings')

onMounted(() => {
  systemStore.startPolling()
  tickTimer = window.setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  systemStore.stopPolling()
  if (tickTimer) clearInterval(tickTimer)
})
</script>
