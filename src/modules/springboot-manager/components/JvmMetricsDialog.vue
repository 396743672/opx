<template>
  <div class="dialog-overlay" @click.self="$emit('close')">
    <div class="dialog-panel max-w-lg">
      <div class="dialog-header">
        <h2>{{ $t('jvmMonitor') }} - {{ appName }}</h2>
      </div>
      <div class="dialog-body">
        <div v-if="!metrics" class="text-center text-muted-foreground py-8">
          {{ $t('loading') }}
        </div>
        <div v-else class="space-y-6">
          <div>
            <div class="flex justify-between text-sm mb-1">
              <span>{{ $t('heapMemory') }}</span>
              <span>{{ formatSize(metrics.heap_used) }} / {{ formatSize(metrics.heap_max) }}</span>
            </div>
            <div class="h-2 bg-muted rounded-full overflow-hidden">
              <div
                class="h-full rounded-full transition-all"
                :class="heapPct > 80 ? 'bg-red-500' : heapPct > 60 ? 'bg-amber-500' : 'bg-green-500'"
                :style="{ width: heapPct + '%' }"
              />
            </div>
            <div class="text-xs text-right text-muted-foreground mt-0.5">{{ heapPct.toFixed(1) }}%</div>
          </div>

          <div class="flex justify-between text-sm">
            <span>{{ $t('threadCount') }}</span>
            <span class="font-mono">{{ metrics.thread_count }}</span>
          </div>

          <div class="grid grid-cols-2 gap-4 text-sm">
            <div>
              <span class="text-muted-foreground">{{ $t('gcCount') }}</span>
              <div class="font-mono">{{ metrics.gc_count }}</div>
            </div>
            <div>
              <span class="text-muted-foreground">{{ $t('gcTime') }}</span>
              <div class="font-mono">{{ (metrics.gc_time / 1000).toFixed(2) }}s</div>
            </div>
          </div>
        </div>
      </div>
      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('close') }}</button>
        <button class="btn" @click="refresh">{{ $t('refresh') }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import type { JvmInfo } from '@/models/springboot'
import { useSpringBootStore } from '../stores/springboot'

const props = defineProps<{
  appId: string
  appName: string
}>()

defineEmits<{ close: [] }>()

const store = useSpringBootStore()
const metrics = ref<JvmInfo | null>(null)
let timer: ReturnType<typeof setInterval> | null = null

const heapPct = computed(() => {
  if (!metrics.value || metrics.value.heap_max === 0) return 0
  return (metrics.value.heap_used / metrics.value.heap_max) * 100
})

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB'
  if (bytes >= 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(0) + ' MB'
  if (bytes >= 1024) return (bytes / 1024).toFixed(0) + ' KB'
  return bytes + ' B'
}

async function refresh() {
  metrics.value = await store.fetchJvmMetrics(props.appId)
}

onMounted(async () => {
  await refresh()
  timer = setInterval(refresh, 10000)
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>
