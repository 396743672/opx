<template>
  <Teleport to="body">
    <div v-if="state.visible" class="overlay" @click.self="cancel">
      <div class="dialog" :class="{ danger: state.danger }">
        <div class="dialog-head">
          <Icon icon="mdi:alert-circle" class="warn" />
          <div class="title">{{ state.title || $t('confirm') }}</div>
        </div>
        <div class="message">{{ state.message }}</div>
        <div class="dialog-footer">
          <button class="btn" @click="cancel">
            {{ state.cancelText || $t('cancel') }}
          </button>
          <button class="btn" :class="{ primary: !state.danger, danger: state.danger }" @click="ok">
            {{ state.okText || $t('confirm') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { useConfirm } from '@/composables/useConfirm'

const { state, resolve } = useConfirm()

const ok = () => resolve(true)
const cancel = () => resolve(false)
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
}
.dialog {
  width: 360px;
  max-width: 90vw;
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 10px;
  box-shadow: 0 10px 30px oklch(0 0 0 / 0.35);
  padding: 16px 18px;
}
.dialog-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 10px;
}
.warn {
  color: var(--color-warning);
}
.dialog.danger .warn {
  color: var(--color-danger);
}
.message {
  font-size: 13px;
  color: var(--color-foreground);
  line-height: 1.5;
  margin-bottom: 16px;
  white-space: pre-wrap;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
