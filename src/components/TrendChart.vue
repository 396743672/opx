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
import { useSettingsStore } from '@/stores/settings'
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
const settingsStore = useSettingsStore()

function cssVar(name: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim()
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
      animation: { duration: 0 },
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
  const color = cssVar(props.colorVar) || COLOR_FALLBACK
  ;(chart.data.datasets[0] as any).borderColor = color
  ;(chart.data.datasets[0] as any).backgroundColor = color
  ;(chart.data.datasets[0] as any).pointHoverBackgroundColor = color
  chart.update('none')
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

// 主题 class（dark/warm，见 settings.ts applyTheme）应用后再重读 CSS 变量
// 重绘——修复刷新竞态：首渲早于主题 class 应用时 getComputedStyle 取到
// 默认主题的蓝/紫（实测多次刷新图表颜色漂移）。auto 主题随系统切换也要跟随。
watch(
  () => [settingsStore.theme, settingsStore.systemPrefersDark] as const,
  async () => {
    await nextTick()
    update()
  },
)

onUnmounted(() => {
  chart?.destroy()
  chart = null
})
</script>
