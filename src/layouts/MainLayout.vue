<template>
  <div class="flex flex-col h-screen w-screen overflow-hidden bg-background">
    <!-- 顶部导航栏 — 简洁 -->
    <header
      class="h-12 flex-shrink-0 flex items-center px-4 justify-between z-30 bg-background/70 backdrop-blur-xl border-b border-border/40"
    >
      <div class="flex items-center gap-3">
        <div class="flex items-center gap-2">
          <div
            class="flex items-center justify-center w-7 h-7 rounded-md bg-primary text-primary-foreground flex-shrink-0"
          >
            <Icon icon="mdi:monitor" class="text-base" />
          </div>
          <span class="text-sm font-semibold tracking-tight">OPX</span>
        </div>
        <div class="h-4 w-px bg-border/60"></div>
        <span class="text-sm text-muted-foreground/80">{{ $t(currentTitle) }}</span>
      </div>

      <div class="flex items-center gap-2">
        <!-- 系统状态指示 — 紧凑 -->
        <div class="hidden md:flex items-center gap-3 mr-1">
          <div class="flex items-center gap-1.5">
            <span class="inline-block w-1.5 h-1.5 rounded-full" :class="systemStore.cpuUsage > 80 ? 'bg-destructive' : 'bg-success'"></span>
            <span class="text-xs text-muted-foreground tnum">{{ systemStore.cpuUsage.toFixed(0) }}%</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="inline-block w-1.5 h-1.5 rounded-full" :class="systemStore.memoryUsage > 80 ? 'bg-destructive' : 'bg-info'"></span>
            <span class="text-xs text-muted-foreground tnum">{{ systemStore.memoryUsage.toFixed(0) }}%</span>
          </div>
        </div>

        <button
          @click="cycleTheme"
          class="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-all duration-150 cursor-pointer"
          :title="$t(settingsStore.theme)"
        >
          <Icon :icon="themeIcon" class="text-lg" />
        </button>
        <button
          @click="goToSettings"
          class="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-all duration-150 cursor-pointer"
          :aria-label="$t('settings')"
        >
          <Icon icon="mdi:cog" class="text-lg" />
        </button>
      </div>
    </header>

    <!-- 主体内容 -->
    <div class="flex flex-1 overflow-hidden relative">
      <Sidebar v-model:pinned="sidebarPinned" />
      <!-- pl-14 为收缩态侧边栏预留空间，pl-52 为固定展开态 -->
      <main class="flex-1 overflow-auto" :class="sidebarPinned ? 'pl-52' : 'pl-14'">
        <div class="p-8 max-w-[1600px]">
          <router-view />
        </div>
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import type { ThemeMode } from '@/models/settings'
import Sidebar from './Sidebar.vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useRouter, useRoute } from 'vue-router'

const { t } = useI18n()
void t
const router = useRouter()
const route = useRoute()
const settingsStore = useSettingsStore()
const systemStore = useSystemStore()
const sidebarPinned = ref(false)

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

const goToSettings = () => router.push('/settings')

onMounted(() => {
  systemStore.startPolling()
})

onUnmounted(() => {
  systemStore.stopPolling()
})
</script>
