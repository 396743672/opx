<template>
  <div class="run-panel">
    <div class="panel-head">
      <div>
        <div class="panel-title">{{ stack.name }}</div>
        <div class="panel-sub" v-if="stack.description">{{ stack.description }}</div>
      </div>
      <div class="overall" :class="overallClass">
        <Icon :icon="overallIcon" />
        {{ $t(overallLabel) }}
      </div>
    </div>

    <!-- 成员实时状态 -->
    <div class="section-title">
      <Icon icon="mdi:account-group" /> {{ $t('memberStatus') }}
      <span class="progress">{{ runningCount }} / {{ stack.items.length }}</span>
    </div>

    <div v-if="stack.items.length === 0" class="empty">{{ $t('noMembers') }}</div>

    <div class="member-grid">
      <div
        v-for="item in stack.items"
        :key="item.ref_id"
        class="member-card"
        :class="statusClass(runtimeOf(item.ref_id)?.status)"
      >
        <div class="mc-top">
          <span
            class="type-badge"
            :class="item.ref_type === 'software' ? 't-sw' : 't-sb'"
          >
            {{ item.ref_type === 'software' ? $t('softwareType') : $t('springbootType') }}
          </span>
          <span class="mc-name">{{ resolveName(item) }}</span>
        </div>
        <div class="mc-bottom">
          <span class="status-dot" :class="statusClass(runtimeOf(item.ref_id)?.status)"></span>
          <span class="status-text">{{ $t(statusLabel(runtimeOf(item.ref_id)?.status)) }}</span>
        </div>
        <div class="mc-actions">
          <button v-if="portOf(item)" class="mini-btn" @click="openPort(item)">
            <Icon icon="mdi:open-in-new" /> {{ portOf(item) }}
          </button>
          <button class="mini-btn" @click="openLogs(item)">
            <Icon icon="mdi:file-document-outline" /> {{ $t('viewLogs') }}
          </button>
        </div>
        <div v-if="runtimeOf(item.ref_id)?.message" class="mc-msg">
          {{ runtimeOf(item.ref_id)?.message }}
        </div>
        <div v-if="item.depends_on.length" class="mc-dep">
          {{ $t('dependsOn') }}:
          <span v-for="d in item.depends_on" :key="d" class="dep-chip">{{ resolveDep(d) }}</span>
        </div>
      </div>
    </div>

    <!-- 最近一次启动报告 -->
    <div v-if="report" class="report-block">
      <div class="section-title">
        <Icon icon="mdi:chart-timeline-variant" /> {{ $t('lastRunReport') }}
        <span class="report-total">{{ $t('totalElapsed') }}: {{ fmtMs(report.total_elapsed_ms) }}</span>
      </div>
      <div v-for="m in report.members" :key="m.ref_id" class="report-row" :class="'r-' + m.status">
        <span class="r-name">{{ nameOf(m.ref_id) }}</span>
        <span class="r-bar"><i :style="{ width: pct(m) }"></i></span>
        <span class="r-time">{{ fmtMs(m.elapsed_ms) }}</span>
        <span v-if="m.message" class="r-msg">{{ m.message }}</span>
      </div>
    </div>

    <!-- 日志查看弹窗 -->
    <LogViewerDialog
      v-if="logTarget?.kind === 'software'"
      :software="logTarget.software"
      @close="logTarget = null"
    />
    <LogViewer
      v-else-if="logTarget?.kind === 'springboot'"
      :app-id="logTarget.id"
      :app-name="logTarget.name"
      :log-path="logTarget.logPath"
      @close="logTarget = null"
    />

    <!-- P2 预留（R11 栈级自启 / R12 模板 / R13 启动报告），本期仅 UI 占位，逻辑 TODO(P2) -->
    <div class="p2-block">
      <div class="p2-title">
        <Icon icon="mdi:flask-outline" /> {{ $t('stackAutoStart') }} /
        {{ $t('stackTemplates') }} / {{ $t('startupReport') }}
        <span class="p2-tag">{{ $t('p2Reserved') }}</span>
      </div>
      <div class="p2-row">
        <label class="p2-toggle">
          <input type="checkbox" disabled />
          {{ $t('stackAutoStart') }}
        </label>
        <button class="p2-btn" disabled>{{ $t('stackTemplates') }}</button>
        <button class="p2-btn" disabled>{{ $t('startupReport') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { Icon } from '@iconify/vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useStackStore } from '@/stores/stack'
import LogViewerDialog from '@/modules/software-manager/components/LogViewerDialog.vue'
import LogViewer from '@/modules/springboot-manager/components/LogViewer.vue'
import type { InstalledSoftware } from '@/models/software'
import type {
  Stack,
  StackItem,
  StackMemberReport,
  StackMemberStatus,
  StackMemberRuntime,
} from '@/models/stack'

const props = defineProps<{ stack: Stack }>()
const store = useStackStore()

const runtime = computed<StackMemberRuntime[]>(() => store.getRuntime(props.stack.id))

function runtimeOf(refId: string): StackMemberRuntime | undefined {
  return runtime.value.find((m) => m.ref_id === refId)
}

function resolveName(item: StackItem): string {
  return store.resolveName(item)
}
function resolveDep(refId: string): string {
  const item: StackItem | undefined = props.stack.items.find((i) => i.ref_id === refId)
  if (item) return resolveName(item)
  // 组外依赖：从已装软件 / Spring Boot 应用候选解析名称
  return (
    store.installedSoftware.find((s) => s.id === refId)?.name ??
    store.springbootApps.find((a) => a.id === refId)?.name ??
    refId
  )
}

const runningCount = computed(
  () => runtime.value.filter((m) => m.status === 'running').length
)

// ===== 启动报告 + 端口/日志入口 =====
type LogTarget =
  | { kind: 'software'; software: InstalledSoftware }
  | { kind: 'springboot'; id: string; name: string; logPath: string }
  | null

const logTarget = ref<LogTarget>(null)
const report = computed(() => props.stack.last_run_report ?? null)

function nameOf(refId: string): string {
  const stackItem = props.stack.items.find((i) => i.ref_id === refId)
  if (stackItem) return store.resolveName(stackItem)
  return (
    store.installedSoftware.find((s) => s.id === refId)?.name ??
    store.springbootApps.find((a) => a.id === refId)?.name ??
    refId
  )
}

function portOf(item: StackItem): number | null {
  if (item.ref_type === 'software') {
    const sw = store.installedSoftware.find((s) => s.id === item.ref_id)
    return sw?.port && sw.port > 0 ? sw.port : null
  }
  const a = store.springbootApps.find((x) => x.id === item.ref_id)
  return a?.port ? a.port : null
}

async function openPort(item: StackItem) {
  const p = portOf(item)
  if (p) await openUrl(`http://127.0.0.1:${p}`)
}

function openLogs(item: StackItem) {
  if (item.ref_type === 'software') {
    const sw = store.installedSoftware.find((s) => s.id === item.ref_id)
    if (sw) logTarget.value = { kind: 'software', software: sw }
  } else {
    const a = store.springbootApps.find((x) => x.id === item.ref_id)
    if (a) logTarget.value = { kind: 'springboot', id: a.id, name: a.name, logPath: a.log_path }
  }
}

function fmtMs(ms: number): string {
  if (ms >= 1000) return (ms / 1000).toFixed(1) + 's'
  return ms + 'ms'
}

function reportMax(): number {
  const r = report.value
  if (!r || !r.members.length) return 1
  return Math.max(...r.members.map((m) => m.elapsed_ms))
}

function pct(m: StackMemberReport): string {
  return Math.round((m.elapsed_ms / reportMax()) * 100) + '%'
}

const overallStatus = computed<StackMemberStatus>(() => {
  if (runtime.value.length === 0) return 'pending'
  if (runtime.value.some((m) => m.status === 'failed')) return 'failed'
  if (runtime.value.every((m) => m.status === 'running')) return 'running'
  if (runtime.value.some((m) => m.status === 'starting')) return 'starting'
  if (runtime.value.some((m) => m.status === 'stopping')) return 'stopping'
  if (runtime.value.every((m) => m.status === 'stopped')) return 'stopped'
  return 'pending'
})

const overallClass = computed(() => statusClass(overallStatus.value))
const overallIcon = computed(() => {
  switch (overallStatus.value) {
    case 'running':
      return 'mdi:check-circle'
    case 'failed':
      return 'mdi:alert-circle'
    case 'starting':
      return 'mdi:loading'
    case 'stopping':
      return 'mdi:stop-circle'
    default:
      return 'mdi:circle-outline'
  }
})
const overallLabel = computed(() => statusLabel(overallStatus.value))

function statusLabel(s?: StackMemberStatus): string {
  switch (s) {
    case 'running':
      return 'stackRunning'
    case 'failed':
      return 'stackFailed'
    case 'starting':
      return 'stackStarting'
    case 'stopping':
      return 'stackStopping'
    case 'stopped':
      return 'stackStopped'
    default:
      return 'stackPending'
  }
}

function statusClass(s?: StackMemberStatus): string {
  return `st-${s ?? 'pending'}`
}
</script>

<style scoped>
.run-panel {
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-card);
  padding: 16px;
}
.panel-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 14px;
}
.panel-title {
  font-size: 16px;
  font-weight: 600;
}
.panel-sub {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
}
.overall {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  padding: 4px 10px;
  border-radius: 999px;
  font-weight: 500;
}
.section-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 10px;
}
.progress {
  margin-left: auto;
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.empty {
  font-size: 13px;
  color: var(--color-muted-foreground);
  padding: 10px 0;
}
.member-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
}
.member-card {
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 10px;
  background: var(--color-muted);
}
.mc-top {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 6px;
}
.mc-name {
  font-size: 13px;
  font-weight: 500;
}
.mc-bottom {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}
.mc-msg {
  font-size: 11px;
  color: var(--color-danger, red);
  margin-top: 4px;
  word-break: break-all;
}
.mc-dep {
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 6px;
}
.dep-chip {
  display: inline-block;
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  padding: 0 5px;
  margin: 0 2px;
}
.type-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
}
.type-badge.t-sw {
  background: var(--color-primary/15);
  color: var(--color-primary);
}
.type-badge.t-sb {
  background: color-mix(in oklch, green 15%, transparent);
  color: green;
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--color-muted-foreground);
}

