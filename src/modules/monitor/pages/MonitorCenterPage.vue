<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:chart-timeline-variant" :title="$t('monitorCenter')">
      <template #actions>
        <input
          v-model="searchKeyword"
          class="h-8 px-2 w-48 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          :placeholder="$t('searchProcessPlaceholder')"
        />
        <button class="btn" @click="loadMetricsHistory">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
      </template>
    </PageHeader>

    <!-- KPI -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
      <StatCard
        :label="$t('kpiRunningProcesses')"
        :value="processRows.length"
        icon="mdi:server"
        accent="primary"
      />
      <StatCard
        :label="$t('kpiCpuAlerting')"
        :value="alertingCpu"
        icon="mdi:alert-circle-outline"
        :accent="alertingCpu > 0 ? 'destructive' : 'chart-2'"
      />
      <StatCard
        :label="$t('kpiMemAlerting')"
        :value="alertingMem"
        icon="mdi:alert-circle-outline"
        :accent="alertingMem > 0 ? 'destructive' : 'chart-2'"
      />
    </div>

    <!-- 进程列表 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card">
      <CardHeader icon="mdi:chart-timeline-variant" :title="$t('processMonitor')" hide-refresh />
      <!-- 告警机制提示：说明采样/阈值/触发条件，避免「改了阈值却没记录」的困惑 -->
      <div class="text-xs text-muted-foreground mb-2" style="overflow-wrap: anywhere">
        {{ $t('alertHintLine', alertHintParams) }}
        <span v-if="processRows.length === 0"> · {{ $t('alertNoRunning') }}</span>
      </div>
      <div v-if="processRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noRunningProcess') }}
      </div>
      <div v-else-if="filteredRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noMatchingProcess') }}
      </div>
      <div v-else class="space-y-1">
        <div v-for="row in filteredRows" :key="row.pid" class="border border-border rounded-md">
          <div class="flex items-center justify-between px-3 py-2 text-sm">
            <span class="flex items-center gap-2 min-w-0">
              <Icon
                :icon="row.type === 'springboot' ? 'mdi:leaf' : 'mdi:server'"
                :class="row.type === 'springboot' ? 'text-green-500' : 'text-info'"
              />
              <span class="truncate">{{ row.name }}</span>
              <span class="text-xs text-muted-foreground tnum">({{ row.pid }})</span>
            </span>
            <span class="flex items-center gap-4 shrink-0">
              <span
                class="text-xs tnum flex items-center gap-1"
                :class="row.cpu >= thresholds.cpu ? 'text-destructive' : ''"
              >
                <Icon v-if="row.cpu >= thresholds.cpu" icon="mdi:alert" />
                CPU {{ row.cpu.toFixed(1) }}%
              </span>
              <span
                class="text-xs tnum flex items-center gap-1"
                :class="row.memPct >= thresholds.mem ? 'text-destructive' : ''"
              >
                <Icon v-if="row.memPct >= thresholds.mem" icon="mdi:alert" />
                {{ $t('memory') }} {{ formatBytes(row.memBytes) }}
              </span>
              <button class="btn btn-sm" @click="toggleProcess(row.pid)">
                {{ expandedPids.has(row.pid) ? $t('collapse') : $t('view') }}
              </button>
            </span>
          </div>
          <div
            v-if="expandedPids.has(row.pid)"
            class="border-t border-border p-3 grid grid-cols-1 lg:grid-cols-2 gap-4"
          >
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('cpuUsage') }}</div>
              <TrendChart
                metric="cpu"
                :points="procHistory[String(row.pid)] ?? []"
                color-var="--color-chart-1"
                :height="120"
              />
            </div>
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('memoryUsageOfTotal') }}</div>
              <TrendChart
                metric="memory"
                :points="procHistory[String(row.pid)] ?? []"
                color-var="--color-chart-2"
                :height="120"
                :max="100"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import { useRunningSoftware } from '@/composables/useRunningSoftware'
import type { ProcessSample } from '@/models/process'
import type { HistoryPoint } from '@/models/system'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import { Icon } from '@iconify/vue'
import { formatBytes } from '@/utils/format'

