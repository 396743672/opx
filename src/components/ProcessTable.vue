<template>
  <div class="flex flex-col h-full">
    <!-- 搜索框 -->
    <div class="relative mb-3">
      <Icon
        icon="mdi:magnify"
        class="absolute left-2 top-1/2 -translate-y-1/2 text-base text-muted-foreground"
      />
      <input
        v-model="keyword"
        type="text"
        :placeholder="$t('searchProcess')"
        class="w-full h-8 pl-8 pr-3 text-sm rounded-md bg-muted border border-transparent focus:border-primary focus:bg-background outline-none transition-colors"
      />
    </div>

    <!-- 表头 -->
    <div
      class="grid gap-2 px-2 py-1.5 text-xs text-muted-foreground border-b border-border"
      style="grid-template-columns: 1fr 70px 70px 90px 110px"
    >
      <span>{{ $t('name') }}</span>
      <span class="text-right">PID</span>
      <span class="text-right">CPU%</span>
      <span class="text-right">{{ $t('memory') }}%</span>
      <span class="text-right">{{ $t('status') }}</span>
    </div>

    <!-- 列表 -->
    <div class="flex-1 overflow-auto min-h-0">
      <div
        v-for="p in filtered"
        :key="p.pid"
        class="group grid gap-2 px-2 py-1.5 text-sm items-center rounded-sm hover:bg-muted/60 transition-colors"
        style="grid-template-columns: 1fr 70px 70px 90px 110px"
      >
        <span class="truncate" :title="p.name">{{ p.name }}</span>
        <span class="text-right tnum text-muted-foreground">{{ p.pid }}</span>
        <span class="text-right tnum">{{ p.cpu_usage.toFixed(1) }}</span>
        <span class="text-right tnum">{{ p.memory_usage.toFixed(1) }}</span>
        <span class="flex items-center justify-end gap-1.5">
          <span
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium border"
            :class="statusClass(p.status)"
          >
            <span class="w-1.5 h-1.5 rounded-full" :class="statusDot(p.status)"></span>
            {{ statusText(p.status) }}
          </span>
          <button
            class="opacity-0 group-hover:opacity-100 p-1 rounded text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-all cursor-pointer"
            :aria-label="$t('killProcess')"
            :title="$t('killProcess')"
            @click.stop="$emit('kill', p.pid)"
          >
            <Icon icon="mdi:close" class="text-base" />
          </button>
        </span>
      </div>
      <div
        v-if="!filtered.length"
        class="py-8 text-center text-xs text-muted-foreground"
      >
        {{ $t('noData') }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import type { ProcessInfo } from '@/models/system'

const { t } = useI18n()

interface Props {
  processes: ProcessInfo[]
  limit?: number
}
const props = withDefaults(defineProps<Props>(), { limit: 50 })

defineEmits<{ kill: [pid: number] }>()

const keyword = ref('')

const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase()
  const list = kw
    ? props.processes.filter((p) => p.name.toLowerCase().includes(kw))
    : props.processes
  return list.slice(0, props.limit)
})

/**
 * sysinfo ProcessStatus::to_string() 跨平台输出：
 * Run / Sleep / Idle / UninterruptibleSleep / Waking / Parked / LockBlocked
 * Zombie / Dead / Unknown ...
 * 存活进程（含 Sleep/Idle）统一视为「运行中」，仅 Zombie/Dead/Unknown 视为异常。
 */
type StatusKind = 'running' | 'stopped' | 'error'

function statusKind(status: string): StatusKind {
  const s = status.toLowerCase()
  if (s === 'zombie' || s === 'dead') return 'error'
  if (s === 'unknown' || s === 'stop' || s === 'stopped') return 'stopped'
  return 'running'
}

function statusClass(status: string): string {
  switch (statusKind(status)) {
    case 'error':
      return 'bg-destructive/10 text-destructive border-destructive/20'
    case 'stopped':
      return 'bg-muted text-muted-foreground border-border'
    default:
      return 'bg-success/10 text-success border-success/20'
  }
}

function statusDot(status: string): string {
  switch (statusKind(status)) {
    case 'error':
      return 'bg-destructive'
    case 'stopped':
      return 'bg-muted-foreground'
    default:
      return 'bg-success'
  }
}

function statusText(status: string): string {
  switch (statusKind(status)) {
    case 'error':
      return t('error')
    case 'stopped':
      return t('stopped')
    default:
      return t('running')
  }
}
</script>
