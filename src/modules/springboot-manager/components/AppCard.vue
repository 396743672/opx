<template>
  <div class="rounded-lg border border-border bg-card p-4 shadow-card">
    <div class="flex items-center justify-between mb-2">
      <div class="font-semibold flex items-center gap-2">
        <Icon icon="mdi:spring" class="text-green-500" />
        {{ app.name }}
      </div>
      <span
        class="text-xs px-2 py-0.5 rounded-full font-medium"
        :class="statusClass"
      >
        {{ $t(statusLabel) }}
      </span>
    </div>

    <div class="text-xs text-muted-foreground space-y-0.5 mb-3 font-mono">
      <div class="flex items-center gap-2">
        <span v-if="app.port">{{ $t('port') }}: {{ app.port }}</span>
        <span>{{ $t('version') }}: {{ app.version }}</span>
      </div>
      <div v-if="app.pid">PID: {{ app.pid }}</div>
      <div v-if="app.start_time">{{ $t('startedAt') }}: {{ app.start_time }}</div>
      <div v-if="app.last_error" class="text-red-500">{{ app.last_error }}</div>
    </div>

    <div class="flex gap-2 flex-wrap">
      <button
        v-if="app.status === AppStatus.Stopped || app.status === AppStatus.Error"
        class="btn primary"
        @click="$emit('start', app.id)"
      >
        <Icon icon="mdi:play" /> {{ $t('start') }}
      </button>
      <button
        v-if="app.status === AppStatus.Running"
        class="btn"
        @click="$emit('stop', app.id)"
      >
        <Icon icon="mdi:stop" /> {{ $t('stop') }}
      </button>
      <button
        v-if="app.status === AppStatus.Running"
        class="btn"
        @click="$emit('restart', app.id)"
      >
        <Icon icon="mdi:restart" /> {{ $t('restart') }}
      </button>
      <button
        class="btn"
        :disabled="app.status === AppStatus.Running || app.status === AppStatus.Starting"
        @click="$emit('config', app.id)"
        :title="app.status === AppStatus.Running ? $t('runningAppConfigDisabled') : ''"
      >
        <Icon icon="mdi:pencil" /> {{ $t('edit') }}
      </button>
      <button
        class="btn"
        :disabled="app.status === AppStatus.Running || app.status === AppStatus.Starting"
        @click="$emit('replace', app.id)"
      >
        <Icon icon="mdi:package-up" /> {{ $t('replaceJar') }}
      </button>
      <button
        v-if="app.status === AppStatus.Running && app.jdk_type !== 'jre'"
        class="btn"
        @click="$emit('monitor', app.id)"
      >
        <Icon icon="mdi:chart-line" /> {{ $t('jvmMonitor') }}
      </button>
      <button class="btn" @click="$emit('logs', app.id)">
        <Icon icon="mdi:file-document-outline" /> {{ $t('viewLogs') }}
      </button>
      <button
        class="btn danger"
        :disabled="app.status === AppStatus.Running || app.status === AppStatus.Starting"
        @click="$emit('delete', app.id)"
      >
        <Icon icon="mdi:delete" /> {{ $t('delete') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { AppStatus, type SpringBootApp } from '@/models/springboot'

const props = defineProps<{
  app: SpringBootApp
}>()

defineEmits<{
  start: [id: string]
  stop: [id: string]
  restart: [id: string]
  config: [id: string]
  replace: [id: string]
  monitor: [id: string]
  logs: [id: string]
  delete: [id: string]
}>()

const statusClass = computed(() => {
  switch (props.app.status) {
    case AppStatus.Running: return 'bg-green-100 text-green-700'
    case AppStatus.Stopped: return 'bg-muted text-muted-foreground'
    case AppStatus.Error: return 'bg-red-100 text-red-700'
    case AppStatus.Starting:
    case AppStatus.Stopping: return 'bg-amber-100 text-amber-700'
    default: return 'bg-muted text-muted-foreground'
  }
})

const statusLabel = computed(() => {
  switch (props.app.status) {
    case AppStatus.Running: return 'running'
    case AppStatus.Stopped: return 'stopped'
    case AppStatus.Error: return 'error'
    case AppStatus.Starting: return 'starting'
    case AppStatus.Stopping: return 'stopping'
    default: return 'unknown'
  }
})
</script>

<style scoped>
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px;
  border-radius: 6px; cursor: pointer; font-size: 13px;
  border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); white-space: nowrap;
}
.btn:hover { background: var(--color-muted); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn.primary:hover { background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background)); }
.btn.danger { background: var(--color-destructive); color: white; border-color: var(--color-destructive); }
.btn.danger:hover { opacity: 0.9; }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
