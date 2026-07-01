<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('cancel')">
      <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:package-variant-closed" />
          {{ $t('install') }} {{ entry.name }}
        </div>
        <button class="dialog-close" @click="$emit('cancel')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('selectVersion') }}</div>
        <div class="version-list">
          <div
            v-for="(v, idx) in entry.versions"
            :key="v.version"
            class="version-row"
            :class="{ selected: selectedVersionIdx === idx }"
            @click="selectedVersionIdx = idx"
          >
            <span class="v-name">{{ v.version }}</span>
            <span v-if="v.version === entry.default_version && entry.key !== 'jre'" class="v-badge">{{ $t('latestVersion') }}</span>
            <Icon v-if="selectedVersionIdx === idx" icon="mdi:check" class="v-check" />
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('selectMirror') }}</div>
        <div class="select" @click="showMirrorDropdown = !showMirrorDropdown">
          <span>{{ selectedMirror?.name }}</span>
          <Icon icon="mdi:chevron-down" class="caret" />
        </div>
        <div v-if="showMirrorDropdown" class="dropdown">
          <div
            v-for="(m, idx) in selectedVersion?.mirrors"
            :key="idx"
            class="dropdown-item"
            :class="{ selected: selectedMirrorIdx === idx }"
            @click="selectMirror(idx)"
          >
            {{ m.name }}
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('installPath') }}</div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
        <button class="btn primary" @click="install" :disabled="installing">
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
import { invoke } from '@tauri-apps/api/core'
import { useInstallStore } from '../stores/install'
import type { CatalogEntry } from '@/models/software'

const props = defineProps<{
  entry: CatalogEntry
}>()

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const selectedVersionIdx = ref(0)
const selectedMirrorIdx = ref(0)
const showMirrorDropdown = ref(false)
const installing = ref(false)

const selectedVersion = computed(() => props.entry.versions[selectedVersionIdx.value])
const selectedMirror = computed(() => selectedVersion.value?.mirrors[selectedMirrorIdx.value])
const installPath = computed(
  () => `apps/${props.entry.key}/${selectedVersion.value?.version}`,
)

function selectMirror(idx: number) {
  selectedMirrorIdx.value = idx
  showMirrorDropdown.value = false
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_software', {
      params: {
        key: props.entry.key,
        version: selectedVersion.value.version,
        mirror_index: selectedMirrorIdx.value,
        set_as_default_jre: false,
      },
    }) as string
    useInstallStore().createTask(
      installId,
      props.entry.key,
      `${props.entry.name} ${selectedVersion.value.version}`,
    )
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
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.select {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
}
.select:hover {
  border-color: var(--color-primary);
}
.caret {
  color: var(--color-muted-foreground);
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
  display: flex;
  align-items: center;
}
.input.readonly {
  color: var(--color-muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.version-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.version-row:hover {
  background: var(--color-muted);
}
.version-row.selected {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.v-name {
  flex: 1;
  font-size: 13px;
}
.v-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--color-info) 14%, transparent);
  color: var(--color-info);
}
.v-check {
  width: 16px;
  height: 16px;
}
.dropdown {
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-top: 4px;
  box-shadow: var(--shadow-popover);
}
.dropdown-item {
  padding: 8px 12px;
  cursor: pointer;
  font-size: 13px;
}
.dropdown-item:hover {
  background: var(--color-muted);
}
.dropdown-item.selected {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
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
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
