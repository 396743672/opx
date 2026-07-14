<template>
  <div class="dialog-overlay" @click.self="$emit('close')">
    <div class="dialog-panel max-w-4xl max-h-[80vh]">
      <div class="dialog-header flex items-center justify-between">
        <h2>{{ $t('viewLogs') }} - {{ appName }}</h2>
        <div class="flex items-center gap-2">
          <button class="btn text-sm" @click="autoScroll = !autoScroll">
            {{ autoScroll ? $t('autoScroll') : $t('manualScroll') }}
          </button>
          <button class="btn text-sm" @click="lines = []">{{ $t('clear') }}</button>
        </div>
      </div>
      <div class="bg-black text-green-400 font-mono text-xs p-4 overflow-y-auto" style="height: 60vh;" ref="logContainer">
        <div v-if="lines.length === 0" class="text-gray-500">{{ $t('noLogs') }}</div>
        <div v-for="(line, i) in lines" :key="i">{{ line }}</div>
      </div>
      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('close') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick, watch } from 'vue'
import { readTextFile } from '@tauri-apps/plugin-fs'

const props = defineProps<{
  appId: string
  appName: string
  logPath: string
}>()

defineEmits<{ close: [] }>()

const lines = ref<string[]>([])
const logContainer = ref<HTMLElement | null>(null)
const autoScroll = ref(true)

async function loadTail() {
  if (!props.logPath) return
  try {
    const content = await readTextFile(props.logPath)
    const all = content.split('\n')
    lines.value = all.slice(-500)
  } catch {
    lines.value = ['[日志文件不可读]']
  }
}

watch(lines, () => {
  if (autoScroll.value) {
    nextTick(() => {
      if (logContainer.value) {
        logContainer.value.scrollTop = logContainer.value.scrollHeight
      }
    })
  }
})

onMounted(async () => {
  await loadTail()
})
</script>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.dialog-panel {
  width: 90vw;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.dialog-header {
  padding: 16px 20px;
  border-bottom: 1px solid var(--color-border);
  font-size: 15px;
  font-weight: 600;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--color-border);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
  white-space: nowrap;
}
.btn:hover {
  background: var(--color-muted);
}
</style>