const systemStore = useSystemStore()
const settingsStore = useSettingsStore()
const { runningSoftware, runningApps } = useRunningSoftware()

/** 最近一次采样结果（pid -> sample），用于表格实时值回填 */
const latestSamples = ref<Map<number, ProcessSample>>(new Map())
const expandedPids = ref<Set<number>>(new Set())
const searchKeyword = ref('')

interface ProcRow {
  name: string
  type: 'software' | 'springboot'
  pid: number
  cpu: number
  memBytes: number
  memPct: number
}

const processRows = computed<ProcRow[]>(() => {
  const rows: ProcRow[] = []
  for (const s of runningSoftware.value) {
    if (s.pid == null) continue
    const smp = latestSamples.value.get(s.pid)
    const memPct = systemStore.memTotal > 0 ? ((smp?.mem_bytes ?? 0) / systemStore.memTotal) * 100 : 0
    rows.push({ name: s.name, type: 'software', pid: s.pid, cpu: smp?.cpu_usage ?? 0, memBytes: smp?.mem_bytes ?? 0, memPct })
  }
  for (const a of runningApps.value) {
    if (a.pid == null) continue
    const smp = latestSamples.value.get(a.pid)
    const memPct = systemStore.memTotal > 0 ? ((smp?.mem_bytes ?? 0) / systemStore.memTotal) * 100 : 0
    rows.push({ name: a.name, type: 'springboot', pid: a.pid, cpu: smp?.cpu_usage ?? 0, memBytes: smp?.mem_bytes ?? 0, memPct })
  }
  return rows
})

/** 搜索只过滤列表展示，不影响 KPI 计数 */
const filteredRows = computed(() => {
  const k = searchKeyword.value.trim().toLowerCase()
  if (!k) return processRows.value
  return processRows.value.filter(
    (r) => r.name.toLowerCase().includes(k) || String(r.pid).includes(k)
  )
})

const thresholds = computed(() => ({
  cpu: settingsStore.settings?.alert_process_cpu ?? 90,
  mem: settingsStore.settings?.alert_process_mem ?? 90,
}))

// 告警机制提示：阈值取自设置（后端每 30s 采样时读取）
const alertHintParams = computed(() => {
  const s = settingsStore.settings
  return {
    sc: s?.alert_system_cpu ?? 90,
    sm: s?.alert_system_mem ?? 90,
    pc: s?.alert_process_cpu ?? 90,
    pm: s?.alert_process_mem ?? 90,
  }
})

const alertingCpu = computed(() => processRows.value.filter((r) => r.cpu >= thresholds.value.cpu).length)
const alertingMem = computed(() => processRows.value.filter((r) => r.memPct >= thresholds.value.mem).length)

function toggleProcess(pid: number) {
  const next = new Set(expandedPids.value)
  next.has(pid) ? next.delete(pid) : next.add(pid)
  expandedPids.value = next
}

// 趋势曲线读后端持久化序列（30s 粒度），前端不做累积
const procHistory = ref<Record<string, HistoryPoint[]>>({})

async function loadMetricsHistory() {
  try {
    procHistory.value = await invoke<Record<string, HistoryPoint[]>>('process_metrics_history')
  } catch {
    // 首次尚无历史文件：保持空对象即可
  }
}

let procTimer: number | null = null
let metricsTimer: number | null = null

onMounted(() => {
  // 进程采样：与整机轮询同频（1s），只刷新实时数值（告警由后端常驻循环负责）
  procTimer = window.setInterval(async () => {
    const pids = processRows.value.map((r) => r.pid).filter((p) => p != null)
    if (pids.length === 0) return
    const samples = await systemStore.sampleProcesses(pids)
    for (const s of samples) {
      latestSamples.value.set(s.pid, s)
    }
  }, 1000)
  loadMetricsHistory()
  metricsTimer = window.setInterval(loadMetricsHistory, 30_000)
})

onUnmounted(() => {
  if (procTimer) clearInterval(procTimer)
  if (metricsTimer) clearInterval(metricsTimer)
})
</script>
