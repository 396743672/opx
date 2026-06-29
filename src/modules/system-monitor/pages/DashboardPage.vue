<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:gauge"
      :title="$t('systemMonitor')"
      :subtitle="$t('dashboardSubtitle')"
    >
      <template #actions>
        <span class="text-xs text-muted-foreground tnum">
          {{ systemInfo?.hostname }}
        </span>
      </template>
    </PageHeader>

    <!-- 概览 StatCard 行 -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-4">
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
        :sub="`${formatBytes(systemInfo?.memory_used || 0)} / ${formatBytes(systemInfo?.memory_total || 0)}`"
        icon="mdi:memory"
        accent="chart-2"
        :progress="systemStore.memoryUsage"
      />
      <StatCard
        :label="$t('diskUsage')"
        :value="systemStore.diskUsage.toFixed(0)"
        unit="%"
        :sub="`${systemInfo?.disks.length || 0} ${$t('volumes')}`"
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
            <span class="tnum">{{ formatBytes(systemInfo?.network.bytes_sent || 0) }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="flex items-center gap-2 text-muted-foreground">
              <Icon icon="mdi:arrow-down-bold" class="text-info" />{{ $t('networkDown') }}
            </span>
            <span class="tnum">{{ formatBytes(systemInfo?.network.bytes_recv || 0) }}</span>
          </div>
          <div class="border-t border-border pt-3 space-y-2 text-xs text-muted-foreground">
            <div class="flex justify-between">
              <span>{{ $t('packetsSent') }}</span>
              <span class="tnum">{{ formatNumber(systemInfo?.network.packets_sent || 0) }}</span>
            </div>
            <div class="flex justify-between">
              <span>{{ $t('packetsRecv') }}</span>
              <span class="tnum">{{ formatNumber(systemInfo?.network.packets_recv || 0) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 进程列表 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card">
      <CardHeader
        :title="$t('processList')"
        :subtitle="`${systemStore.processes.length} ${$t('totalProcesses')}`"
        hide-refresh
      />
      <div class="h-80">
        <ProcessTable :processes="systemStore.topProcesses" @kill="confirmKill" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSystemStore } from '@/stores/system'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import ProcessTable from '@/components/ProcessTable.vue'
import ProgressBar from '@/components/ProgressBar.vue'
import { Icon } from '@iconify/vue'
import { formatBytes, formatRate, formatUptime, formatBootTime } from '@/utils/format'

const { t } = useI18n()
const systemStore = useSystemStore()

const systemInfo = computed(() => systemStore.systemInfo)

const bootTimeStr = computed(() =>
  systemInfo.value ? formatBootTime(systemInfo.value.boot_time) : '-'
)

/* 运行时长 */
const nowTick = ref(Date.now())
let tickTimer: number | null = null
const uptime = computed(() => {
  const boot = systemInfo.value?.boot_time
  if (!boot) return '-'
  return formatUptime(Math.floor(nowTick.value / 1000) - boot)
})

function formatNumber(n: number): string {
  return n.toLocaleString()
}

async function confirmKill(pid: number) {
  if (window.confirm(t('confirmKillProcess'))) {
    await systemStore.killProcess(pid)
  }
}

onMounted(() => {
  tickTimer = window.setInterval(() => {
    nowTick.value = Date.now()
  }, 1000)
})

onUnmounted(() => {
  if (tickTimer) clearInterval(tickTimer)
})
</script>
