<template>
  <!-- 启动加载遮罩 -->
  <div
    v-if="booting"
    class="fixed inset-0 z-[60] flex flex-col items-center justify-center bg-background text-foreground"
  >
    <div class="flex items-center justify-center w-14 h-14 rounded-2xl bg-primary/10 text-primary mb-5">
      <Icon icon="mdi:monitor" class="text-3xl" />
    </div>
    <div class="text-lg font-semibold tracking-tight mb-1">OPX</div>
    <div class="flex items-center gap-3">
      <span class="inline-block w-1.5 h-1.5 rounded-full bg-primary/40 animate-pulse"></span>
      <span class="text-sm text-muted-foreground/60">{{ $t('loading') }}</span>
    </div>
  </div>
  <MainLayout v-show="!booting" />
  <CloseDialog
    v-if="showCloseDialog"
    :default-choice="defaultChoice"
    @choose="onChoose"
    @cancel="showCloseDialog = false"
  />
  <StopProgressDialog v-if="showStopProgress" />
  <ErrorDialog v-if="lifecycleStore.errorMessage" />
  <ConfirmDialog />
  <ToastHost />
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { Icon } from '@iconify/vue'
import MainLayout from '@/layouts/MainLayout.vue'
import CloseDialog from '@/components/CloseDialog.vue'
import StopProgressDialog from '@/components/StopProgressDialog.vue'
import ErrorDialog from '@/components/ErrorDialog.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import ToastHost from '@/components/ToastHost.vue'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import { useLifecycleStore } from '@/modules/software-manager/stores/lifecycle'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CloseWindowAction } from '@/models/settings'

const settingsStore = useSettingsStore()
const systemStore = useSystemStore()
const lifecycleStore = useLifecycleStore()

const showCloseDialog = ref(false)
const showStopProgress = ref(false)
/** 启动加载态：设置加载 + 首次系统数据就绪前显示遮罩 */
const booting = ref(true)

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
  // 禁用右键菜单和 F12 等开发者工具快捷键
  document.addEventListener('contextmenu', (e) => e.preventDefault())
  document.addEventListener('keydown', (e) => {
    if (
      e.key === 'F12' ||
      (e.ctrlKey && e.shiftKey && ['I', 'J', 'C'].includes(e.key.toUpperCase())) ||
      (e.ctrlKey && e.key.toUpperCase() === 'U')
    ) {
      e.preventDefault()
    }
  })

  // 监听首次系统数据就绪，解除启动遮罩
  const stopBootWatch = watch(
    () => systemStore.systemInfo,
    (info) => {
      if (info) {
        booting.value = false
        stopBootWatch()
      }
    }
  )
  // 超时兜底：3s 后强制解除，避免后端异常导致永久白屏
  const bootTimeout = window.setTimeout(() => {
    booting.value = false
    stopBootWatch()
  }, 3000)

  await settingsStore.loadSettings()
  // 启动系统数据轮询（首次 fetchAll 完成后 watch 会触发解除遮罩）
  systemStore.startPolling()

  unlistenClose = await listen('close-requested', () => {
    onCloseRequested()
  })
  unlistenStopComplete = await listen('stop-complete', () => {
    if (exitTimer) clearTimeout(exitTimer)
    exitTimer = window.setTimeout(() => {
      invoke('exit_app')
    }, 600)
  })

  window.clearTimeout(bootTimeout)
})

onUnmounted(() => {
  unlistenClose?.()
  unlistenStopComplete?.()
  if (exitTimer) clearTimeout(exitTimer)
})
</script>
