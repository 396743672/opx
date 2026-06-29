<template>
  <span
    class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium border"
    :class="statusClass"
  >
    <span
      class="w-1.5 h-1.5 rounded-full"
      :class="[dotClass, processing && 'animate-pulse']"
    ></span>
    {{ text }}
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { SoftwareStatus } from '@/models/software'
import type { AppStatus } from '@/models/springboot'

const { t } = useI18n()

interface Props {
  status: SoftwareStatus | AppStatus
}
const props = defineProps<Props>()

const map: Record<
  string,
  { textKey: string; processing: boolean; cls: string; dot: string }
> = {
  Running: {
    textKey: 'running',
    processing: false,
    cls: 'bg-success/10 text-success border-success/20',
    dot: 'bg-success',
  },
  Stopped: {
    textKey: 'stopped',
    processing: false,
    cls: 'bg-muted text-muted-foreground border-border',
    dot: 'bg-muted-foreground',
  },
  Error: {
    textKey: 'error',
    processing: false,
    cls: 'bg-destructive/10 text-destructive border-destructive/20',
    dot: 'bg-destructive',
  },
  Unknown: {
    textKey: 'unknown',
    processing: false,
    cls: 'bg-warning/10 text-warning border-warning/20',
    dot: 'bg-warning',
  },
  Starting: {
    textKey: 'starting',
    processing: true,
    cls: 'bg-warning/10 text-warning border-warning/20',
    dot: 'bg-warning',
  },
  Stopping: {
    textKey: 'stopping',
    processing: true,
    cls: 'bg-warning/10 text-warning border-warning/20',
    dot: 'bg-warning',
  },
}

const info = computed(() => map[props.status as string] ?? map.Unknown)
const text = computed(() => t(info.value.textKey))
const statusClass = computed(() => info.value.cls)
const dotClass = computed(() => info.value.dot)
const processing = computed(() => info.value.processing)
</script>
