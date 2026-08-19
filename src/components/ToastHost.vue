<template>
  <Teleport to="body">
    <div class="toast-host">
      <TransitionGroup name="toast">
        <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
          <Icon :icon="kindIcon(t.kind)" />
          <span class="toast-msg">{{ t.message }}</span>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { useToast, type ToastKind } from '@/composables/useToast'

const { toasts } = useToast()

const kindIcon = (kind: ToastKind) =>
  kind === 'ok'
    ? 'mdi:check-circle'
    : kind === 'err'
      ? 'mdi:alert-circle'
      : 'mdi:information-outline'
</script>

<style scoped>
.toast-host {
  position: fixed;
  bottom: 20px;
  right: 20px;
  z-index: 120;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}
.toast {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 380px;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 4px 16px oklch(0 0 0 / 0.3);
  font-size: 13px;
}
.toast svg {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}
.toast-msg {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.toast.ok svg {
  color: var(--color-success);
}
.toast.err svg {
  color: var(--color-destructive);
}
.toast.info svg {
  color: var(--color-info);
}
.toast-enter-active,
.toast-leave-active {
  transition: all 0.2s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(16px);
}
</style>
