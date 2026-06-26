<template>
  <div class="flex items-center justify-between mb-4">
    <h2 class="text-lg font-semibold">{{ title }}</h2>
    <div v-if="!hideRefresh" class="flex items-center gap-2">
      <slot name="actions"></slot>
      <n-button quaternary @click="$emit('refresh')">
        <iconify-icon icon="mdi:refresh" class="text-lg" />
      </n-button>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Props {
  title: string
  hideRefresh?: boolean
}

defineProps<Props>()
defineEmits<{
  refresh: []
}>()
</script>

<style scoped>
</style>
