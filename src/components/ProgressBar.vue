<template>
  <div :class="variant === 'linear' ? 'w-full' : 'inline-block'">
    <div v-if="showLabel" class="flex items-center justify-between mb-1.5">
      <span class="text-xs text-muted-foreground">{{ label }}</span>
      <span class="text-xs font-medium tnum">{{ value.toFixed(1) }}%</span>
    </div>

    <!-- 线性进度条 -->
    <div
      v-if="variant === 'linear'"
      class="h-1.5 w-full bg-muted rounded-full overflow-hidden"
    >
      <div
        class="h-full rounded-full transition-all duration-300 ease-out"
        :style="{
          width: `${Math.min(value, 100)}%`,
          backgroundColor: `var(--color-${activeToken})`,
        }"
      ></div>
    </div>

    <!-- 环形进度条 -->
    <div v-else class="relative inline-flex items-center justify-center">
      <svg
        :width="size"
        :height="size"
        :viewBox="`0 0 ${size} ${size}`"
        class="-rotate-90"
      >
        <circle
          :cx="size / 2"
          :cy="size / 2"
          :r="radius"
          fill="none"
          stroke="var(--color-muted)"
          :stroke-width="strokeWidth"
        />
        <circle
          :cx="size / 2"
          :cy="size / 2"
          :r="radius"
          fill="none"
          :stroke="`var(--color-${activeToken})`"
          :stroke-width="strokeWidth"
          stroke-linecap="round"
          :stroke-dasharray="circumference"
          :stroke-dashoffset="dashOffset"
          class="transition-all duration-300 ease-out"
        />
      </svg>
      <span
        class="absolute text-xs font-semibold tnum"
        :style="{ fontSize: `${size * 0.22}px` }"
      >
        {{ value.toFixed(0) }}<span class="opacity-60">%</span>
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

interface Props {
  label?: string
  value: number
  warningThreshold?: number
  dangerThreshold?: number
  variant?: 'linear' | 'ring'
  size?: number
  strokeWidth?: number
  showLabel?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  warningThreshold: 60,
  dangerThreshold: 85,
  variant: 'linear',
  size: 56,
  strokeWidth: 6,
  showLabel: true,
})

/** 当前状态对应的语义色 token 名（不含 --color- 前缀） */
const activeToken = computed(() => {
  if (props.value >= props.dangerThreshold) return 'destructive'
  if (props.value >= props.warningThreshold) return 'warning'
  return 'primary'
})

const radius = computed(() => (props.size - props.strokeWidth) / 2)
const circumference = computed(() => 2 * Math.PI * radius.value)
const dashOffset = computed(
  () => circumference.value * (1 - Math.min(props.value, 100) / 100)
)
</script>
