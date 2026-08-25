<template>
  <div class="category-tabs" role="tablist">
    <button
      v-for="tab in tabs"
      :key="tab.key"
      class="category-tab"
      :class="{ active: modelValue === tab.key }"
      role="tab"
      type="button"
      :aria-selected="modelValue === tab.key"
      @click="$emit('update:modelValue', tab.key)"
    >
      <Icon :icon="tab.icon" />
      <span>{{ tab.label }}</span>
      <span v-if="tab.count != null" class="count">{{ tab.count }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'

export interface CategoryTab {
  key: string
  label: string
  icon: string
  count?: number
}

defineProps<{
  tabs: CategoryTab[]
  modelValue: string
}>()

defineEmits<{
  'update:modelValue': [value: string]
}>()
</script>

<style scoped>
.category-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.category-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  font-size: 12px;
  font-weight: 500;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-muted-foreground);
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s, background 0.15s;
}
.category-tab:hover {
  color: var(--color-foreground);
  border-color: color-mix(in oklch, var(--color-primary) 40%, var(--color-border));
}
.category-tab:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}
.category-tab.active {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  border-color: var(--color-primary);
  color: var(--color-primary);
  font-weight: 600;
}
.category-tab svg {
  width: 14px;
  height: 14px;
}
.category-tab .count {
  font-size: 11px;
  opacity: 0.7;
  font-variant-numeric: tabular-nums;
}
</style>