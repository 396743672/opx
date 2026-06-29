<template>
  <MainLayout />
  <CloseDialog
    v-if="showCloseDialog"
    :default-choice="defaultChoice"
    @choose="onChoose"
    @cancel="showCloseDialog = false"
  />
  <StopProgressDialog v-if="showStopProgress" />
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import MainLayout from '@/layouts/MainLayout.vue'
import CloseDialog from '@/components/CloseDialog.vue'
import StopProgressDialog from '@/components/StopProgressDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CloseWindowAction } from '@/models/settings'

const settingsStore = useSettingsStore()
const appStore = useAppStore()
void appStore

const showCloseDialog = ref(false)
const showStopProgress = ref(false)

const defaultChoice = computed<'tray' | 'exit'>(() =>
  settingsStore.settings?.close_window_action === CloseWindowAction.Exit
    ? 'exit'
    : 'tray'
)

let unlistenClose: UnlistenFn | null = null
let unlistenStopComplete: UnlistenFn | null = null
let exitTimer: number | null = null

async function executeTray() {
  await invoke('hide_main_window')
}

async function executeExit() {
  // 先显示进度对话框，再触发后端停止流程。
  // stop-complete 监听在 App 挂载时已注册，确保不遗漏事件。
  showStopProgress.value = true
  await invoke('quit_app')
}

async function onCloseRequested() {
  const s = settingsStore.settings
  if (!s) {
    await executeTray()
    return
  }
  if (s.ask_on_close) {
    showCloseDialog.value = true
  } else {
    if (s.close_window_action === CloseWindowAction.Exit) {
      await executeExit()
    } else {
      await executeTray()
    }
  }
}

async function onChoose(choice: 'tray' | 'exit', remember: boolean) {
  showCloseDialog.value = false
  if (remember && settingsStore.settings) {
    settingsStore.settings.close_window_action =
      choice === 'exit' ? CloseWindowAction.Exit : CloseWindowAction.CloseToTray
    settingsStore.settings.ask_on_close = false
    await settingsStore.saveSettings()
  }
  if (choice === 'exit') {
    await executeExit()
  } else {
    await executeTray()
  }
}

onMounted(async () => {
  await settingsStore.loadSettings()
  unlistenClose = await listen('close-requested', () => {
    onCloseRequested()
  })
  // 监听停止完成：展示「已安全退出」后真正退出
  unlistenStopComplete = await listen('stop-complete', () => {
    if (exitTimer) clearTimeout(exitTimer)
    exitTimer = window.setTimeout(() => {
      invoke('exit_app')
    }, 600)
  })
})

onUnmounted(() => {
  unlistenClose?.()
  unlistenStopComplete?.()
  if (exitTimer) clearTimeout(exitTimer)
})
</script>
