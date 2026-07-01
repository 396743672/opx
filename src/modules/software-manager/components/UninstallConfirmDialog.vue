<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('cancel')">
      <div class="dialog">
        <div class="dialog-head">
          <div class="dialog-title title-warning">
            <Icon icon="mdi:alert" />
            {{ $t('confirmUninstall') }}
          </div>
          <button class="dialog-close" @click="$emit('cancel')">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <div class="confirm-body">
          <p>{{ $t('uninstallConfirmDesc') }}</p>
          <div class="info-row">
            <span class="info-label">{{ $t('softwareName') }}</span>
            <span class="info-value">{{ software.name }}</span>
          </div>
          <div class="info-row">
            <span class="info-label">{{ $t('installPath') }}</span>
            <span class="info-value mono">{{ software.install_path }}</span>
          </div>
        </div>

        <div class="dialog-footer">
          <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
          <button class="btn danger" @click="$emit('confirm')" :disabled="uninstalling">
            <Icon icon="mdi:delete" /> {{ $t('uninstall') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import type { InstalledSoftware } from '@/models/software'

defineProps<{
  software: InstalledSoftware
  uninstalling: boolean
}>()

defineEmits<{
  confirm: []
  cancel: []
}>()
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.dialog {
  width: 460px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.title-warning {
  color: var(--color-destructive);
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.confirm-body {
  margin-bottom: 14px;
}
.confirm-body p {
  font-size: 13px;
  color: var(--color-muted-foreground);
  margin-bottom: 12px;
  line-height: 1.5;
}
.info-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 0;
  font-size: 13px;
}
.info-label {
  width: 80px;
  color: var(--color-muted-foreground);
  flex-shrink: 0;
}
.info-value {
  flex: 1;
  word-break: break-all;
}
.info-value.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--color-border);
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
.btn.danger {
  background: var(--color-destructive);
  color: white;
  border-color: var(--color-destructive);
}
.btn.danger:hover {
  background: color-mix(in oklch, var(--color-destructive) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
