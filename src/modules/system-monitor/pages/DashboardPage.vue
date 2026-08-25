<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:gauge"
      :title="$t('systemMonitor')"
      :subtitle="$t('dashboardSubtitle')"
    >
      <template #actions>
        <span class="text-xs text-muted-foreground tnum">
          {{ systemInfo?.hostname || '—' }}
        </span>
      </template>
    </PageHeader>

    <!-- 数据未就绪时显示骨架屏 -->
    <div v-if="!systemInfo" class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-4">
      <div
        v-for="i in 4"
        :key="i"
        class="rounded-lg border border-border bg-card p-4 shadow-card h-[110px] animate-pulse"
      >
        <div class="h-3 w-20 bg-muted rounded mb-3"></div>
        <div class="h-7 w-24 bg-muted rounded"></div>
      </div>
    </div>

    <!-- 概览 StatCard 行 -->
    <div v-else class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-4">
      <StatCard
        :label="$t('cpuUsage')"
        :value="systemStore.cpuUsage.toFixed(0)"
        unit="%"
        icon="mdi:cpu-64-bit"
        accent="primary"
        :progress="systemStore.cpuUsage"
      />
      <StatCard
        :label="$t('memoryUsage')"
        :value="systemStore.memoryUsage.toFixed(0)"
        unit="%"
        :sub="`${formatBytes(systemInfo.memory_used)} / ${formatBytes(systemInfo.memory_total)}`"
        icon="mdi:memory"
        accent="chart-2"
        :progress="systemStore.memoryUsage"
      />
      <StatCard
        :label="$t('diskUsage')"
        :value="systemStore.diskUsage.toFixed(0)"
        unit="%"
        :sub="`${systemInfo.disks.length} ${$t('volumes')}`"
        icon="mdi:harddisk"
        accent="chart-3"
        :progress="systemStore.diskUsage"
      />
      <StatCard
        :label="$t('networkTraffic')"
        :value="formatRate(systemStore.netRecvRate)"
        :sub="`${$t('networkUp')}: ${formatRate(systemStore.netSentRate)}`"
        icon="mdi:lan"
        accent="chart-4"
      />
    </div>

    <!-- 趋势图 -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4 mb-4">
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('cpuUsage')" :subtitle="$t('trendHint')" hide-refresh />
        <TrendChart metric="cpu" :points="systemStore.history" color-var="--color-chart-1" />
      </div>
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('memoryUsage')" :subtitle="$t('trendHint')" hide-refresh />
        <TrendChart metric="memory" :points="systemStore.history" color-var="--color-chart-2" />
      </div>
    </div>

    <!-- 服务组概览 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader icon="mdi:layers-outline" :title="$t('stacks')" />
      <div v-if="stackOverview.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noStacks') }}
      </div>
      <div v-else class="grid grid-cols-2 md:grid-cols-3 gap-3">
        <div
          v-for="sg in stackOverview"
          :key="sg.id"
          class="rounded-md border border-border p-3 flex items-center justify-between"
        >
          <div>
            <div class="text-sm font-medium">{{ sg.name }}</div>
            <div class="text-xs text-muted-foreground tnum">
              {{ sg.running }} / {{ sg.total }} {{ $t('stackMembers') }}
            </div>
          </div>
          <Icon
            :icon="sg.failed ? 'mdi:alert-circle' : sg.running === sg.total && sg.total > 0 ? 'mdi:check-circle' : 'mdi:circle-outline'"
            class="shrink-0"
            :class="sg.failed ? 'text-destructive' : sg.running === sg.total && sg.total > 0 ? 'text-success' : 'text-muted-foreground'"
          />
        </div>
      </div>
    </div>

    <!-- 进程资源监控 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader icon="mdi:chart-timeline-variant" :title="$t('processMonitor')" />
      <div v-if="processRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noRunningProcess') }}
      </div>
      <div v-else class="space-y-1">
        <div v-for="row in processRows" :key="row.pid" class="border border-border rounded-md">
          <!-- 表格行 -->
          <div class="flex items-center justify-between px-3 py-2 text-sm">
            <span class="flex items-center gap-2 min-w-0">
              <Icon :icon="row.type === 'springboot' ? 'mdi:leaf' : 'mdi:server'" :class="row.type === 'springboot' ? 'text-green-500' : 'text-info'" />
              <span class="truncate">{{ row.name }}</span>
              <span class="text-xs text-muted-foreground tnum">({{ row.pid }})</span>
            </span>
            <span class="flex items-center gap-4 shrink-0">
              <span class="text-xs tnum" :class="row.cpu >= 90 ? 'text-destructive' : ''">
                CPU {{ row.cpu.toFixed(1) }}%
              </span>
              <span class="text-xs tnum" :class="row.memPct >= 90 ? 'text-destructive' : ''">
                内存 {{ formatBytes(row.memBytes) }}
              </span>
              <button class="btn btn-sm" @click="toggleProcess(row.pid)">
                {{ expandedPids.has(row.pid) ? $t('collapse') : $t('view') }}
              </button>
            </span>
          </div>
          <!-- 行内展开趋势图 -->
          <div v-if="expandedPids.has(row.pid)" class="border-t border-border p-3 grid grid-cols-1 lg:grid-cols-2 gap-4">
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('cpuUsage') }}</div>
              <TrendChart metric="cpu" :points="processPoints(row.pid)" color-var="--color-chart-1" :height="120" />
            </div>
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('memoryUsage') }}（占整机 %）</div>
              <TrendChart metric="memory" :points="processPoints(row.pid)" color-var="--color-chart-2" :height="120" :max="100" />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ⚡ 服务与应用概览（示例） -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4 mb-4">
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('installedSoftware')" hide-refresh />
        <div class="space-y-1 max-h-72 overflow-y-auto pr-1">
          <div v-for="s in installedSoftware" :key="s.id" class="flex items-center justify-between text-sm">
            <span class="flex items-center gap-2">
              <span class="w-1.5 h-1.5 rounded-full" :class="s.status === SoftwareStatus.Running ? 'bg-success' : 'bg-muted-foreground/40'"></span>
              {{ s.name }}
            </span>
            <span class="text-xs text-muted-foreground">{{ s.version }}</span>
          </div>
        </div>
      </div>

      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('runningServices')" hide-refresh />
        <div class="space-y-1 max-h-72 overflow-y-auto pr-1">
          <div v-for="s in runningSoftware" :key="s.id" class="flex items-center justify-between text-sm">
            <span class="flex items-center gap-2">
              <span class="w-1.5 h-1.5 rounded-full bg-success"></span>
              {{ s.name }}
            </span>
            <span class="text-xs text-success">{{ $t('running') }}</span>
          </div>
        </div>
      </div>

      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('runningApps')" hide-refresh />
        <div class="space-y-1 max-h-72 overflow-y-auto pr-1">
          <div v-for="app in runningApps" :key="app.id" class="flex items-center justify-between text-sm">
            <span class="flex items-center gap-2">
              <span class="w-1.5 h-1.5 rounded-full bg-info"></span>
              {{ app.name }}
            </span>
            <span class="text-xs text-muted-foreground tnum">:{{ app.port }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 信息行：系统信息 / 磁盘 / 网络 -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4 mb-4">
      <!-- 系统信息 -->
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('systemInfo')" hide-refresh />
        <dl class="space-y-2.5 text-sm">
          <div class="flex justify-between gap-2">
            <dt class="text-muted-foreground">{{ $t('os') }}</dt>
            <dd class="text-right truncate">{{ systemInfo?.os_name }} {{ systemInfo?.os_version }}</dd>
          </div>
          <div class="flex justify-between gap-2">
            <dt class="text-muted-foreground">{{ $t('hostname') }}</dt>
            <dd class="text-right truncate">{{ systemInfo?.hostname }}</dd>
          </div>
          <div class="flex justify-between gap-2">
            <dt class="text-muted-foreground">{{ $t('bootTime') }}</dt>
            <dd class="text-right truncate">{{ bootTimeStr }}</dd>
          </div>
          <div class="flex justify-between gap-2">
            <dt class="text-muted-foreground">{{ $t('uptime') }}</dt>
            <dd class="text-right tnum">{{ uptime }}</dd>
          </div>
        </dl>
      </div>

      <!-- 磁盘 -->
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('diskUsage')" hide-refresh />
        <div class="space-y-3">
          <div v-for="disk in systemInfo?.disks" :key="disk.mount_point">
            <ProgressBar
              :label="disk.mount_point"
              :value="disk.usage"
            />
            <div class="mt-1 text-xs text-muted-foreground tnum text-right">
              {{ formatBytes(disk.used) }} / {{ formatBytes(disk.total) }}
            </div>
          </div>
          <div v-if="!systemInfo?.disks.length" class="text-xs text-muted-foreground py-4 text-center">
            {{ $t('noData') }}
          </div>
        </div>
      </div>

      <!-- 网络 -->
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('networkTraffic')" hide-refresh />
        <div class="space-y-3 text-sm">
          <div class="flex items-center justify-between">
            <span class="flex items-center gap-2 text-muted-foreground">
              <Icon icon="mdi:arrow-up-bold" class="text-success" />{{ $t('networkUp') }}
            </span>
            <span class="tnum">{{ formatRate(systemStore.netSentRate) }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="flex items-center gap-2 text-muted-foreground">
              <Icon icon="mdi:arrow-down-bold" class="text-info" />{{ $t('networkDown') }}
            </span>
            <span class="tnum">{{ formatRate(systemStore.netRecvRate) }}</span>
          </div>
          <div class="border-t border-border pt-3 space-y-2 text-xs text-muted-foreground">
            <div class="flex justify-between">
              <span>{{ $t('totalSent') }}</span>
              <span class="tnum">{{ formatBytes(systemInfo?.network.bytes_sent || 0) }}</span>
            </div>
            <div class="flex justify-between">
              <span>{{ $t('totalRecv') }}</span>
              <span class="tnum">{{ formatBytes(systemInfo?.network.bytes_recv || 0) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { useSystemStore } from '@/stores/system'
import { useSpringBootStore } from '@/modules/springboot-manager/stores/springboot'
import { useLifecycleStore } from '@/modules/software-manager/stores/lifecycle'
import { useStackStore } from '@/stores/stack'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { InstalledSoftware } from '@/models/software'
import { SoftwareStatus } from '@/models/software'
import type { SpringBootApp } from '@/models/springboot'
import { AppStatus } from '@/models/springboot'
import type { ProcessSample } from '@/models/process'
import type { HistoryPoint } from '@/models/system'
import { toast } from '@/composables/useToast'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import ProgressBar from '@/components/ProgressBar.vue'
import { Icon } from '@iconify/vue'
import { formatBytes, formatRate, formatUptime, formatBootTime } from '@/utils/format'

const { t } = useI18n()
const systemStore = useSystemStore()
const sbStore = useSpringBootStore()
const lifecycleStore = useLifecycleStore()
const stackStore = useStackStore()

const installedSoftware = ref<InstalledSoftware[]>([])
const runningApps = ref<SpringBootApp[]>([])
let unlistenSb: UnlistenFn | null = null

const runningSoftware = computed(() =>
  installedSoftware.value.filter(s => {
    const st = lifecycleStore.getStatus(s.id)
    const status = st !== SoftwareStatus.Unknown ? st : s.status
    return status === SoftwareStatus.Running
  })
)

// ===== 进程资源监控 =====
const expandedPids = ref<Set<number>>(new Set())
/** 告警去重：pid:metric 已告警标记 */
const alerted = ref<Set<string>>(new Set())
/** 最近一次采样结果（pid -> sample），用于表格实时值回填 */
const latestSamples = ref<Map<number, ProcessSample>>(new Map())

interface ProcRow { name: string; type: 'software' | 'springboot'; pid: number; cpu: number; memBytes: number; memPct: number }

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

function processPoints(pid: number): HistoryPoint[] {
  return systemStore.processSamples[pid] ?? []
}

function toggleProcess(pid: number) {
  const next = new Set(expandedPids.value)
  next.has(pid) ? next.delete(pid) : next.add(pid)
  expandedPids.value = next
}

const THRESHOLD_CPU = 90
const THRESHOLD_MEM = 90
// 冷启动首个采样 CPU% 因 sysinfo 增量算法可能虚高，故跳过首个 tick 的告警判定
const firstTick = ref(true)
function checkAlerts() {
  if (firstTick.value) {
    firstTick.value = false
    return
  }
  for (const row of processRows.value) {
    if (row.cpu >= THRESHOLD_CPU) {
      const key = `${row.pid}:cpu`
      if (!alerted.value.has(key)) {
        alerted.value.add(key)
        toast(t('processAlertCpu', { name: row.name, value: row.cpu.toFixed(0) }), 'err')
      }
    } else {
      alerted.value.delete(`${row.pid}:cpu`)
    }
    if (row.memPct >= THRESHOLD_MEM) {
      const key = `${row.pid}:mem`
      if (!alerted.value.has(key)) {
        alerted.value.add(key)
        toast(t('processAlertMem', { name: row.name, value: row.memPct.toFixed(0) }), 'err')
      }
    } else {
      alerted.value.delete(`${row.pid}:mem`)
    }
  }
}

const systemInfo = computed(() => systemStore.systemInfo)

// 服务组概览：各服务组 running/总数 聚合、failed 标红
const stackOverview = computed(() =>
  stackStore.stacks.map((s) => {
    const rt = stackStore.getRuntime(s.id)
    const running = rt.filter((m) => m.status === 'running').length
    const failed = rt.some((m) => m.status === 'failed')
    return { id: s.id, name: s.name, total: s.items.length, running, failed }
  })
)

const bootTimeStr = computed(() =>
  systemInfo.value ? formatBootTime(systemInfo.value.boot_time) : '-'
)

/* 运行时长 */
const nowTick = ref(Date.now())
let tickTimer: number | null = null
let procTimer: number | null = null
const uptime = computed(() => {
  const boot = systemInfo.value?.boot_time
  if (!boot) return '-'
  return formatUptime(Math.floor(nowTick.value / 1000) - boot)
})

onMounted(async () => {
  installedSoftware.value = await invoke<InstalledSoftware[]>('list_installed_software')
  await sbStore.fetchApps()
  runningApps.value = sbStore.apps.filter(a => a.status === AppStatus.Running)
  tickTimer = window.setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)
  // 进程采样 + 告警：与整机轮询同频（1s）
  procTimer = window.setInterval(async () => {
    const pids = processRows.value.map((r) => r.pid).filter((p) => p != null)
    if (pids.length === 0) return
    const samples = await systemStore.sampleProcesses(pids)
    for (const s of samples) {
      latestSamples.value.set(s.pid, s)
    }
    checkAlerts()
  }, 1000)
  // ponytail: 监听启动/停止事件，运行列表实时刷新
  await lifecycleStore.initListener()
  await stackStore.loadStacks()
  await stackStore.subscribe()
  unlistenSb = await listen('springboot-status-changed', async () => {
    await sbStore.fetchApps()
    runningApps.value = sbStore.apps.filter(a => a.status === AppStatus.Running)
  })
})

onUnmounted(() => {
  if (tickTimer) clearInterval(tickTimer)
  if (procTimer) clearInterval(procTimer)
  lifecycleStore.destroyListener()
  stackStore.unsubscribe()
  unlistenSb?.()
})
</script>
