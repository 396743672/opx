<template>
  <div class="flex items-center justify-between mb-4">
    <div class="flex items-center gap-2.5 min-w-0">
      <slot name="icon" />
      <div class="min-w-0">
        <h2 class="text-base font-semibold truncate">{{ title }}</h2>
        <p v-if="subtitle" class="text-xs text-muted-foreground truncate">
          {{ subtitle }}
        </p>
      </div>
    </div>
    <div v-if="!hideRefresh || $slots.actions" class="flex items-center gap-1.5">
      <slot name="actions" />
      <button
        v-if="!hideRefresh"
        class="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors cursor-pointer"
        :aria-label="$t('refresh')"
        @click="$emit('refresh')"
      >
        <Icon icon="mdi:refresh" class="text-lg" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'

interface Props {
  title: string
  subtitle?: string
  hideRefresh?: boolean
}
defineProps<Props>()
defineEmits<{ refresh: [] }>()

useI18n()
</script>
