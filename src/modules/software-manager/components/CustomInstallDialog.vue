<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('cancel')">
      <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:plus-box" />
          {{ $t('uploadCustom') }}
        </div>
        <button class="dialog-close" @click="$emit('cancel')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('customName') }}</div>
        <input class="input" v-model="name" placeholder="my-tool" />
      </div>

      <div class="field">
        <div class="field-label">{{ $t('selectArchive') }}</div>
        <div class="file-drop" @click="selectFile">
          <template v-if="!archivePath">
            <Icon icon="mdi:upload" class="upload-icon" />
            <div class="main-text">{{ $t('dragFileHere') }}</div>
            <div class="sub-text">{{ $t('supportedFormats') }}</div>
          </template>
          <template v-else>
            <div class="file-chosen">
              <Icon icon="mdi:check" />
              <span class="path">{{ archiveName }}</span>
              <button class="btn ghost" @click.stop="selectFile">{{ $t('edit') }}</button>
            </div>
          </template>
        </div>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('installPath') }}</div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
        <button class="btn primary" @click="install" :disabled="!canInstall || installing">
          {{ $t('install') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@/utils/ipc'
import { useInstallStore } from '../stores/install'

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const name = ref('')
const archivePath = ref('')
const installing = ref(false)

const canInstall = computed(() => name.value.trim() !== '' && archivePath.value !== '')
const archiveName = computed(() => {
  const parts = archivePath.value.split(/[\\/]/)
  return parts[parts.length - 1] || ''
})
const installPath = computed(() =>
  name.value.trim() ? `apps/custom/${name.value.trim()}` : '',
)

async function selectFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Archives', extensions: ['zip', 'gz', 'tgz'] }],
  })
  if (selected && typeof selected === 'string') {
    archivePath.value = selected
  }
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_custom', {
      params: {
        name: name.value.trim(),
        archive_path: archivePath.value,
      },
    }) as string
    useInstallStore().createTask(installId, 'custom', name.value.trim())
    emit('installed', installId)
  } catch (e) {
    console.error('Failed to install:', e)
  } finally {
    installing.value = false
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
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.dialog {
  width: 420px;
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

.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: var(--color-primary);
}
.input.readonly {
  color: var(--color-muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.file-drop {
  border: 1px dashed var(--color-border);
  border-radius: 8px;
  padding: 18px;
  text-align: center;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.file-drop:hover {
  border-color: var(--color-primary);
  background: color-mix(in oklch, var(--color-primary) 6%, transparent);
}
.upload-icon {
  width: 28px;
  height: 28px;
  color: var(--color-muted-foreground);
  margin: 0 auto 6px;
}
.file-drop .main-text {
  font-size: 13px;
}
.file-drop .sub-text {
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
}
.file-chosen {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--color-muted);
  border-radius: 6px;
  font-size: 12px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
}
.file-chosen svg {
  width: 14px;
  height: 14px;
  color: var(--color-success);
  flex-shrink: 0;
}
.file-chosen .path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  border: none;
  background: transparent;
  color: var(--color-foreground);
}
.btn:hover {
  background: var(--color-muted);
}
.btn.ghost:hover {
  background: var(--color-muted);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--color-border);
}
.btn.primary {
  height: 32px;
  padding: 0 12px;
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
