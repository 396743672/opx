<template>
  <div class="instance-row" :class="{ 'has-error': software.status === SoftwareStatus.Error }">
    <div class="row-icon" :class="categoryClass">
      <Icon :icon="categoryIcon" />
    </div>
    <div class="row-main">
      <div class="row-name">
        {{ software.name }} <span class="ver">{{ software.version }}</span>
        <span v-if="software.is_custom" class="tag custom">{{ $t('custom') }}</span>
      </div>
      <div class="row-path mono">{{ software.install_path }}</div>
      <div class="row-meta">
        <span v-if="software.pid" class="kv">
          <Icon icon="mdi:identifier" /> PID <b class="tnum">{{ software.pid }}</b>
        </span>
        <span v-if="software.port" class="kv">
          <Icon icon="mdi:ethernet-port" /> {{ $t('port') }} <b class="tnum">{{ software.port }}</b>
        </span>
        <span v-if="software.last_error" class="kv error-text">
          <Icon icon="mdi:alert-circle" /> {{ software.last_error }}
        </span>
      </div>
    </div>
    <StatusBadge :status="software.status" :error="software.last_error" />
    <div class="row-actions">
      <button class="btn small" :class="{ primary: canStart }" :disabled="!canStart" @click="$emit('start')">
        <Icon icon="mdi:play" /> {{ $t('start') }}
      </button>
      <button class="btn small" :class="{ primary: canStop }" :disabled="!canStop" @click="$emit('stop')">
        <Icon icon="mdi:stop" /> {{ $t('stop') }}
      </button>
      <button class="btn small" :disabled="!canConfig" @click="$emit('config')">
        <Icon icon="mdi:cog-outline" /> {{ $t('config') }}
      </button>
      <button class="btn small ghost" :disabled="!canStartupSettings" :title="$t('startupSettings')" @click="$emit('startup-settings')">
        <Icon icon="mdi:tune-vertical" />
      </button>
      <button class="btn small danger" :disabled="!canUninstall" :title="uninstallHint" @click="$emit('uninstall')">
        <Icon icon="mdi:delete" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import StatusBadge from './StatusBadge.vue'
import { InstalledSoftware, SoftwareStatus } from '@/models/software'

const props = defineProps<{
  software: InstalledSoftware
}>()

defineEmits<{
  start: []
  stop: []
  config: []
  'startup-settings': []
  uninstall: []
}>()

const categoryClass = computed(() => {
  switch (props.software.key) {
    case 'jre':
      return 'runtime'
    case 'mysql':
      return 'database'
    case 'redis':
      return 'cache'
    case 'nginx':
      return 'webserver'
    case 'minio':
    case 'rustfs':
      return 'storage'
    default:
      return 'custom'
  }
})

const categoryIcon = computed(() => {
  switch (categoryClass.value) {
    case 'runtime':
      return 'mdi:language-java'
    case 'database':
      return 'mdi:database'
    case 'cache':
      return 'mdi:lightning-bolt'
    case 'webserver':
      return 'mdi:web'
    case 'storage':
      return 'mdi:storage'
    default:
      return 'mdi:upload'
  }
})

// JRE 是运行时依赖，不参与启停/配置（由 SpringBoot 应用拉起），仅支持卸载
const isRuntime = computed(() => props.software.key === 'jre')

const canStart = computed(
  () =>
    !isRuntime.value &&
    (props.software.status === SoftwareStatus.Stopped ||
      props.software.status === SoftwareStatus.Error ||
      props.software.status === SoftwareStatus.Unknown),
)

const canStop = computed(
  () =>
    !isRuntime.value &&
    (props.software.status === SoftwareStatus.Running ||
      props.software.status === SoftwareStatus.Starting ||
      props.software.status === SoftwareStatus.Error),
)

const canConfig = computed(
  () =>
    !isRuntime.value &&
    props.software.status !== SoftwareStatus.Starting &&
    props.software.status !== SoftwareStatus.Stopping &&
    props.software.status !== SoftwareStatus.Initializing,
)

const canStartupSettings = computed(() => canConfig.value)

const canUninstall = computed(
  () =>
    props.software.status === SoftwareStatus.Stopped ||
    props.software.status === SoftwareStatus.Error ||
    props.software.status === SoftwareStatus.Unknown ||
    props.software.status === SoftwareStatus.Initializing,
)

const uninstallHint = computed(() => (canUninstall.value ? '' : '请先停止后再卸载'))
</script>

<style scoped>
.instance-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  transition: border-color 0.15s;
}
.instance-row:hover {
  border-color: color-mix(in oklch, var(--color-primary) 30%, var(--color-border));
}
.instance-row.has-error {
  border-color: color-mix(in oklch, var(--color-destructive) 40%, var(--color-border));
}
.row-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  flex-shrink: 0;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.row-icon.database {
  background: color-mix(in oklch, var(--color-info) 14%, transparent);
  color: var(--color-info);
}
.row-icon.runtime {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.row-icon.cache {
  background: color-mix(in oklch, var(--color-destructive) 14%, transparent);
  color: var(--color-destructive);
}
.row-icon.webserver {
  background: color-mix(in oklch, var(--color-success) 14%, transparent);
  color: var(--color-success);
}
.row-icon.storage {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.row-icon.custom {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.row-main {
  flex: 1;
  min-width: 0;
}
.row-name {
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.row-name .ver {
  font-weight: 400;
  color: var(--color-muted-foreground);
  font-size: 13px;
}
.row-path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-meta {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 4px;
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.kv {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.kv svg {
  width: 12px;
  height: 12px;
}
.error-text {
  color: var(--color-destructive);
}
.tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
}
.tag.custom {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
}
.tnum {
  font-variant-numeric: tabular-nums;
}
.row-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
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
}
.btn:hover {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.danger {
  background: var(--color-destructive);
  color: white;
  border-color: var(--color-destructive);
}
.btn.ghost {
  background: transparent;
  border-color: transparent;
}
.btn.small {
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn svg {
  width: 16px;
  height: 16px;
}
</style>
