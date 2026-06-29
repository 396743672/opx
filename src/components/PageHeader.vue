<template>
  <div class="flex items-center justify-between gap-4 mb-6">
    <div class="flex items-center gap-3 min-w-0">
      <div
        v-if="icon"
        class="flex items-center justify-center w-9 h-9 rounded-lg bg-primary/10 text-primary flex-shrink-0"
      >
        <Icon :icon="icon" class="text-xl" />
      </div>
      <div class="min-w-0">
        <h1 class="text-xl font-semibold truncate">{{ title }}</h1>
        <p v-if="subtitle" class="text-sm text-muted-foreground truncate">
          {{ subtitle }}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-2 flex-shrink-0">
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
