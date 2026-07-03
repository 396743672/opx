<template>
  <div class="overlay" @click.self="$emit('close')">
    <div class="dialog">
      <div>{{ $t('editConfig') }}（{{ software.name }}）</div>
      <div class="hint">{{ $t('todoPlaceholder') }}</div>
      <button class="btn" @click="$emit('close')">{{ $t('close') }}</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { InstalledSoftware } from '@/models/software'

defineProps<{ software: InstalledSoftware }>()
defineEmits<{ close: [] }>()
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}
.dialog {
  background: var(--color-card);
  border-radius: 8px;
  padding: 24px;
  min-width: 320px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.hint {
  color: var(--color-muted-foreground);
  font-size: 13px;
}
.btn {
  align-self: flex-end;
  padding: 6px 16px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  cursor: pointer;
}
</style>
