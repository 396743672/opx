<template>
  <div class="flex items-center justify-between gap-4 mb-8">
    <div class="flex items-center gap-3 min-w-0">
      <div
        v-if="icon"
        class="flex items-center justify-center w-9 h-9 rounded-xl bg-primary/8 text-primary flex-shrink-0"
      >
        <Icon :icon="icon" class="text-xl" />
      </div>
      <div class="min-w-0">
        <h1 class="text-lg font-semibold tracking-tight">{{ title }}</h1>
        <p v-if="subtitle" class="text-sm text-muted-foreground/70">
          {{ subtitle }}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-1.5 flex-wrap justify-end" style="max-width:65%">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'

interface Props {
  title: string
  subtitle?: string
  icon?: string
}
defineProps<Props>()
</script>
