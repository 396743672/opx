<template>
  <div class="page">
    <PageHeader
      icon="mdi:language-javascript"
      :title="$t('nodeApps')"
      :subtitle="$t('nodeAppsDesc')"
    >
      <template #actions>
        <button class="btn primary" @click="openEdit()">
          <Icon icon="mdi:plus" /> {{ $t('addNodeApp') }}
        </button>
      </template>
    </PageHeader>

    <div v-if="loading && apps.length === 0" class="placeholder">{{ $t('loading') }}</div>

    <div v-else-if="apps.length === 0" class="empty">
      <Icon icon="mdi:language-javascript" class="empty-icon" />
      <p>{{ $t('noNodeApps') }}</p>
      <button class="btn primary" @click="openEdit()">
        <Icon icon="mdi:plus" /> {{ $t('addNodeApp') }}
      </button>
    </div>

    <div v-else class="grid">
      <div
        v-for="a in apps"
        :key="a.id"
        class="card"
        :class="{ err: a.status === 'error' }"
      >
        <div class="card-top">
          <div class="name">{{ a.name }}</div>
          <StatusBadge :status="nodeStatus(a.status)" :error="a.last_error" />
        </div>
        <div class="entry mono">{{ a.entry_path }}</div>
        <div class="meta">
          <span v-if="a.pid" class="kv"><Icon icon="mdi:identifier" /> PID {{ a.pid }}</span>
          <span v-if="a.auto_start" class="kv"><Icon icon="mdi:power" /> {{ $t('autoStartOnAppStart') }}</span>
        </div>
        <div class="actions">
          <button class="btn btn-sm" :disabled="a.status === 'running'" @click="start(a)">{{ $t('start') }}</button>
          <button class="btn btn-sm" :disabled="a.status !== 'running'" @click="stop(a)">{{ $t('stop') }}</button>
          <button class="btn btn-sm" @click="openEdit(a)">{{ $t('edit') }}</button>
          <button class="btn btn-sm" @click="showLog(a)"><Icon icon="mdi:file-document-outline" /> {{ $t('logs') }}</button>
          <button class="btn btn-sm danger" @click="remove(a)">{{ $t('delete') }}</button>
        </div>
      </div>
    </div>

    <!-- 添加 / 编辑 -->
    <div v-if="editTarget !== null" class="overlay" @click.self="editTarget = null">
      <div class="dialog">
        <div class="head">
          <b>{{ editTarget.id ? $t('editNodeApp') : $t('addNodeApp') }}</b>
          <button class="close" @click="editTarget = null"><Icon icon="mdi:close" /></button>
        </div>
        <div class="field">
          <label>{{ $t('nodeAppName') }}</label>
          <input v-model="form.name" class="input" :placeholder="'my-service'" />
        </div>
        <div class="field">
          <label>{{ $t('entryPath') }}</label>
          <div class="row">
            <input v-model="form.entry_path" class="input" :placeholder="$t('jarPath')" />
            <button class="btn" @click="browse"><Icon icon="mdi:folder-open" /> {{ $t('browse') }}</button>
          </div>
        </div>
        <div class="field">
          <label>{{ $t('startArgs') }}</label>
          <textarea v-model="form.argsText" class="input ta" rows="2" :placeholder="$t('programArgsHint')"></textarea>
        </div>
        <div class="row-chk">
          <label class="chk"><input type="checkbox" v-model="form.auto_start" /> {{ $t('autoStartOnAppStart') }}</label>
          <label class="fld"><span>{{ $t('startupOrder') }}</span><input v-model.number="form.startup_order" class="input num" type="number" /></label>
        </div>
        <div class="foot">
          <button class="btn" @click="editTarget = null">{{ $t('cancel') }}</button>
          <button class="btn primary" :disabled="saving" @click="save">{{ $t('save') }}</button>
        </div>
      </div>
    </div>

    <!-- 日志查看 -->
    <div v-if="logApp" class="overlay" @click.self="logApp = null">
      <div class="dialog log-dialog">
        <div class="head"><b>{{ $t('logs') }} - {{ logApp.name }}</b><button class="close" @click="logApp = null"><Icon icon="mdi:close" /></button></div>
        <div class="logbox">
          <pre v-for="(l, i) in logLines" :key="i">{{ l }}</pre>
          <div v-if="!logLines.length" class="empty-hint">{{ $t('noLogs') }}</div>
        </div>
        <div class="foot"><button class="btn" @click="logApp = null">{{ $t('close') }}</button></div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/modules/software-manager/components/StatusBadge.vue'
import type { NodeApp, NodeAppStatus } from '@/models/node-app'
import { SoftwareStatus } from '@/models/software'

const { t } = useI18n()

const apps = ref<NodeApp[]>([])
const loading = ref(false)
const saving = ref(false)
const editTarget = ref<NodeApp | null>(null)
const logApp = ref<NodeApp | null>(null)
const logLines = ref<string[]>([])
const form = ref({
  name: '',
  entry_path: '',
  argsText: '',
  auto_start: false,
  startup_order: 0,
})
let timer: ReturnType<typeof setInterval> | null = null

async function load() {
  loading.value = true
  try {
    apps.value = await invoke<NodeApp[]>('list_node_apps')
  } catch (e: any) {
    console.error('load node apps failed:', e)
  } finally {
    loading.value = false
  }
}

