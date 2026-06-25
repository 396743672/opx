<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('diskUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="space-y-4">
        <div v-for="disk in disks" :key="disk.mount_point" class="space-y-2">
          <div class="flex items-center justify-between text-sm">
            <span class="font-medium">{{ disk.mount_point }}</span>
            <span>{{ disk.usage.toFixed(1) }}%</span>
          </div>
          <div class="w-full bg-secondary rounded-full h-2">
            <div
              class="h-2 rounded-full bg-primary transition-all duration-500"
              :style="{ width: `${Math.min(disk.usage, 100)}%` }"
            ></div>
          </div>
          <div class="text-xs text-muted-foreground">
            {{ formatBytes(disk.used) }} / {{ formatBytes(disk.total) }}
          </div>
        </div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { DiskInfo } from '@/models/system'
import { formatBytes } from '@/utils/format'
withDefaults(defineProps<{
  disks: DiskInfo[]
}>(), {})

const { t } = useI18n()
</script>