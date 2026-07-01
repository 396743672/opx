<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title" :class="{ 'title-failed': task.phase === 'failed' }">
          <Icon :icon="phaseIcon" class="phase-icon" />
          {{ phaseText }}
        </div>
        <button
          v-if="task.phase === 'failed' || task.phase === 'completed'"
          class="dialog-close"
          @click="close"
        >
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="prog-stage">{{ task.name }}</div>
      <div v-if="task.phase === 'failed' && task.error" class="error-box">
        <Icon icon="mdi:alert-circle" />
        <span>{{ task.error }}</span>
      </div>

      <div class="prog-bar" :class="{ indeterminate: task.total == null && task.phase === 'downloading' }">
        <i :style="{ width: percentDisplay + '%' }"></i>
      </div>
      <div class="prog-stats">
        <span v-if="task.phase === 'downloading'">
          {{ formatSize(task.downloaded) }}
          <span v-if="task.total != null">/ {{ formatSize(task.total) }}</span>
        </span>
        <span v-else-if="task.phase === 'extracting'">{{ $t('extracting') }}</span>
        <span v-else-if="task.phase === 'failed'" class="error-text">{{ task.error }}</span>
        <span class="tnum">{{ percentDisplay }}%</span>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useInstallStore } from '../stores/install'

const { t } = useI18n()

const installStore = useInstallStore()

function close() {
  installStore.removeTask(props.task.id)
}

const props = defineProps<{
  task: {
    id: string
    key: string
    name: string
    phase: string
    downloaded: number
    total: number | null
    percent: number | null
    error: string | null
    installedId: string | null
  }
}>()

const phaseIcon = computed(() => {
  if (props.task.phase === 'downloading') return 'mdi:download'
  if (props.task.phase === 'extracting') return 'mdi:package-variant-closed'
  if (props.task.phase === 'completed') return 'mdi:check-circle'
  return 'mdi:alert-circle'
})

const phaseText = computed(() => {
  if (props.task.phase === 'downloading') return t('downloading')
  if (props.task.phase === 'extracting') return t('extracting')
  if (props.task.phase === 'completed') return t('installCompleted')
  if (props.task.phase === 'failed') return t('installFailed')
  return ''
})

const percentDisplay = computed(() => {
  if (props.task.phase === 'downloading') {
    return props.task.percent ?? 0
  }
  if (props.task.phase === 'extracting') {
    return props.task.percent ?? 0
  }
  if (props.task.phase === 'completed') return 100
  return 0
})

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.phase-icon {
  color: var(--color-primary);
}
.prog-stage {
  font-size: 13px;
  color: var(--color-muted-foreground);
  margin-bottom: 10px;
}
.prog-bar {
  height: 8px;
  background: var(--color-muted);
  border-radius: 999px;
  overflow: hidden;
  margin-bottom: 8px;
}
.prog-bar i {
  display: block;
  height: 100%;
  border-radius: 999px;
  background: var(--color-primary);
  transition: width 0.3s ease-out;
}
.prog-bar.indeterminate i {
  width: 40% !important;
  animation: indet 1.4s ease-in-out infinite;
}
@keyframes indet {
  0% { margin-left: -40%; }
  50% { margin-left: 100%; }
  100% { margin-left: 100%; }
}
.prog-stats {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.tnum {
  font-variant-numeric: tabular-nums;
}
.error-text {
  color: var(--color-destructive);
}
.title-failed {
  color: var(--color-destructive);
}
.error-box {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-bottom: 12px;
  padding: 10px 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-destructive) 10%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 30%, transparent);
  color: var(--color-destructive);
  font-size: 12px;
  line-height: 1.5;
  word-break: break-all;
}
.error-box svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  margin-top: 1px;
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.dialog-close svg {
  width: 18px;
  height: 18px;
}
</style>
