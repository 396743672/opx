<template>
  <div class="relative w-full" :style="{ height: `${height}px` }">
    <canvas ref="canvasRef"></canvas>
    <div
      v-if="!points.length"
      class="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground"
    >
      {{ $t('noData') }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import Chart from 'chart.js/auto'
import type { ChartConfiguration } from 'chart.js'
import { useI18n } from 'vue-i18n'
import type { HistoryPoint } from '@/models/system'

const { t } = useI18n()

interface Props {
  points: HistoryPoint[]
  /** 'cpu' | 'memory' */
  metric: 'cpu' | 'memory'
  colorVar?: string
  height?: number
  max?: number
}
const props = withDefaults(defineProps<Props>(), {
  colorVar: '--color-chart-1',
  height: 160,
  max: 100,
})

const canvasRef = ref<HTMLCanvasElement | null>(null)
let chart: Chart | null = null

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim()
}

function prefersReduced(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/** 青蓝主色 fallback，避免 CSS 变量读取时机问题导致黑色 */
const COLOR_FALLBACK = '#3b82f6'

function labels(): string[] {
  return props.points.map((p) => {
    const d = new Date(p.timestamp)
    return `${String(d.getHours()).padStart(2, '0')}:${String(
      d.getMinutes()
    ).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
  })
}

function values(): number[] {
  return props.points.map((p) =>
    props.metric === 'cpu' ? p.cpu_usage : p.memory_usage
  )
}

function buildConfig(): ChartConfiguration {
  const color = cssVar(props.colorVar) || COLOR_FALLBACK
  const grid = cssVar('--color-border') || 'rgba(128,128,128,0.15)'
  const tick = cssVar('--color-muted-foreground') || '#888'
  return {
    type: 'line',
    data: {
      labels: labels(),
      datasets: [
        {
          data: values(),
          borderColor: color,
          backgroundColor: color,
          borderWidth: 2,
          fill: true,
          tension: 0.35,
          pointRadius: 0,
          pointHoverRadius: 3,
          pointHoverBackgroundColor: color,
        },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: { duration: prefersReduced() ? 0 : 300 },
      interaction: { intersect: false, mode: 'index' },
      plugins: {
        legend: { display: false },
        tooltip: {
          callbacks: {
            label: (ctx) =>
              ` ${t(props.metric === 'cpu' ? 'cpuUsage' : 'memoryUsage')}: ${Number(ctx.parsed.y).toFixed(1)}%`,
          },
        },
      },
      scales: {
        x: {
          display: true,
          grid: { display: false },
          ticks: {
            color: tick,
            font: { size: 10 },
            maxRotation: 0,
            autoSkipPadding: 24,
          },
          border: { display: false },
        },
        y: {
          min: 0,
          max: props.max,
          grid: { color: grid },
          ticks: {
            color: tick,
            font: { size: 10 },
            stepSize: 25,
            callback: (v) => `${v}%`,
          },
          border: { display: false },
        },
      },
    },
  }
}

function update() {
  if (!chart) return
  chart.data.labels = labels()
  chart.data.datasets[0].data = values()
  chart.update(prefersReduced() ? 'none' : undefined)
}

onMounted(async () => {
  await nextTick()
  if (canvasRef.value) {
    chart = new Chart(canvasRef.value, buildConfig())
  }
})

watch(
  () => props.points,
  () => update(),
  { deep: true }
)

onUnmounted(() => {
  chart?.destroy()
  chart = null
})
</script>
