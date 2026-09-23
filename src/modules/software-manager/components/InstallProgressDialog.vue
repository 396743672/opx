<template>
  <div class="progress-toast" :class="{ collapsed }">
    <div class="toast-head" @click="collapsed = !collapsed">
      <div class="toast-title" :class="{ 'title-failed': task.phase === 'failed' }">
        <Icon :icon="phaseIcon" class="phase-icon" />
        <span>{{ phaseText }}</span>
      </div>
      <span class="toast-name">{{ task.name }}</span>
      <div class="toast-head-actions">
        <span class="tnum text-xs text-muted-foreground">{{ percentDisplay }}%</span>
        <Icon :icon="collapsed ? 'mdi:chevron-up' : 'mdi:chevron-down'" class="text-muted-foreground" />
      </div>
    </div>
    <div v-show="!collapsed" class="toast-body">
      <div v-if="task.phase === 'failed' && task.error" class="error-line">
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
        <span v-else-if="task.phase === 'failed'" class="text-destructive">{{ $t('installFailed') }}</span>
        <span v-else class="text-success">{{ $t('installCompleted') }}</span>
        <span class="tnum">{{ percentDisplay }}%</span>
      </div>
    </div>
    <button v-if="task.phase === 'failed' || task.phase === 'completed'" class="dialog-close toast-close" @click="close">
      <Icon icon="mdi:close" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useInstallStore } from '../stores/install'

const { t } = useI18n()
const installStore = useInstallStore()
const collapsed = ref(false)

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
  if (props.task.phase === 'downloading') return props.task.percent ?? 0
  if (props.task.phase === 'extracting') return props.task.percent ?? 0
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
.progress-toast {
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  padding: 10px 14px;
  min-width: 320px;
  max-width: 400px;
  position: relative;
}
.progress-toast.collapsed {
  padding: 0;
}
.toast-head {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  padding: 10px 14px;
}
.collapsed .toast-head {
  padding: 10px 14px;
}
.toast-title {
  font-size: 13px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.title-failed {
  color: var(--color-destructive);
}
.phase-icon {
  color: var(--color-primary);
}
.toast-name {
  font-size: 12px;
  color: var(--color-muted-foreground);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}
.toast-head-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.toast-body {
  padding: 0 14px 10px;
}
.error-line {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-bottom: 6px;
  font-size: 12px;
  color: var(--color-destructive);
  line-height: 1.4;
}
.error-line svg {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
  margin-top: 1px;
}
.prog-bar {
  height: 6px;
  background: var(--color-muted);
  border-radius: 999px;
  overflow: hidden;
  margin-bottom: 6px;
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
  font-size: 11px;
  color: var(--color-muted-foreground);
}
/* 定位与尺寸覆盖全局 .dialog-close（scoped 带 [data-v-*]，特异性更高） */
.toast-close {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 22px;
  height: 22px;
}
.toast-close svg {
  width: 14px;
  height: 14px;
}
</style>
