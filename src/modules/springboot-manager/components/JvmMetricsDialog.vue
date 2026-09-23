<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="panel">
        <div class="panel-hd">
          <h2>{{ $t('jvmMonitor') }} · {{ appName }}</h2>
          <button class="dialog-close" :title="$t('close')" @click="$emit('close')">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <div class="panel-body">
          <div v-if="errMsg" class="empty">{{ errMsg }}</div>
          <div v-else-if="!metrics" class="empty">{{ $t('loading') }}</div>

          <template v-else>
            <!-- 结论先行：不让用户自己判断 31.3% 算不算高 -->
            <div class="status">
              <span class="dot" :class="status.level" />
              <span class="status-text">{{ status.text }}</span>
              <span class="status-hint">{{ $t('jvmAutoRefresh') }}</span>
            </div>

            <section class="block">
              <div class="block-hd">
                <span>{{ $t('heapMemory') }}</span>
                <span class="block-value">
                  {{ formatSize(metrics.heap_used) }}
                  <span class="block-total">/ {{ formatSize(metrics.heap_max) }}</span>
                </span>
              </div>
              <div class="bar">
                <div class="bar-fill" :class="barColor" :style="{ width: heapPct + '%' }" />
              </div>
              <div class="block-ft">{{ $t('jvmHeapUsedPct', { pct: heapPct.toFixed(1) }) }}</div>
            </section>

            <section class="block">
              <div class="block-hd">
                <span>{{ $t('nonHeapMemory') }}</span>
                <span class="block-value">
                  {{ formatSize(metrics.non_heap_used) }}
                  <span class="block-total">/ {{ formatSize(metrics.non_heap_committed) }}</span>
                </span>
              </div>
              <div class="bar">
                <div class="bar-fill info" :style="{ width: nonHeapPct + '%' }" />
              </div>
              <div class="block-ft">
                {{ $t('jvmClasses', { loaded: formatNum(metrics.classes_loaded), unloaded: formatNum(metrics.classes_unloaded) }) }}
              </div>
            </section>

            <!-- 趋势：瞬时快照看不出堆是否在持续爬升，而持续爬升才是泄漏信号 -->
            <section class="trend">
              <div class="trend-hd">{{ $t('jvmHeapTrend', { n: (HISTORY * REFRESH_MS) / 1000 }) }}</div>
              <svg class="spark" viewBox="0 0 100 40" preserveAspectRatio="none" aria-hidden="true">
                <path v-if="sparkArea" :d="sparkArea" class="spark-area" :class="barColor" />
                <path v-if="sparkPath" :d="sparkPath" class="spark-line" :class="barColor" />
              </svg>
              <div class="trend-ft">{{ trendLabel }}</div>
            </section>

            <div class="grid">
              <div class="cell">
                <div class="cell-lb">{{ $t('threadCount') }}</div>
                <div class="cell-v">{{ formatNum(metrics.thread_count) }}</div>
                <div class="cell-ft">{{ $t('jvmThreadDaemon', { n: formatNum(metrics.thread_daemon) }) }}</div>
              </div>
              <div class="cell">
                <div class="cell-lb">{{ $t('jvmThreadPeak') }}</div>
                <div class="cell-v">{{ formatNum(metrics.thread_peak) }}</div>
                <div class="cell-ft">{{ $t('jvmThreadStarted', { n: formatNum(metrics.thread_started) }) }}</div>
              </div>
              <div class="cell">
                <div class="cell-lb">{{ $t('jvmYoungGc') }}</div>
                <div class="cell-v">{{ formatNum(metrics.gc_young_count) }}</div>
                <div class="cell-ft">
                  {{ $t('jvmGcAccum', { t: formatDuration(metrics.gc_young_time_ms) }) }}<template
                    v-if="metrics.gc_young_count > 0"
                  >
                    · {{ $t('jvmGcAvg', { t: formatDuration(metrics.gc_young_time_ms / metrics.gc_young_count) }) }}</template
                  >
                </div>
              </div>
              <div class="cell">
                <div class="cell-lb">{{ $t('jvmFullGc') }}</div>
                <div class="cell-v">{{ formatNum(metrics.gc_full_count) }}</div>
                <div class="cell-ft">{{ $t('jvmGcAccum', { t: formatDuration(metrics.gc_full_time_ms) }) }}</div>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import type { JvmInfo } from '@/models/springboot'
import { useSpringBootStore } from '../stores/springboot'

