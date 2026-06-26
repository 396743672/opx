<template>
  <div class="w-full">
    <div class="flex items-center justify-between mb-1">
      <span class="text-sm text-muted-foreground">{{ label }}</span>
      <span class="text-sm font-medium">{{ value.toFixed(1) }}%</span>
    </div>
    <div class="h-2 w-full bg-muted rounded-full overflow-hidden">
      <div
        class="h-full transition-all duration-300 rounded-full"
        :class="getBackgroundClass()"
        :style="{ width: `${Math.min(value, 100)}%` }"
      ></div>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Props {
  label: string
  value: number
  warningThreshold?: number
  dangerThreshold?: number
}

const props = withDefaults(defineProps<Props>(), {
  warningThreshold: 60,
  dangerThreshold: 85,
})

const getBackgroundClass = () => {
  if (props.value >= props.dangerThreshold) {
    return 'bg-destructive'
  } else if (props.value >= props.warningThreshold) {
    return 'bg-orange-500'
  } else {
    return 'bg-primary'
  }
}
</script>

<style scoped>
</style>
