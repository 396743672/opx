<template>
  <Teleport to="body">
  <div class="overlay" @click.self="$emit('close')">
    <div class="dialog narrow">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:alert" class="warn" /> {{ $t('uninstallConfirm') }}
        </div>
        <button class="dialog-close" @click="$emit('close')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="software-info">
        <b>{{ software.name }} {{ software.version }}</b>
        <div class="path">{{ software.install_path }}</div>
      </div>

      <div v-if="loading" class="loading">{{ $t('checking') }}</div>

      <div v-else-if="report && !report.safe" class="blocker-list">
        <div v-for="b in report.blockers" :key="b.kind" class="blocker-item">
          <Icon icon="mdi:close-circle" class="warn" />
          <div>
            <div>{{ $t(b.message_i18n) }}</div>
            <ul v-if="b.dependents.length" class="dependent-list">
              <li
                v-for="d in b.dependents"
                :key="d.id"
                :class="{ running: d.status === 'running' }"
              >
                {{ d.name }} ({{ d.status }})
              </li>
            </ul>
          </div>
        </div>
      </div>

      <div v-else class="ok-section">
        <div>{{ $t('uninstallSafeConfirm') }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button
          v-if="report && report.safe"
          class="btn danger"
          :disabled="uninstalling"
          @click="onConfirm"
        >
          <Icon icon="mdi:delete" />
          {{ uninstalling ? $t('uninstalling') : $t('uninstall') }}
        </button>
        <button v-else class="btn danger" disabled>{{ $t('uninstall') }}</button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import type { InstalledSoftware, UninstallSafetyReport } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{ close: []; uninstalled: [] }>()

const loading = ref(true)
const report = ref<UninstallSafetyReport | null>(null)
const uninstalling = ref(false)

onMounted(async () => {
  try {
    report.value = await invoke<UninstallSafetyReport>('check_uninstall_safety', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('check uninstall safety failed:', e)
  }
  loading.value = false
})

async function onConfirm() {
  uninstalling.value = true
  try {
    await invoke('uninstall_software', { installedId: props.software.id })
    emit('uninstalled')
    emit('close')
  } catch (e) {
    console.error('uninstall failed:', e)
  } finally {
    uninstalling.value = false
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
.dialog-title .warn {
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
.software-info {
  font-size: 13px;
  margin-bottom: 12px;
}
.path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  font-family: ui-monospace, monospace;
  word-break: break-all;
}
.loading {
  padding: 16px;
  text-align: center;
  color: var(--color-muted-foreground);
}
.blocker-list {
  padding: 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-destructive) 8%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 25%, transparent);
}
.blocker-item {
  display: flex;
  gap: 8px;
  padding: 4px 0;
  font-size: 13px;
}
.blocker-item .warn {
  color: var(--color-destructive);
  flex-shrink: 0;
}
.dependent-list {
  margin-top: 8px;
  padding-left: 24px;
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.dependent-list li.running {
  color: var(--color-success);
}
.ok-section {
  padding: 12px;
  border-radius: 6px;
  background: var(--color-muted);
  font-size: 13px;
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
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
