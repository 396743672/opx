<template>
  <span class="status-badge" :class="statusClass">
    <span class="dot" :class="{ pulse: isTransitioning }"></span>
    <Icon v-if="isTransitioning" icon="mdi:loading" class="spin" />
    <Icon v-else-if="status === SoftwareStatus.Error" icon="mdi:alert-circle" />
    <Icon v-else-if="status === SoftwareStatus.Unknown" icon="mdi:help-circle" />
    {{ label }}
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { SoftwareStatus } from '@/models/software'

const props = defineProps<{
  status: SoftwareStatus
  error?: string | null
}>()

const { t } = useI18n()

const label = computed(() => {
  switch (props.status) {
    case SoftwareStatus.Running:
      return t('running')
    case SoftwareStatus.Stopped:
      return t('stopped')
    case SoftwareStatus.Starting:
      return t('starting')
    case SoftwareStatus.Stopping:
      return t('stopping')
    case SoftwareStatus.Error:
      return t('error')
    case SoftwareStatus.Unknown:
      return t('unknown')
    case SoftwareStatus.Initializing:
      return t('initializing')
    default:
      return props.status
  }
})

const statusClass = computed(() => props.status.toLowerCase())

const isTransitioning = computed(
  () =>
    props.status === SoftwareStatus.Starting ||
    props.status === SoftwareStatus.Stopping ||
    props.status === SoftwareStatus.Initializing,
)
</script>

<style scoped>
.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  border: 1px solid;
  white-space: nowrap;
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
}
.dot.pulse {
  animation: pulse 1s ease-in-out infinite;
}
.spin {
  animation: spin 1s linear infinite;
}

.running {
  background: color-mix(in oklch, var(--color-success) 14%, transparent);
  color: var(--color-success);
  border-color: color-mix(in oklch, var(--color-success) 25%, transparent);
}
.running .dot {
  background: var(--color-success);
}

.stopped {
  background: var(--color-muted);
  color: var(--color-muted-foreground);
  border-color: var(--color-border);
}
.stopped .dot {
  background: var(--color-muted-foreground);
}

.starting,
.stopping,
.initializing {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
  border-color: color-mix(in oklch, var(--color-warning) 25%, transparent);
}
.starting .dot,
.stopping .dot,
.initializing .dot {
  background: var(--color-warning);
}

.error {
  background: color-mix(in oklch, var(--color-destructive) 14%, transparent);
  color: var(--color-destructive);
  border-color: color-mix(in oklch, var(--color-destructive) 30%, transparent);
}
.error .dot {
  background: var(--color-destructive);
}

.unknown {
  background: var(--color-muted);
  color: var(--color-muted-foreground);
  border-color: var(--color-border);
}
.unknown .dot {
  background: var(--color-muted-foreground);
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.3;
  }
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
