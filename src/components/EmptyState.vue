<template>
  <div
    class="flex flex-col items-center justify-center text-center py-16 px-6 rounded-lg border border-dashed border-border bg-card/50"
  >
    <div
      class="flex items-center justify-center w-14 h-14 rounded-full bg-muted text-muted-foreground mb-4"
    >
      <Icon :icon="icon" class="text-3xl" />
    </div>
    <h3 class="text-base font-medium">{{ title }}</h3>
    <p v-if="description" class="mt-1.5 text-sm text-muted-foreground max-w-sm">
      {{ description }}
    </p>
    <div v-if="$slots.action" class="mt-5">
      <slot name="action" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'

interface Props {
  icon: string
  title: string
  description?: string
}
defineProps<Props>()
</script>
