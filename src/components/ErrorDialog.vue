<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog narrow">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon icon="mdi:alert-circle" class="warn" /> {{ $t('error') }}
          </div>
          <button class="dialog-close" @click="close">
            <Icon icon="mdi:close" />
          </button>
        </div>
        <div class="error-msg">{{ message }}</div>
        <div class="dialog-footer">
          <button class="btn primary" @click="close">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useLifecycleStore } from '../modules/software-manager/stores/lifecycle'
import { translateError } from '@/utils/i18nError'

const { t, te } = useI18n()
const lifecycleStore = useLifecycleStore()

const message = computed(() => translateError(lifecycleStore.errorMessage, t, te))

function close() {
  lifecycleStore.clearErrorMessage()
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
}
.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
}
.dialog-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
  font-weight: 600;
}
.dialog-title .warn {
  color: var(--color-destructive);
}
.dialog-title svg {
  width: 22px;
  height: 22px;
}

.error-msg {
  font-size: 13px;
  color: var(--color-foreground);
  padding: 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-destructive) 8%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 25%, transparent);
  margin-bottom: 16px;
  white-space: pre-wrap;
  word-break: break-all;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
}
.btn:hover {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
</style>
