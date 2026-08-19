<template>
  <div class="instance-card" :class="{ 'has-error': software.status === SoftwareStatus.Error }">
    <div class="card-head">
      <div class="card-title">
        <div class="card-icon" :class="categoryClass">
          <Icon :icon="categoryIcon" />
        </div>
        <div>
          <div class="name">
            {{ software.name }} <span class="ver">{{ software.version }}</span>
          </div>
          <div v-if="software.is_custom" class="tag custom">{{ $t('custom') }}</div>
        </div>
      </div>
      <StatusBadge :status="software.status" :error="software.last_error" />
    </div>

    <div class="card-body">
      <div class="path mono">{{ software.install_path }}</div>
      <div class="meta">
        <span v-if="software.pid" class="kv">
          <Icon icon="mdi:identifier" /> PID <b class="tnum">{{ software.pid }}</b>
        </span>
        <span v-if="software.port" class="kv">
          <Icon icon="mdi:lan" /> {{ $t('port') }} <b class="tnum">{{ software.port }}</b>
        </span>
      </div>
      <div v-if="software.last_error" class="error-text">
        <Icon icon="mdi:alert-circle" /> {{ translateError(software.last_error, t, te) }}
      </div>
    </div>

    <div class="card-actions">
      <button class="btn" :class="{ primary: canStart }" :disabled="!canStart" @click="$emit('start')">
        <Icon icon="mdi:play" /> {{ $t('start') }}
      </button>
      <button class="btn" :class="{ primary: canStop }" :disabled="!canStop" @click="$emit('stop')">
        <Icon icon="mdi:stop" /> {{ $t('stop') }}
      </button>
      <button class="btn" :disabled="!canConfig" @click="$emit('config')">
        <Icon icon="mdi:cog-outline" /> {{ $t('config') }}
      </button>
      <button class="btn" :disabled="!canOps" :title="$t('logs')" @click="$emit('log')">
        <Icon icon="mdi:file-document-outline" /> {{ $t('logs') }}
      </button>
      <button class="btn" :disabled="!canOps" :title="$t('backup')" @click="$emit('backup')">
        <Icon icon="mdi:backup-restore" /> {{ $t('backup') }}
      </button>
      <button class="btn danger" :disabled="!canOps" :title="$t('resetInstance')" @click="$emit('reset')">
        <Icon icon="mdi:rotate-left" /> {{ $t('resetInstance') }}
      </button>
      <button class="btn ghost" :disabled="!canStartupSettings" :title="$t('startupSettings')" @click="$emit('startup-settings')">
        <Icon icon="mdi:tune-vertical" />
      </button>
      <button class="btn danger" :disabled="!canUninstall" :title="uninstallHint" @click="$emit('uninstall')">
        <Icon icon="mdi:delete" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import StatusBadge from './StatusBadge.vue'
import { InstalledSoftware, SoftwareStatus } from '@/models/software'
import { translateError } from '@/utils/i18nError'

const { t, te } = useI18n()

const props = defineProps<{
  software: InstalledSoftware
  actingStates?: Record<string, 'start' | 'stop'>
}>()

defineEmits<{
  start: []
  stop: []
  config: []
  'startup-settings': []
  uninstall: []
  log: []
  backup: []
  reset: []
}>()

const categoryClass = computed(() => {
  switch (props.software.key) {
    case 'jre':
      return 'runtime'
    case 'mysql':
    case 'postgresql':
    case 'mongodb':
      return 'database'
    case 'redis':
      return 'cache'
    case 'nginx':
      return 'webserver'
    case 'minio':
    case 'rustfs':
      return 'storage'
    case 'nacos':
      return 'registry'
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
    case 'registry':
      return 'mdi:hexagon-multiple'
    default:
      return 'mdi:upload'
  }
})

// JRE/JDK 是运行时依赖，不参与启停/配置（由 SpringBoot 应用拉起），仅支持卸载
const isRuntime = computed(() => props.software.key === 'jre' || props.software.key === 'jdk')

const canStart = computed(
  () =>
    !isRuntime.value &&
    (props.software.status === SoftwareStatus.Stopped ||
      props.software.status === SoftwareStatus.Error ||
      props.software.status === SoftwareStatus.Unknown) &&
    props.actingStates?.[props.software.id] !== 'start',
)

const canStop = computed(
  () =>
    !isRuntime.value &&
    (props.software.status === SoftwareStatus.Running ||
      props.software.status === SoftwareStatus.Starting ||
      props.software.status === SoftwareStatus.Initializing ||
      // Error 仅在仍有存活进程时可停止（如健康检查超时）；启动失败/崩溃无 PID，不显示停止
      (props.software.status === SoftwareStatus.Error && props.software.pid != null)) &&
    props.actingStates?.[props.software.id] !== 'stop',
)

// Bug 3 修复：运行中的软件不允许修改配置（Running 状态下配置按钮置灰）
const canConfig = computed(
  () =>
    !isRuntime.value &&
    props.software.status !== SoftwareStatus.Running &&
    props.software.status !== SoftwareStatus.Starting &&
    props.software.status !== SoftwareStatus.Stopping &&
    props.software.status !== SoftwareStatus.Initializing,
)

const canStartupSettings = computed(() => canConfig.value)

// JRE/JDK 是运行时依赖，不支持日志/备份入口（与 isRuntime 一致）
const canOps = computed(() => !isRuntime.value)

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
.instance-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  border-radius: 10px;
  box-shadow: var(--shadow-card);
  transition: border-color 0.15s;
}
.instance-card:hover {
  border-color: color-mix(in oklch, var(--color-primary) 30%, var(--color-border));
}
.instance-card.has-error {
  border-color: color-mix(in oklch, var(--color-destructive) 40%, var(--color-border));
}
.card-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 8px;
}
.card-title {
  display: flex;
  align-items: center;
  gap: 10px;
}
.card-icon {
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
.card-icon.database {
  background: color-mix(in oklch, var(--color-info) 14%, transparent);
  color: var(--color-info);
}
.card-icon.runtime {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.card-icon.cache {
  background: color-mix(in oklch, var(--color-destructive) 14%, transparent);
  color: var(--color-destructive);
}
.card-icon.webserver {
  background: color-mix(in oklch, var(--color-success) 14%, transparent);
  color: var(--color-success);
}
.card-icon.storage {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.card-icon.custom {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.card-icon.registry {
  background: color-mix(in oklch, var(--color-primary) 14%, transparent);
  color: var(--color-primary);
}
.name {
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 2px;
}
.name .ver {
  font-weight: 400;
  color: var(--color-muted-foreground);
  font-size: 13px;
}
.card-body {
  flex: 1;
  min-width: 0;
}
.path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-bottom: 6px;
}
.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
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
  font-size: 11px;
  margin-top: 6px;
  display: flex;
  align-items: center;
  gap: 4px;
}
.tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  display: inline-block;
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
.card-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--color-muted);
}
</style>
