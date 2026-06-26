<template>
  <n-badge :type="badgeType" :processing="processing">
    <span class="text-xs px-2 py-1 rounded-full">{{ text }}</span>
  </n-badge>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { SoftwareStatus } from '@/models/software'
import type { AppStatus } from '@/models/springboot'

const { t } = useI18n()

interface Props {
  status: SoftwareStatus | AppStatus
}

const props = defineProps<Props>()

const statusMap: Record<string, { type: 'success' | 'error' | 'warning' | 'default'; textKey: string, processing: boolean }> = {
  Running: { type: 'success', textKey: 'running', processing: false },
  Stopped: { type: 'default', textKey: 'stopped', processing: false },
  Error: { type: 'error', textKey: 'error', processing: false },
  Unknown: { type: 'warning', textKey: 'unknown', processing: false },
  Starting: { type: 'warning', textKey: 'starting', processing: true },
  Stopping: { type: 'warning', textKey: 'stopping', processing: true },
}

const badgeInfo = statusMap[props.status]
const badgeType = badgeInfo.type
const processing = badgeInfo.processing
const text = t(badgeInfo.textKey)
</script>

<style scoped>
</style>
