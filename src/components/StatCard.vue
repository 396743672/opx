<template>
  <div
    class="relative rounded-xl border border-border/60 bg-card p-5 shadow-card overflow-hidden transition-all duration-200 hover:border-primary/25 hover:shadow-elevated"
  >
    <!-- 顶部色带 -->
    <div
      class="absolute top-0 left-0 right-0 h-0.5"
      :style="{ backgroundColor: `var(--color-${accent})` }"
    ></div>

    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <div class="flex items-center gap-1.5 text-muted-foreground/70">
          <Icon :icon="icon" class="text-sm" />
          <span class="text-xs font-medium">{{ label }}</span>
        </div>
        <div class="mt-2 flex items-baseline gap-1">
          <span class="text-3xl font-semibold tracking-tight tnum">{{ value }}</span>
          <span v-if="unit" class="text-sm text-muted-foreground">{{ unit }}</span>
        </div>
        <div v-if="sub" class="mt-1.5 text-xs text-muted-foreground/60 tnum truncate leading-relaxed">
          {{ sub }}
        </div>
      </div>

      <!-- 环形指示 -->
      <ProgressBar
        v-if="progress !== undefined"
        :value="progress"
        variant="ring"
        :size="52"
        :show-label="false"
        class="flex-shrink-0"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import ProgressBar from './ProgressBar.vue'

interface Props {
  label: string
  value: string | number
  unit?: string
  sub?: string
  icon: string
  accent?: string
  progress?: number
}
withDefaults(defineProps<Props>(), {
  accent: 'primary',
})
</script>