const props = defineProps<{ appId: string; appName: string }>()
defineEmits<{ close: [] }>()

const { t } = useI18n()
const store = useSpringBootStore()
const metrics = ref<JvmInfo | null>(null)
const errMsg = ref('')

/** 轮询间隔。趋势窗口时长由它与 HISTORY 共同决定，改一个要同步检查另一个。 */
const REFRESH_MS = 5000
/** 趋势保留的采样点数：24 × 5s = 2 分钟 */
const HISTORY = 24

/** 堆已用采样序列（只有本次弹窗打开期间的数据） */
const heapHistory = ref<number[]>([])

let timer: ReturnType<typeof setInterval> | null = null
let attempt = 0

/** 分母用 Xmx（heap_max）而非当前提交容量，才是「离上限还有多远」 */
const heapPct = computed(() => {
  const m = metrics.value
  if (!m || m.heap_max === 0) return 0
  return Math.min(100, (m.heap_used / m.heap_max) * 100)
})

const nonHeapPct = computed(() => {
  const m = metrics.value
  if (!m || m.non_heap_committed === 0) return 0
  return Math.min(100, (m.non_heap_used / m.non_heap_committed) * 100)
})

const barColor = computed(() => (heapPct.value > 85 ? 'danger' : heapPct.value > 65 ? 'warn' : 'ok'))

const status = computed<{ level: 'ok' | 'warn' | 'danger'; text: string }>(() => {
  const m = metrics.value
  if (!m) return { level: 'ok', text: '' }
  if (heapPct.value >= 90) return { level: 'danger', text: t('jvmStatusHeapCritical') }
  if (heapPct.value >= 75) return { level: 'warn', text: t('jvmStatusHeapHigh') }
  // Full GC 会带来可感知的停顿，发生过就值得看一眼（但不算危险）
  if (m.gc_full_count > 0) {
    return { level: 'warn', text: t('jvmStatusFullGc', { n: formatNum(m.gc_full_count) }) }
  }
  return { level: 'ok', text: t('jvmStatusOk') }
})

/**
 * 趋势纵轴范围：以窗口数据为中心，跨度取「实际极差」与「最小跨度」中的较大者。
 *
 * 只按极值归一化会把平稳状态画成陡峭斜线——实测 323 → 325 MB（波动 2 MB，
 * 占 Xmx 0.1%）被铺满整个绘图区，看起来像内存持续增长。故给跨度设下限：
 * 波动小于下限时呈现为一条平线，这才是真实观感；真出现持续爬升时极差会
 * 超过下限，纵轴自动切换回真实跨度。
 */
const MIN_SPAN_RATIO = 0.05
const MIN_SPAN_BYTES = 32 * 1024 * 1024
const SPARK_H = 40
const SPARK_PAD = 4

const trendAxis = computed(() => {
  const h = heapHistory.value
  if (h.length < 2) return null
  const lo = Math.min(...h)
  const hi = Math.max(...h)
  const center = (lo + hi) / 2
  const floor = (metrics.value?.heap_max ?? 0) * MIN_SPAN_RATIO
  const span = Math.max(hi - lo, floor, MIN_SPAN_BYTES)
  return { lo, hi, min: center - span / 2, span }
})

