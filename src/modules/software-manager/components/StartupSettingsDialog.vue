<template>
  <Teleport to="body">
  <div class="overlay" @click.self="$emit('close')">
    <div class="dialog narrow">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:tune-vertical" /> {{ $t('startupSettings') }}
        </div>
        <button class="dialog-close" @click="$emit('close')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="software-name">
        <b>{{ software.name }} {{ software.version }}</b>
        <div class="path">{{ software.install_path }}</div>
      </div>

      <div class="startup-row">
        <div
          class="toggle"
          :class="{ off: !autoStart }"
          role="switch"
          :aria-checked="autoStart"
          tabindex="0"
          @click="autoStart = !autoStart"
          @keydown.enter.prevent="autoStart = !autoStart"
          @keydown.space.prevent="autoStart = !autoStart"
        ></div>
        <div class="label">
          <div>{{ $t('autoStartOnAppStart') }}</div>
          <div class="desc">{{ $t('autoStartOnAppStartDesc') }}</div>
        </div>
      </div>

      <div class="startup-row">
        <div
          class="toggle"
          :class="{ off: !autoRestart }"
          role="switch"
          :aria-checked="autoRestart"
          tabindex="0"
          @click="autoRestart = !autoRestart"
          @keydown.enter.prevent="autoRestart = !autoRestart"
          @keydown.space.prevent="autoRestart = !autoRestart"
        ></div>
        <div class="label">
          <div>{{ $t('autoRestart') }}</div>
        </div>
      </div>

      <div class="field">
        <label class="form-field-label">{{ $t('startupOrder') }}</label>
        <div class="order-input">
          <input
            v-model.number="order"
            type="number"
            min="0"
            class="startup-order-input tnum"
          />
          <span class="desc">{{ $t('startupOrderDesc') }}</span>
        </div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button class="btn primary" :disabled="saving" @click="onSave">
          {{ $t('save') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import type { InstalledSoftware } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{ close: [] }>()

const autoStart = ref(false)
const order = ref(0)
const autoRestart = ref(false)
const saving = ref(false)

onMounted(() => {
  autoStart.value = props.software.auto_start_on_app_start
  order.value = props.software.startup_order
  autoRestart.value = props.software.auto_restart ?? false
})

async function onSave() {
  saving.value = true
  try {
    await invoke('save_startup_settings', {
      installedId: props.software.id,
      autoStart: autoStart.value,
      order: order.value,
      autoRestart: autoRestart.value,
    })
    emit('close')
  } catch (e) {
    console.error('save startup settings failed:', e)
  } finally {
    saving.value = false
  }
}
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
  margin-bottom: 16px;
}
.dialog-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
  font-weight: 600;
}
.dialog-title svg {
  color: var(--color-primary);
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
.software-name {
  font-size: 13px;
  margin-bottom: 14px;
}
.path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  font-family: ui-monospace, monospace;
  word-break: break-all;
}
.startup-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: 6px;
  background: var(--color-muted);
  margin-bottom: 12px;
}
.toggle {
  width: 36px;
  height: 20px;
  border-radius: 999px;
  background: var(--color-primary);
  position: relative;
  cursor: pointer;
  transition: background 0.2s;
  flex-shrink: 0;
}
.toggle::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 18px;
  width: 16px;
  height: 16px;
  border-radius: 999px;
  background: white;
  transition: left 0.2s;
}
.toggle.off {
  background: var(--color-border);
}
.toggle.off::after {
  left: 2px;
}
.label {
  flex: 1;
  font-size: 13px;
}
.desc {
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
}
.field {
  margin-bottom: 14px;
}
.form-field-label {
  display: block;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.order-input {
  display: flex;
  gap: 8px;
  align-items: center;
}
.startup-order-input {
  width: 80px;
  height: 30px;
  padding: 0 8px;
  background: var(--color-muted);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  color: var(--color-foreground);
  font-size: 13px;
  text-align: center;
  outline: none;
}
.startup-order-input:focus {
  border-color: var(--color-primary);
}
.tnum {
  font-variant-numeric: tabular-nums;
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
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