/* 状态着色 */
.st-running {
  border-color: color-mix(in oklch, green 50%, var(--color-border));
}
.st-running > .mc-bottom .status-dot,
.st-running.status-dot {
  background: green;
}
.st-failed {
  border-color: var(--color-danger, red);
}
.st-failed > .mc-bottom .status-dot,
.st-failed.status-dot {
  background: var(--color-danger, red);
}
.st-starting > .mc-bottom .status-dot,
.st-starting.status-dot,
.overall.st-starting {
  background: var(--color-primary);
}
.st-stopping > .mc-bottom .status-dot,
.st-stopping.status-dot {
  background: orange;
}
.st-stopped > .mc-bottom .status-dot,
.st-stopped.status-dot {
  background: var(--color-muted-foreground);
}
.st-pending > .mc-bottom .status-dot,
.st-pending.status-dot {
  background: var(--color-muted-foreground);
}

.p2-block {
  margin-top: 16px;
  border: 1px dashed var(--color-border);
  border-radius: 8px;
  padding: 10px 12px;
  opacity: 0.75;
}
.p2-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 8px;
}
.p2-tag {
  font-size: 10px;
  color: var(--color-muted-foreground);
  font-weight: 400;
}
.p2-row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
}
.p2-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.p2-btn {
  height: 28px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
  cursor: not-allowed;
}

/* 成员操作：端口 / 日志 */
.mc-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  flex-wrap: wrap;
}
.mini-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  font-size: 11px;
  cursor: pointer;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-primary);
}
.mini-btn:hover { background: var(--color-muted); }

/* 最近一次启动报告 */
.report-block {
  margin-top: 16px;
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 10px 12px;
  background: var(--color-card);
}
.report-total {
  margin-left: auto;
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.report-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  padding: 3px 0;
}
.r-name {
  width: 120px;
  flex: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.r-bar {
  flex: 1;
  height: 6px;
  border-radius: 999px;
  background: var(--color-muted);
  overflow: hidden;
}
.r-bar i {
  display: block;
  height: 100%;
  border-radius: 999px;
  background: var(--color-primary);
}
.report-row.r-failed .r-bar i { background: var(--color-danger, red); }
.report-row.r-stopped .r-bar i { background: var(--color-muted-foreground); }
.r-time {
  width: 64px;
  flex: none;
  text-align: right;
  font-variant-numeric: tabular-nums;
  color: var(--color-muted-foreground);
}
.r-msg {
  flex: none;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--color-danger, red);
}
</style>
