<template>
  <div class="flex flex-col h-screen w-screen overflow-hidden bg-background">
    <!-- 顶部导航栏 — 简洁 -->
    <header
      @mousedown="onTitlebarMouseDown"
      class="h-12 flex-shrink-0 flex items-center justify-between z-30 bg-background/70 backdrop-blur-xl border-b border-border/40 pr-2"
    >
      <div class="flex items-center h-full">
        <!-- 品牌区：宽度随侧边栏同步，左对齐与下方菜单图标光学对齐（字形中心 28px） -->
        <div
          class="flex items-center justify-start pl-3.5 flex-shrink-0 h-full transition-all duration-300"
          :class="sidebarPinned ? 'w-52' : 'w-14'"
        >
          <div class="flex items-center gap-2">
            <div
              class="flex items-center justify-center w-7 h-7 rounded-md bg-primary text-primary-foreground flex-shrink-0"
            >
              <Icon icon="mdi:monitor" class="text-base" />
            </div>
            <span v-show="sidebarPinned" class="text-sm font-semibold tracking-tight whitespace-nowrap">OPX</span>
          </div>
        </div>
        <div v-show="sidebarPinned" class="h-4 w-px bg-border/60 flex-shrink-0 transition-all duration-300"></div>
        <span class="text-sm text-muted-foreground/80 px-3">{{ $t(currentTitle) }}</span>
      </div>

      <div class="flex items-center gap-1">
        <!-- 系统状态指示 — 紧凑 -->
        <div class="hidden md:flex items-center gap-3 mr-2">
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

        <!-- 窗口控制 -->
        <div class="flex items-center ml-2 gap-0.5">
          <button @mousedown.stop.prevent="minimize" class="win-btn" title="最小化">
            <Icon icon="mdi:window-minimize" class="text-sm" />
          </button>
          <button @mousedown.stop.prevent="toggleMaximize" class="win-btn" :title="maximized ? '还原' : '最大化'">
            <Icon :icon="maximized ? 'mdi:window-restore' : 'mdi:window-maximize'" class="text-sm" />
          </button>
          <button @mousedown.stop.prevent="closeWindow" class="win-btn win-btn-close" title="关闭">
            <Icon icon="mdi:window-close" class="text-sm" />
          </button>
        </div>
      </div>
    </header>

    <!-- 主体内容 -->
    <div class="flex flex-1 overflow-hidden relative">
      <Sidebar :pinned="sidebarPinned" @update:pinned="settingsStore.setSidebarCollapsed(!$event)" />
      <!-- pl-14 为收缩态侧边栏预留空间，pl-52 为固定展开态 -->
      <main class="flex-1 overflow-auto" :class="sidebarPinned ? 'pl-52' : 'pl-14'">
        <div class="p-6 xl:p-8">
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
import { getCurrentWindow } from '@tauri-apps/api/window'

useI18n()
const router = useRouter()
const route = useRoute()
const settingsStore = useSettingsStore()
const systemStore = useSystemStore()
const sidebarPinned = computed(() => !settingsStore.sidebarCollapsed)

const currentTitle = computed(() => (route.meta.title as string) || 'systemMonitor')

/* —— 主题切换（auto → light → warm → dark 循环）—— */
const themeOrder: ThemeMode[] = ['auto', 'light', 'warm', 'dark']
const themeIconMap: Record<ThemeMode, string> = {
  auto: 'mdi:theme-light-dark',
  light: 'mdi:weather-sunny',
  warm: 'mdi:weather-partly-cloudy',
  dark: 'mdi:moon-waning-crescent',
}
const themeIcon = computed(() => themeIconMap[settingsStore.theme])
function cycleTheme() {
  const i = themeOrder.indexOf(settingsStore.theme)
  settingsStore.setTheme(themeOrder[(i + 1) % themeOrder.length])
}

const goToSettings = () => router.push('/settings')

const maximized = ref(false)

function win() { return getCurrentWindow() }

function onTitlebarMouseDown(e: MouseEvent) {
  // 左键且非交互元素 → 拖拽窗口
  if (e.buttons === 1 && !(e.target as HTMLElement).closest('button')) {
    if (e.detail === 2) {
      // 双击最大化/还原
      win().toggleMaximize()
    } else {
      // 单次拖拽
      win().startDragging()
    }
  }
}

async function minimize() {
  try { await win().minimize() } catch (err) { console.error('minimize:', err) }
}
async function toggleMaximize() {
  try {
    await win().toggleMaximize()
    maximized.value = await win().isMaximized()
  } catch (err) { console.error('toggleMaximize:', err) }
}
async function closeWindow() {
  try { await win().close() } catch (err) { console.error('close:', err) }
}

onMounted(async () => {
  const w = win()
  try { maximized.value = await w.isMaximized() } catch {}
  try { await w.onResized(() => { w.isMaximized().then(v => maximized.value = v) }) } catch {}
  systemStore.startPolling()
})

onUnmounted(() => {
  systemStore.stopPolling()
})
</script>

<style scoped>
.win-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 28px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  cursor: pointer;
  transition: all 0.12s;
}
.win-btn:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.win-btn-close:hover {
  background: var(--color-destructive);
  color: var(--color-destructive-foreground);
}
</style>
