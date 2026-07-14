<template>
  <Teleport to="body">
    <div class="dialog-overlay">
      <div class="dialog-panel">
        <div class="dialog-hd">
          <h2>{{ $t('viewLogs') }} - {{ appName }}</h2>
          <div class="flex items-center gap-2">
            <button class="btn" @click="loadTail"><Icon icon="mdi:refresh" /> {{ $t('refresh') }}</button>
            <button class="btn" @click="$emit('close')"><Icon icon="mdi:close" /></button>
          </div>
        </div>
        <div class="log-box" ref="logBox">
          <div v-if="error" class="p-4 text-red-400 font-sans">{{ error }}</div>
          <div v-else-if="lines.length === 0" class="p-4 text-gray-500 font-sans">{{ $t('noLogs') }}</div>
          <div v-for="(line, i) in lines" :key="i" class="log-line">{{ line }}</div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, nextTick, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ appId: string; appName: string; logPath: string }>()
defineEmits<{ close: [] }>()

const lines = ref<string[]>([])
const error = ref('')
let timer: ReturnType<typeof setInterval> | null = null
const logBox = ref<HTMLElement | null>(null)

async function loadTail() {
  error.value = ''
  if (!props.logPath) { error.value = '日志路径未配置'; return }
  try {
    lines.value = await invoke<string[]>('read_springboot_log', { path: props.logPath })
  } catch (e: any) {
    error.value = '无法读取日志: ' + (typeof e === 'string' ? e : (e?.message || ''))
  }
}

watch(lines, () => nextTick(() => {
  if (logBox.value) logBox.value.scrollTop = logBox.value.scrollHeight
}))

onMounted(() => { loadTail(); timer = setInterval(loadTail, 2000) })
onBeforeUnmount(() => { if (timer) clearInterval(timer) })
</script>

<style scoped>
.dialog-overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5);
}
.dialog-panel {
  width: 90vw; max-width: 900px; max-height: 80vh;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: var(--shadow-popover);
  display: flex; flex-direction: column; overflow: hidden;
}
.dialog-hd {
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 20px; border-bottom: 1px solid var(--color-border);
}
.dialog-hd h2 { font-size: 15px; font-weight: 600; margin: 0; }
.log-box {
  background: #0d1117; color: #58a6ff;
  font-family: ui-monospace, monospace; font-size: 12px;
  padding: 16px; overflow-y: auto; flex: 1; min-height: 50vh;
  white-space: pre-wrap; word-break: break-all;
}
.log-line { line-height: 1.4; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 10px;
  border-radius: 6px; cursor: pointer; font-size: 12px;
  border: 1px solid var(--color-border); background: var(--color-card);
  color: var(--color-foreground);
}
.btn:hover { background: var(--color-muted); }
</style>