async function start(a: NodeApp) {
  try {
    await invoke('start_node_app', { id: a.id })
  } catch (e: any) {
    alert(String(e))
  }
  await load()
}
async function stop(a: NodeApp) {
  await invoke('stop_node_app', { id: a.id })
  await load()
}
async function remove(a: NodeApp) {
  if (!confirm(t('confirmDeleteNodeApp') + `「${a.name}」？`)) return
  await invoke('delete_node_app', { id: a.id })
  await load()
}

function openEdit(a?: NodeApp) {
  editTarget.value = a ?? {
    id: '',
    name: '',
    entry_path: '',
    args: [],
    env_vars: [],
    auto_start: false,
    startup_order: 0,
    status: 'stopped',
    pid: null,
    last_error: null,
    log_path: '',
  }
  form.value = a
    ? {
        name: a.name,
        entry_path: a.entry_path,
        argsText: a.args.join('\n'),
        auto_start: a.auto_start,
        startup_order: a.startup_order,
      }
    : { name: '', entry_path: '', argsText: '', auto_start: false, startup_order: 0 }
}

async function browse() {
  const r = await open({ filters: [{ name: 'JS', extensions: ['js', 'mjs', 'cjs', 'ts'] }] })
  if (typeof r === 'string') form.value.entry_path = r
}

async function save() {
  if (!form.value.name.trim() || !form.value.entry_path.trim()) {
    alert(t('configRequiredMissing', { field: t('nodeAppName') }))
    return
  }
  saving.value = true
  try {
    const params = {
      name: form.value.name.trim(),
      entry_path: form.value.entry_path.trim(),
      args: form.value.argsText.split('\n').map((s) => s.trim()).filter(Boolean),
      auto_start: form.value.auto_start,
      startup_order: form.value.startup_order,
    }
    if (editTarget.value?.id) {
      await invoke('update_node_app', { id: editTarget.value.id, params })
    } else {
      await invoke('add_node_app', { params })
    }
    editTarget.value = null
    await load()
  } catch (e: any) {
    alert(String(e))
  } finally {
    saving.value = false
  }
}

async function showLog(a: NodeApp) {
  logApp.value = a
  logLines.value = await invoke<string[]>('read_node_app_log', { id: a.id })
}

function nodeStatus(s: NodeAppStatus): SoftwareStatus {
  if (s === 'running') return SoftwareStatus.Running
  if (s === 'error') return SoftwareStatus.Error
  return SoftwareStatus.Stopped
}

onMounted(() => {
  load()
  timer = setInterval(load, 3000)
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<style scoped>
.page { padding: 8px 0; }
.placeholder { padding: 32px; text-align: center; color: var(--color-muted-foreground); }
.empty { padding: 48px; text-align: center; color: var(--color-muted-foreground); }
.empty-icon { font-size: 40px; opacity: 0.5; }
.empty p { margin: 8px 0 16px; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
.card { border: 1px solid var(--color-border); border-radius: 10px; padding: 14px; background: var(--color-card); }
.card.err { border-color: var(--color-danger, red); }
.card-top { display: flex; justify-content: space-between; align-items: center; gap: 8px; margin-bottom: 8px; }
.name { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.entry { font-size: 12px; color: var(--color-muted-foreground); word-break: break-all; margin-bottom: 6px; }
.meta { display: flex; gap: 12px; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 10px; }
.kv { display: inline-flex; align-items: center; gap: 4px; }
.actions { display: flex; gap: 6px; flex-wrap: wrap; }
.overlay { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: oklch(0 0 0 / 0.4); }
.dialog { width: 460px; max-width: 92vw; max-height: 85vh; overflow-y: auto; border-radius: 10px; border: 1px solid var(--color-border); background: var(--color-card); padding: 16px; }
.log-dialog { width: 720px; }
.head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
.close { width: 28px; height: 28px; border: none; background: transparent; color: var(--color-muted-foreground); border-radius: 4px; cursor: pointer; }
.close:hover { background: var(--color-muted); }
.field { margin-bottom: 10px; }
.field label, .fld span { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.row { display: flex; gap: 6px; }
.input { width: 100%; height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; }
.input.ta { height: auto; padding: 6px 10px; font-family: ui-monospace, monospace; font-size: 12px; }
.input.num { width: 72px; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.row-chk { display: flex; align-items: center; gap: 16px; margin: 12px 0; }
.chk { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--color-muted-foreground); cursor: pointer; }
.fld { display: inline-flex; align-items: center; gap: 6px; }
.fld span { margin: 0; }
.foot { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; padding-top: 12px; border-top: 1px solid var(--color-border); }
.btn { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 12px; border-radius: 6px; cursor: pointer; font-size: 13px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); }
.btn:hover { background: var(--color-muted); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn.danger { color: var(--color-danger, red); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn-sm { height: 26px; padding: 0 9px; font-size: 12px; }
.logbox { background: #0d1117; color: #58a6ff; font-family: ui-monospace, monospace; font-size: 12px; padding: 12px; overflow-y: auto; max-height: 55vh; border-radius: 6px; }
.logbox pre { margin: 0; white-space: pre-wrap; word-break: break-all; line-height: 1.4; }
.empty-hint { color: var(--color-muted-foreground); text-align: center; padding: 24px 0; }
</style>