/** 折线路径（纵轴按 trendAxis） */
const sparkPath = computed(() => {
  const axis = trendAxis.value
  if (!axis) return ''
  const drawH = SPARK_H - SPARK_PAD * 2
  const h = heapHistory.value
  return h
    .map((v, i) => {
      const x = (i / (h.length - 1)) * 100
      const y = SPARK_PAD + (1 - (v - axis.min) / axis.span) * drawH
      return `${i === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(' ')
})

/** 折线下方的淡填充：单线在空白区里显得孤立，填充给曲线一个体量基准 */
const sparkArea = computed(() => {
  const p = sparkPath.value
  if (!p) return ''
  return `${p} L100,${SPARK_H} L0,${SPARK_H} Z`
})

/** 标注真实区间而非纵轴范围——纵轴有最小跨度，不标实际波动幅度会看不出变化大小 */
const trendLabel = computed(() => {
  const axis = trendAxis.value
  if (!axis) return ''
  return `${formatSize(axis.lo)} – ${formatSize(axis.hi)}`
})

function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`
  return `${bytes} B`
}

function formatNum(n: number): string {
  return Math.round(n).toLocaleString()
}

function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return '0ms'
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${Math.round(ms)}ms`
}

async function refresh() {
  try {
    const m = await store.fetchJvmMetrics(props.appId)
    if (!m) {
      // 后端返回 null 仅表示 pid 不存在（应用未运行/已停止）；
      // 采集失败会走 catch 分支并展示后端给出的真实原因
      attempt++
      if (attempt > 3) errMsg.value = t('jvmAppNotRunning')
      return
    }
    metrics.value = m
    errMsg.value = ''
    attempt = 0
    heapHistory.value = [...heapHistory.value, m.heap_used].slice(-HISTORY)
  } catch (e: any) {
    errMsg.value = typeof e === 'string' ? e : t('jvmMonitorFailed')
  }
}

onMounted(async () => {
  await refresh()
  timer = setInterval(refresh, REFRESH_MS)
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
.overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5);
}
.panel {
  width: 460px; max-width: 92vw; max-height: 88vh;
  display: flex; flex-direction: column;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: var(--shadow-popover);
  overflow: hidden;
}
.panel-hd {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 14px 16px 12px 20px; flex: none;
}
.panel-hd h2 {
  font-size: 15px; font-weight: 600; margin: 0; min-width: 0;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.panel-body { padding: 4px 20px 20px; overflow-y: auto; flex: 1 1 auto; min-height: 0; }
.empty { text-align: center; padding: 32px 0; font-size: 13px; color: var(--color-muted-foreground); }

.status { display: flex; align-items: center; gap: 6px; margin-bottom: 16px; }
.dot { width: 7px; height: 7px; border-radius: 50%; flex: none; }
.dot.ok { background: var(--color-success); }
.dot.warn { background: var(--color-warning); }
.dot.danger { background: var(--color-destructive); }
.status-text { font-size: 12px; color: var(--color-muted-foreground); }
.status-hint {
  margin-left: auto; font-size: 11px;
  color: var(--color-muted-foreground); opacity: 0.7;
}

.block { margin-bottom: 14px; }
.block-hd {
  display: flex; align-items: baseline; justify-content: space-between;
  gap: 8px; margin-bottom: 6px; font-size: 13px;
}
.block-value { font-weight: 600; font-variant-numeric: tabular-nums; }
.block-total { font-weight: 400; color: var(--color-muted-foreground); }
.bar { height: 9px; border-radius: 999px; background: var(--color-muted); overflow: hidden; }
.bar-fill { height: 100%; border-radius: 999px; transition: width 0.3s ease; }
.bar-fill.ok { background: var(--color-success); }
.bar-fill.warn { background: var(--color-warning); }
.bar-fill.danger { background: var(--color-destructive); }
.bar-fill.info { background: var(--color-info); }
.block-ft {
  margin-top: 4px; text-align: right; font-size: 11px;
  color: var(--color-muted-foreground);
}

.trend { margin: 16px 0; }
.trend-hd { font-size: 11px; color: var(--color-muted-foreground); margin-bottom: 6px; }
.spark { display: block; width: 100%; height: 40px; }
/* preserveAspectRatio="none" 会拉伸描边，用 non-scaling-stroke 保持线宽 */
.spark-line {
  fill: none; stroke-width: 1.5; stroke-linejoin: round; stroke-linecap: round;
  vector-effect: non-scaling-stroke;
}
.spark-line.ok { stroke: var(--color-success); }
.spark-line.warn { stroke: var(--color-warning); }
.spark-line.danger { stroke: var(--color-destructive); }
/* 填充跟随折线配色，10% 透明度——只做「有体量」的视觉锚点，不喧宾夺主 */
.spark-area { stroke: none; }
.spark-area.ok { fill: var(--color-success); fill-opacity: 0.1; }
.spark-area.warn { fill: var(--color-warning); fill-opacity: 0.1; }
.spark-area.danger { fill: var(--color-destructive); fill-opacity: 0.1; }
.trend-ft {
  margin-top: 4px; text-align: right; font-size: 11px;
  color: var(--color-muted-foreground);
}

.grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
.cell {
  padding: 10px 12px; border-radius: 8px;
  background: var(--color-muted); border: 1px solid var(--color-border);
}
.cell-lb { font-size: 11px; color: var(--color-muted-foreground); }
.cell-v { font-size: 19px; font-weight: 600; font-variant-numeric: tabular-nums; margin: 2px 0 1px; }
.cell-ft { font-size: 11px; color: var(--color-muted-foreground); }
</style>
