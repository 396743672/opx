<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="panel">
        <div class="panel-hd">
          <h2>{{ $t('jvmMonitor') }} - {{ appName }}</h2>
        </div>
        <div class="panel-body">
          <div v-if="errMsg" class="text-center py-8" style="color:#ef4444">
            {{ errMsg }}
          </div>
          <div v-else-if="!metrics || !metrics.heap_max" class="text-center py-8" style="color:var(--color-muted-foreground)">
            {{ $t('loading') }}
          </div>
          <div v-else>
            <div class="mb-4">
              <div class="flex justify-between text-sm mb-1">
                <span>{{ $t('heapMemory') }}</span>
                <span>{{ formatSize(metrics.heap_used) }} / {{ formatSize(metrics.heap_max) }}</span>
              </div>
              <div class="bar-bg">
                <div class="bar-fill" :class="barColor" :style="{ width: heapPct + '%' }" />
              </div>
              <div class="text-xs text-right mt-0.5" style="color:var(--color-muted-foreground)">{{ heapPct.toFixed(1) }}%</div>
            </div>
            <div class="flex justify-between text-sm mb-4">
              <span>{{ $t('nonHeapMemory') }}</span>
              <span class="font-mono">{{ formatSize(metrics.non_heap_used) }}</span>
            </div>
            <div class="flex justify-between text-sm mb-2">
              <span>{{ $t('threadCount') }}</span>
              <span class="font-mono">{{ metrics.thread_count }}</span>
            </div>
            <div class="grid grid-cols-2 gap-4 text-sm">
              <div>
                <span style="color:var(--color-muted-foreground)">{{ $t('gcCount') }}</span>
                <div class="font-mono">{{ metrics.gc_count }}</div>
              </div>
              <div>
                <span style="color:var(--color-muted-foreground)">{{ $t('gcTime') }}</span>
                <div class="font-mono">{{ (metrics.gc_time / 1000).toFixed(2) }}s</div>
              </div>
            </div>
          </div>
        </div>
        <div class="panel-ft">
          <button class="btn" @click="$emit('close')">{{ $t('close') }}</button>
          <button class="btn" @click="refresh">{{ $t('refresh') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { JvmInfo } from '@/models/springboot'
import { useSpringBootStore } from '../stores/springboot'

const props = defineProps<{ appId: string; appName: string }>()
defineEmits<{ close: [] }>()

const { t } = useI18n()
const store = useSpringBootStore()
const metrics = ref<JvmInfo | null>(null)
const errMsg = ref('')
let timer: ReturnType<typeof setInterval> | null = null
let attempt = 0

const heapPct = computed(() => {
  if (!metrics.value || metrics.value.heap_max === 0) return 0
  return (metrics.value.heap_used / metrics.value.heap_max) * 100
})

const barColor = computed(() => heapPct.value > 80 ? 'danger' : heapPct.value > 60 ? 'warn' : 'ok')

function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB'
  if (bytes >= 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(0) + ' MB'
  if (bytes >= 1024) return (bytes / 1024).toFixed(0) + ' KB'
  return bytes + ' B'
}

async function refresh() {
  try {
    const m = await store.fetchJvmMetrics(props.appId)
    if (m) { metrics.value = m; errMsg.value = ''; attempt = 0 }
    else { attempt++; if (attempt > 3) errMsg.value = t('jvmMonitorNeedsJdk') }
  } catch (e: any) { errMsg.value = typeof e === 'string' ? e : t('jvmMonitorFailed') }
}

onMounted(async () => { await refresh(); timer = setInterval(refresh, 5000) })
onBeforeUnmount(() => { if (timer) clearInterval(timer) })
</script>

<style scoped>
.overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5);
}
.panel {
  width: 400px; max-height: 80vh;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: var(--shadow-popover);
  overflow: hidden;
}
.panel-hd { padding: 16px 20px 0; }
.panel-hd h2 { font-size: 15px; font-weight: 600; margin: 0; }
.panel-body { padding: 16px 20px; }
.panel-ft {
  display: flex; justify-content: flex-end; gap: 8px;
  padding: 14px 20px; border-top: 1px solid var(--color-border);
}
.bar-bg { height: 8px; border-radius: 999px; background: var(--color-muted); overflow: hidden; }
.bar-fill { height: 100%; border-radius: 999px; transition: width 0.3s; }
.bar-fill.ok { background: #22c55e; }
.bar-fill.warn { background: #f59e0b; }
.bar-fill.danger { background: #ef4444; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px;
  border-radius: 6px; cursor: pointer; font-size: 13px;
  border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground);
}
.btn:hover { background: var(--color-muted); }
</style>
