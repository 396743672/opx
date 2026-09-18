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
        <TrendChart metric="cpu" :points="sysHistory" color-var="--color-chart-1" />
      </div>
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <CardHeader :title="$t('memoryUsage')" :subtitle="$t('trendHint')" hide-refresh />
        <TrendChart metric="memory" :points="sysHistory" color-var="--color-chart-2" />
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

    <!-- 运行中服务与应用：默认收起，避免首页被软件列表占满 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card">
      <button
        class="w-full flex items-center justify-between cursor-pointer text-left"
        :aria-expanded="showRunningList"
        @click="showRunningList = !showRunningList"
      >
        <span class="flex items-center gap-2 text-sm font-semibold tracking-tight">
          <Icon icon="mdi:server-network" class="text-muted-foreground" />
          {{ $t('runningServicesAndApps') }} ({{ runningTotal }})
        </span>
        <Icon
          :icon="showRunningList ? 'mdi:chevron-up' : 'mdi:chevron-down'"
          class="text-muted-foreground"
        />
      </button>
      <div v-if="showRunningList" class="mt-4">
        <div
          v-if="runningTotal === 0"
          class="py-3 text-sm text-muted-foreground"
        >
          {{ $t('noRunningServicesOrApps') }}
        </div>
        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <div class="text-xs text-muted-foreground mb-2">{{ $t('runningServices') }}</div>
            <div class="space-y-1">
              <div
                v-for="s in runningSoftware"
                :key="s.id"
                class="flex items-center justify-between text-sm border border-border rounded-md px-3 py-2"
              >
                <span class="flex items-center gap-2 min-w-0">
                  <span class="w-1.5 h-1.5 rounded-full bg-success shrink-0"></span>
                  <span class="truncate">{{ s.name }}</span>
                </span>
                <span class="text-xs text-muted-foreground shrink-0">{{ s.version }}</span>
              </div>
            </div>
          </div>
          <div>
            <div class="text-xs text-muted-foreground mb-2">{{ $t('runningApps') }}</div>
            <div class="space-y-1">
              <div
                v-for="app in runningApps"
                :key="app.id"
                class="flex items-center justify-between text-sm border border-border rounded-md px-3 py-2"
              >
                <span class="flex items-center gap-2 min-w-0">
                  <span class="w-1.5 h-1.5 rounded-full bg-info shrink-0"></span>
                  <span class="truncate">{{ app.name }}</span>
                </span>
                <span class="text-xs text-muted-foreground tnum shrink-0">:{{ app.port }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSystemStore } from '@/stores/system'
import { useRunningSoftware } from '@/composables/useRunningSoftware'
import type { HistoryPoint } from '@/models/system'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import ProgressBar from '@/components/ProgressBar.vue'
import { Icon } from '@iconify/vue'
import { formatBytes, formatRate, formatUptime, formatBootTime } from '@/utils/format'

const systemStore = useSystemStore()
const { runningSoftware, runningApps } = useRunningSoftware()

const showRunningList = ref(false)
const runningTotal = computed(() => runningSoftware.value.length + runningApps.value.length)

// 趋势曲线读后端持久化序列（30s 粒度），前端不再累积历史、不做告警判定
const sysHistory = ref<HistoryPoint[]>([])

async function loadMetricsHistory() {
  try {
    sysHistory.value = await invoke<HistoryPoint[]>('system_history')
  } catch {
    // 首次尚无历史文件：保持空数组即可
  }
}

const systemInfo = computed(() => systemStore.systemInfo)

const bootTimeStr = computed(() =>
  systemInfo.value ? formatBootTime(systemInfo.value.boot_time) : '-'
)

/* 运行时长 */
const nowTick = ref(Date.now())
let tickTimer: number | null = null
let metricsTimer: number | null = null
const uptime = computed(() => {
  const boot = systemInfo.value?.boot_time
  if (!boot) return '-'
  return formatUptime(Math.floor(nowTick.value / 1000) - boot)
})

onMounted(() => {
  tickTimer = window.setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)
  // 趋势曲线：读后端持久化序列，与后端采样间隔一致（30s）
  loadMetricsHistory()
  metricsTimer = window.setInterval(loadMetricsHistory, 30_000)
})

onUnmounted(() => {
  if (tickTimer) clearInterval(tickTimer)
  if (metricsTimer) clearInterval(metricsTimer)
})
</script>
