<template>
  <card v-if="jvmInfo" class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('jvmMonitor') }}</h3>
    </card-header>
    <card-content>
      <div class="space-y-4">
        <div>
          <div class="flex justify-between text-sm mb-1">
            <span>{{ $t('heapMemory') }}</span>
            <span>{{ formatBytes(jvmInfo.heap_used) }} / {{ formatBytes(jvmInfo.heap_max) }}</span>
          </div>
          <div class="w-full bg-secondary rounded-full h-4">
            <div
              class="h-4 rounded-full bg-primary transition-all duration-500"
              :style="{ width: `${Math.round((jvmInfo.heap_used / jvmInfo.heap_max) * 100)}%` }"
            ></div>
          </div>
        </div>
        <div>
          <div class="flex justify-between text-sm mb-1">
            <span>{{ $t('nonHeapMemory') }}</span>
            <span>{{ formatBytes(jvmInfo.non_heap_used) }}</span>
          </div>
          <div class="w-full bg-secondary rounded-full h-4">
            <div
              class="h-4 rounded-full bg-primary transition-all duration-500"
              :style="{ width: `${Math.min((jvmInfo.non_heap_used / (jvmInfo.non_heap_used + 1024 * 1024 * 100)) * 100, 100)}%` }"
            ></div>
          </div>
        </div>
        <div class="grid grid-cols-2 gap-4">
          <div class="text-center">
            <div class="text-2xl font-bold">{{ jvmInfo.thread_count }}</div>
            <div class="text-sm text-muted-foreground">{{ $t('threadCount') }}</div>
          </div>
          <div class="text-center">
            <div class="text-2xl font-bold">{{ jvmInfo.gc_count }}</div>
            <div class="text-sm text-muted-foreground">{{ $t('gcCount') }}</div>
          </div>
        </div>
      </div>
    </card-content>
  </card>
  <div v-else class="p-8 text-center text-muted-foreground">
    {{ $t('noJvmInfo') }}
  </div>
</template>

<script setup lang="ts">
import type { JvmInfo } from '@/models/springboot'
import { useI18n } from 'vue-i18n'

withDefaults(defineProps<{
  jvmInfo: JvmInfo | null
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