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
      <StatusBadge v-if="!isRuntime" :status="software.status" :error="software.last_error" />
    </div>

    <div class="card-body">
      <div class="path mono">{{ software.install_path }}</div>
      <div class="meta">
        <span v-if="software.pid" class="kv">
          <Icon icon="mdi:identifier" /> PID <b class="tnum">{{ software.pid }}</b>
        </span>
        <span v-if="runtimePort" class="kv">
          <Icon icon="mdi:lan" /> {{ $t('port') }}
          <a
            v-if="webUrl && canOpenWeb"
            class="tnum web-open"
            :href="webUrl"
            target="_blank"
            rel="noopener"
            :title="$t('openInBrowser')"
          >{{ runtimePort }} <Icon icon="mdi:open-in-new" /></a>
          <b v-else class="tnum">{{ runtimePort }}</b>
        </span>
        <span v-if="depsNames.length" class="kv">
          <Icon icon="mdi:graph-outline" /> {{ $t('deps') }}
          <span class="deps-list">
            <span v-for="d in depsNames" :key="d" class="dep-chip">
              <Icon icon="mdi:lan-connect" /> {{ d }}
            </span>
          </span>
        </span>
      </div>
      <div v-if="software.last_error" class="error-text">
        <Icon icon="mdi:alert-circle" /> {{ translateError(software.last_error, t, te) }}
      </div>
    </div>

    <div class="card-actions">
      <template v-if="!isRuntime">
        <button
          v-if="upgradeTo"
          class="btn primary"
          :disabled="actingStates?.[software.id] != null"
          :title="t('upgradeHint')"
          @click="$emit('upgrade')"
        >
          <Icon icon="mdi:package-up" /> {{ $t('upgradeTo', { v: upgradeTo }) }}
        </button>
        <button
          v-if="rollbackTo"
          class="btn"
          :disabled="actingStates?.[software.id] != null"
          :title="t('rollbackTo', { v: rollbackTo })"
          @click="$emit('rollback')"
        >
          <Icon icon="mdi:history" /> {{ $t('rollbackTo', { v: rollbackTo }) }}
        </button>
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
        <button class="btn" :disabled="!canStartupSettings" :title="$t('startupSettings')" @click="$emit('startup-settings')">
          <Icon icon="mdi:tune-vertical" /> {{ $t('startupSettings') }}
        </button>
        <button class="btn" :disabled="!canStartupSettings" :title="$t('depsEdit')" @click="$emit('deps')">
          <Icon icon="mdi:graph-outline" /> {{ $t('depsEdit') }}
        </button>
      </template>
      <button class="btn danger" :disabled="!canUninstall" :title="uninstallHint" @click="$emit('uninstall')">
        <Icon icon="mdi:delete" /> {{ $t('uninstall') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import StatusBadge from './StatusBadge.vue'
import { InstalledSoftware, SoftwareCategory, SoftwareStatus } from '@/models/software'
import { translateError } from '@/utils/i18nError'

const { t, te } = useI18n()

const props = defineProps<{
  software: InstalledSoftware
  actingStates?: Record<string, 'start' | 'stop'>
  upgradeTo?: string | null
  rollbackTo?: string | null
  /** 依赖 id → 显示名（用于展示已配置依赖友好名，缺省回退为 id） */
  depsNameMap?: Record<string, string>
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
  upgrade: []
  rollback: []
  deps: []
}>()

const CATEGORY_CLASS: Record<string, string> = {
  Database: 'database',
  Runtime: 'runtime',
  Cache: 'cache',
  WebServer: 'webserver',
  Storage: 'storage',
  Registry: 'registry',
  MessageQueue: 'messagequeue',
  Search: 'search',
  TimeSeries: 'timeseries',
}

const categoryClass = computed(() => CATEGORY_CLASS[props.software.category ?? ''] ?? 'custom')

// 图标：优先 catalog 软件专属图标（与软件仓库一致）；自定义软件回退为上传图标
const categoryIcon = computed(() => props.software.icon || 'mdi:upload')

// 运行端口：优先运行时字段，其次按软件从 config 取对应端口字段（默认值兜底）
const runtimePort = computed(() => {
  const s = props.software
  const cfg = s.config || {}
  if (s.port) return s.port
  switch (s.key) {
    case 'minio':
      return cfg.console_port || 9001
    case 'nacos':
      return cfg.console_port || 8080
    case 'nginx':
      return cfg.listen || 80
    case 'elasticsearch':
      return cfg.port || 9200
    case 'influxdb':
      return cfg.port || 8086
    case 'influxdb3':
      return cfg.port || 8181
    default:
      return cfg.port || 0
  }
})

// 可网页访问的软件返回访问地址，否则 null（数据库/Kafka/JRE 等走非 HTTP 协议）
const webUrl = computed(() => {
  const s = props.software
  const host = '127.0.0.1'
  const port = runtimePort.value
  switch (s.key) {
    case 'elasticsearch':
    case 'influxdb':
    case 'influxdb3':
    case 'nginx':
    case 'minio':
      return `http://${host}:${port}`
    case 'nacos': {
      const path = s.config?.context_path || '/nacos'
      return `http://${host}:${port}${path}`
    }
  }
  // 自定义软件：Http 健康检查的 url 即网页地址
  const hc = s.custom_start_command?.health_check
  if (s.custom_start_command && hc?.kind === 'Http' && hc.spec?.url) return hc.spec.url
  return null
})

const canOpenWeb = computed(
  () =>
    props.software.status === SoftwareStatus.Running ||
    props.software.status === SoftwareStatus.Starting,
)

// JRE/JDK 是运行时依赖，不参与启停/配置（由 SpringBoot 应用拉起），仅支持卸载
const isRuntime = computed(() => props.software.category === SoftwareCategory.Runtime)

// 已配置依赖的显示名列表（缺省回退为 id）
const depsNames = computed(() =>
  (props.software.depends_on ?? []).map((id) => props.depsNameMap?.[id] || id),
)

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
.card-icon.messagequeue {
  background: color-mix(in oklch, var(--color-primary) 14%, transparent);
  color: var(--color-primary);
}
.card-icon.search {
  background: color-mix(in oklch, var(--color-success) 14%, transparent);
  color: var(--color-success);
}
.card-icon.timeseries {
  background: color-mix(in oklch, var(--color-info) 14%, transparent);
  color: var(--color-info);
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
.deps-list {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 4px;
}
.dep-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: color-mix(in oklch, var(--color-primary) 10%, transparent);
  color: var(--color-primary);
  font-weight: 500;
}
.dep-chip svg {
  width: 10px;
  height: 10px;
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
.web-open {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: var(--color-primary);
  font-weight: 600;
  text-decoration: none;
}
.web-open:hover {
  text-decoration: underline;
}
.web-open svg {
  width: 11px;
  height: 11px;
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
