<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('memoryUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="flex items-center justify-between mb-4">
        <span class="text-3xl font-bold">{{ memoryUsage?.toFixed(1) }}%</span>
        <span class="text-muted-foreground">{{ formatBytes(memoryUsed) }} / {{ formatBytes(memoryTotal) }}</span>
      </div>
      <div class="w-full bg-secondary rounded-full h-4">
        <div
          class="h-4 rounded-full bg-primary transition-all duration-500"
          :style="{ width: `${Math.min(memoryUsage, 100)}%` }"
        ></div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  memoryUsed: number
  memoryTotal: number
  memoryUsage: number
}>(), {})

const { t } = useI18n()

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}
</script>