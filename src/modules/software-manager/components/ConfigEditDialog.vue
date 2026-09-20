<template>
  <Teleport to="body">
  <div class="overlay">
    <div class="dialog wide">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:cog-outline" />
          {{ software.name }} {{ software.version }} {{ $t('config') }}
          <button v-if="docUrl" class="btn btn-sm doc-btn" @click="openDoc">
            <Icon icon="mdi:book-open-variant" /> {{ $t('docs') }}
          </button>
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
        <button
          class="tab"
          :class="{ active: tab === 'backups' }"
          @click="switchTab('backups')"
        >
          {{ $t('configBackups') }}
        </button>
      </div>

      <div v-if="tab === 'form' && matchedPresets.length" class="preset-bar">
        <span class="preset-label">{{ $t('configPresets') }}:</span>
        <button
          v-for="p in matchedPresets"
          :key="p.nameKey"
          class="btn btn-sm"
          @click="applyPreset(p)"
        >
          <Icon icon="mdi:auto-fix" /> {{ $t(p.nameKey) }}
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
        v-else-if="tab === 'source'"
        ref="sourceTabRef"
        :software="software"
        @update:dirty="onDirty"
        @update:content="onSourceContent"
      />
      <div v-else-if="tab === 'backups'" class="backups-tab">
        <p class="backup-hint">
          <Icon icon="mdi:information-outline" /> {{ $t('backupHint') }}
        </p>
        <div v-if="loadingBackups" class="text-sm text-muted-foreground py-4 text-center">{{ $t('loading') }}</div>
        <div v-else-if="backups.length === 0" class="text-sm text-muted-foreground py-4 text-center">{{ $t('noBackups') }}</div>
        <div v-else class="backup-list">
          <div v-for="b in backups" :key="b.name" class="backup-row">
            <div class="backup-info">
              <Icon icon="mdi:file-document-outline" class="text-muted-foreground" />
              <span class="backup-name">{{ b.name }}</span>
              <span class="backup-size">{{ formatBytes(b.size) }}</span>
            </div>
            <button class="btn btn-sm" @click="onRestore(b.name)">
              {{ $t('restoreBackup') }}
            </button>
          </div>
        </div>
      </div>

      <div class="hint-bar">
        <Icon icon="mdi:information-outline" /> {{ $t('configEditRestartHint') }}
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button class="btn" :disabled="!dirty || saving" @click="onSave(true)">{{ $t('save') }}</button>
        <button class="btn primary" :disabled="!dirty || saving" @click="onSaveAndRestart">
          {{ $t('saveAndRestart') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useI18n } from 'vue-i18n'
import ConfigFormTab from './ConfigFormTab.vue'
import ConfigSourceTab from './ConfigSourceTab.vue'
import { useLifecycleStore } from '../stores/lifecycle'
import type { ConfigSchema, FormData, InstalledSoftware } from '@/models/software'
import { formatBytes } from '@/utils/format'

	const { t } = useI18n()
const lifecycleStore = useLifecycleStore()

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{ close: [] }>()

// 官方文档外链：key → docs URL
const DOC_URLS: Record<string, string> = {
  mysql: 'https://dev.mysql.com/doc/refman/8.4/en/',
  redis: 'https://redis.io/docs/latest/',
  nginx: 'https://nginx.org/en/docs/',
  minio: 'https://silo.pgsty.com/docs/',
  rustfs: 'https://github.com/influxdata/rustfs',
  postgresql: 'https://www.postgresql.org/docs/current/',
  mongodb: 'https://www.mongodb.com/docs/manual/',
  nacos: 'https://nacos.io/docs/',
  kafka: 'https://kafka.apache.org/documentation/',
  elasticsearch: 'https://www.elastic.co/guide/en/elasticsearch/reference/current/index.html',
  influxdb: 'https://docs.influxdata.com/influxdb/v2/',
  influxdb3: 'https://docs.influxdata.com/influxdb/v3/',
  jre: 'https://adoptium.net/',
  jdk: 'https://adoptium.net/',
  node: 'https://nodejs.org/en/docs/',
}
const docUrl = computed(() => DOC_URLS[props.software.key] ?? null)
function openDoc() {
  if (docUrl.value) openUrl(docUrl.value).catch(() => {})
}

const tab = ref<'form' | 'source' | 'backups'>('form')
const dirty = ref(false)
const saving = ref(false)
const schema = ref<ConfigSchema | null>(null)
const formTabRef = ref<InstanceType<typeof ConfigFormTab>>()
const sourceTabRef = ref<InstanceType<typeof ConfigSourceTab>>()
const backups = ref<{ name: string; size: number; modified: number }[]>([])
const loadingBackups = ref(false)
let sourceContent = ''

// ---- R3 配置预设：按软件 key 命中，应用一组 key→value 到表单 ----
const CONFIG_PRESETS: {
  key: string
  nameKey: string
  values: Record<string, string | number>
}[] = [
  {
    key: 'mysql',
    nameKey: 'presetMysqlDev',
    values: { max_connections: 500, wait_timeout: 28800, max_allowed_packet: 67108864 },
  },
  {
    key: 'redis',
    nameKey: 'presetRedisCache',
    values: { maxmemory: '256mb', maxmemory_policy: 'allkeys-lru' },
  },
]
const matchedPresets = computed(() =>
  CONFIG_PRESETS.filter((p) => p.key === props.software.key)
)
function applyPreset(preset: (typeof CONFIG_PRESETS)[number]) {
  const fd = formTabRef.value?.formData
  if (!fd) return
  for (const [k, v] of Object.entries(preset.values)) {
    if (k in fd) {
      fd[k] = v as never
    } else {
      // 允许从 schema 默认缺失的 key 直接写入
      fd[k] = v as never
    }
  }
  dirty.value = true
}

onMounted(async () => {
  try {
    schema.value = await invoke<ConfigSchema | null>('get_config_schema', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('get schema failed:', e)
  }
})

function switchTab(tabName: 'form' | 'source' | 'backups') {
  if (tab.value === tabName) return
  if (dirty.value) {
    if (!confirm(t('configDirtyConfirm'))) return
  }
  tab.value = tabName
  dirty.value = false
  if (tabName === 'backups') loadBackups()
}

function onDirty(d: boolean) {
  dirty.value = d
}
function onSourceContent(c: string) {
  sourceContent = c
}

async function onSave(closeAfter?: boolean) {
  saving.value = true
  try {
    if (tab.value === 'form' && formTabRef.value) {
      const data: FormData = (formTabRef.value as any).formData
      // 必填校验（field_rules.required）：缺失则中止并提示
      const missing = (formTabRef.value as any).validateRequired?.() ?? null
      if (missing) {
        alert(t('configRequiredMissing', { field: t(fieldLabel(missing)) }))
        return
      }
      await invoke('write_config_form', {
        installedId: props.software.id,
        data,
      })
      // 把 ephemeral 字段（如 MySQL 初始化密码）暂存到运行时 store，供 start_software 消费；
      // 这些字段不会被 write_config_form 持久化（后端跳过），仅存于内存、一次性消费。
      syncInitPassword(data)
    } else if (tab.value === 'source' && sourceTabRef.value) {
      const content =
        (sourceTabRef.value as any).getContent?.() ?? sourceContent
      await invoke('write_config_source', {
        installedId: props.software.id,
        content,
      })
    }
    dirty.value = false
    if (closeAfter) {
      emit('close')
    }
  } catch (e) {
    console.error('save config failed:', e)
  } finally {
    saving.value = false
  }
}

// 字段 key → label_i18n（用于必填校验提示）
function fieldLabel(key: string): string {
  return (
    schema.value?.fields.find((f) => f.key === key)?.label_i18n ??
    key
  )
}

// 把 ephemeral 字段（如 MySQL 初始化密码）从表单暂存到运行时 store（绝不持久化）
function syncInitPassword(data: FormData) {
  const schemaKeys = schema.value?.ephemeral_keys
  if (!schemaKeys || schemaKeys.length === 0) return
  for (const key of schemaKeys) {
    const v = data[key]
    const s = typeof v === 'string' ? v : ''
    // 仅在有值时暂存；空值不清除，避免用户仅修改其它字段（未动密码框）保存时误清已暂存的密码。
    // 已暂存的密码会在 server 起来（Running）后由 lifecycle store 自动清除。
    if (s) lifecycleStore.setInitPassword(props.software.id, s)
  }
}

async function loadBackups() {
  loadingBackups.value = true
  try {
    backups.value = await invoke('list_config_backups', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('load backups failed:', e)
  } finally {
    loadingBackups.value = false
  }
}

async function onRestore(backupName: string) {
  if (!confirm(t('confirmRestoreBackup'))) return
  try {
    await invoke('restore_config_backup', {
      installedId: props.software.id,
      backupName,
    })
    tab.value = 'form'
    dirty.value = false
    schema.value = await invoke('get_config_schema', { installedId: props.software.id })
  } catch (e) {
    console.error('restore failed:', e)
  }
}

async function onSaveAndRestart() {
  saving.value = true
  try {
    // 内联保存逻辑（而非调用 onSave），确保 saving 在整个保存+重启流程中保持 true
    if (tab.value === 'form' && formTabRef.value) {
      const data: FormData = (formTabRef.value as any).formData
      // 必填校验（field_rules.required）：缺失则中止并提示
      const missing = (formTabRef.value as any).validateRequired?.() ?? null
      if (missing) {
        alert(t('configRequiredMissing', { field: t(fieldLabel(missing)) }))
        return
      }
      await invoke('write_config_form', {
        installedId: props.software.id,
        data,
      })
      syncInitPassword(data)
    } else if (tab.value === 'source' && sourceTabRef.value) {
      const content =
        (sourceTabRef.value as any).getContent?.() ?? sourceContent
      await invoke('write_config_source', {
        installedId: props.software.id,
        content,
      })
    }
    dirty.value = false
    // 从运行时 store 取初始化密码，传给 restart_software（仅首次初始化消费一次）
    const initPw = lifecycleStore.getInitPassword(props.software.id)
    const args: Record<string, unknown> = { installedId: props.software.id }
    if (initPw) (args as any).initPassword = initPw
    await invoke('restart_software', args)
    emit('close')
  } catch (e) {
    console.error('saveAndRestart failed:', e)
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
.doc-btn {
  margin-left: 8px;
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
.preset-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  font-size: 13px;
}
.preset-label {
  color: var(--color-muted-foreground);
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
.btn-sm {
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
}

/* 备份 tab */
.backups-tab {
  min-height: 100px;
}
.backup-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 12px;
}
.backup-hint svg {
  width: 14px;
  height: 14px;
}
.backup-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.backup-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-muted);
}
.backup-info {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.backup-info svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
.backup-name {
  font-size: 12px;
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.backup-size {
  font-size: 11px;
  color: var(--color-muted-foreground);
  flex-shrink: 0;
}
</style>
