<template>
  <div class="overlay" @click.self="$emit('close')">
    <div class="dialog wide">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:cog-outline" />
          {{ software.name }} {{ software.version }} {{ $t('config') }}
        </div>
        <button class="dialog-close" @click="$emit('close')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="tab-bar">
        <button
          class="tab"
          :class="{ active: tab === 'form' }"
          @click="switchTab('form')"
        >
          {{ $t('formView') }}
        </button>
        <button
          class="tab"
          :class="{ active: tab === 'source' }"
          @click="switchTab('source')"
        >
          {{ $t('sourceView') }}
        </button>
      </div>

      <ConfigFormTab
        v-if="tab === 'form'"
        ref="formTabRef"
        :software="software"
        :schema="schema"
        @update:dirty="onDirty"
      />
      <ConfigSourceTab
        v-else
        ref="sourceTabRef"
        :software="software"
        @update:dirty="onDirty"
        @update:content="onSourceContent"
      />

      <div class="hint-bar">
        <Icon icon="mdi:information-outline" /> {{ $t('configEditRestartHint') }}
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button class="btn" :disabled="!dirty" @click="onSave">{{ $t('save') }}</button>
        <button class="btn primary" :disabled="!dirty" @click="onSaveAndRestart">
          {{ $t('saveAndRestart') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import ConfigFormTab from './ConfigFormTab.vue'
import ConfigSourceTab from './ConfigSourceTab.vue'
import type { ConfigSchema, FormData, InstalledSoftware } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{ close: [] }>()

const tab = ref<'form' | 'source'>('form')
const dirty = ref(false)
const schema = ref<ConfigSchema | null>(null)
const formTabRef = ref<InstanceType<typeof ConfigFormTab>>()
const sourceTabRef = ref<InstanceType<typeof ConfigSourceTab>>()
let sourceContent = ''

onMounted(async () => {
  try {
    schema.value = await invoke<ConfigSchema | null>('get_config_schema', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('get schema failed:', e)
  }
})

function switchTab(t: 'form' | 'source') {
  if (tab.value === t) return
  if (dirty.value) {
    if (!confirm('当前改动未保存，切换 tab 会丢失，确定吗？')) return
  }
  tab.value = t
  dirty.value = false
}

function onDirty(d: boolean) {
  dirty.value = d
}
function onSourceContent(c: string) {
  sourceContent = c
}

async function onSave() {
  try {
    if (tab.value === 'form' && formTabRef.value) {
      const data: FormData = (formTabRef.value as any).formData
      await invoke('write_config_form', {
        installedId: props.software.id,
        data,
      })
    } else if (tab.value === 'source' && sourceTabRef.value) {
      const content =
        (sourceTabRef.value as any).getContent?.() ?? sourceContent
      await invoke('write_config_source', {
        installedId: props.software.id,
        content,
      })
    }
    dirty.value = false
  } catch (e) {
    console.error('save config failed:', e)
  }
}

async function onSaveAndRestart() {
  await onSave()
  try {
    await invoke('restart_software', { installedId: props.software.id })
  } catch (e) {
    console.error('restart failed:', e)
  }
  emit('close')
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
  width: 540px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
}
.dialog.wide {
  width: 680px;
  max-height: 90vh;
  overflow-y: auto;
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
.tab-bar {
  display: flex;
  gap: 4px;
  padding: 4px;
  background: var(--color-muted);
  border-radius: 6px;
  margin-bottom: 16px;
}
.tab {
  flex: 1;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  text-align: center;
  color: var(--color-muted-foreground);
  border: none;
  background: transparent;
}
.tab.active {
  background: var(--color-card);
  color: var(--color-primary);
  font-weight: 500;
}
.hint-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-info) 8%, transparent);
  color: var(--color-info);
  font-size: 12px;
  margin-top: 16px;
}
.hint-bar svg {
  width: 14px;
  height: 14px;
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
