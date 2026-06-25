<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('cpuUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="flex items-center justify-between mb-4">
        <span class="text-3xl font-bold">{{ cpuUsage?.toFixed(1) }}%</span>
      </div>
      <div class="w-full bg-secondary rounded-full h-4">
        <div
          class="h-4 rounded-full bg-primary transition-all duration-500"
          :style="{ width: `${Math.min(cpuUsage, 100)}%` }"
        ></div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  cpuUsage: number
}>(), {})
const { t } = useI18n()
</script>