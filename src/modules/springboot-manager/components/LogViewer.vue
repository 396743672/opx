<template>
  <Teleport to="body">
    <div class="dialog-overlay" @click.self="$emit('close')">
      <div class="dialog-panel">
        <div class="dialog-header">
          <h2>{{ $t('viewLogs') }} - {{ appName }}</h2>
          <div class="flex items-center gap-2">
            <button class="btn" @click="loadTail">{{ $t('refresh') }}</button>
            <button class="btn" @click="$emit('close')"><Icon icon="mdi:close" /></button>
          </div>
        </div>
        <div class="log-box" ref="logContainer">
          <div v-if="error" class="text-red-400 p-4">{{ error }}</div>
          <div v-else-if="lines.length === 0" class="text-gray-500 p-4">{{ $t('noLogs') }}</div>
          <div v-for="(line, i) in lines" :key="i" class="log-line">{{ line }}</div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { readTextFile } from '@tauri-apps/plugin-fs'

const props = defineProps<{ appId: string; appName: string; logPath: string }>()
defineEmits<{ close: [] }>()

const lines = ref<string[]>([])
const error = ref('')
const logContainer = ref<HTMLElement | null>(null)

async function loadTail() {
  error.value = ''
  if (!props.logPath) { error.value = '日志路径未配置'; return }
  try {
    const content = await readTextFile(props.logPath)
    lines.value = content ? content.split('\n').slice(-500) : []
  } catch {
    error.value = '无法读取日志文件: ' + props.logPath
    lines.value = []
  }
}

watch(lines, () => nextTick(() => {
  if (logContainer.value) logContainer.value.scrollTop = logContainer.value.scrollHeight
}))

onMounted(loadTail)
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
.dialog-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px; border-bottom: 1px solid var(--color-border);
}
.dialog-header h2 { font-size: 15px; font-weight: 600; margin: 0; }
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
