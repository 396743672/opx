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

    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div
        v-for="a in apps"
        :key="a.id"
        class="rounded-lg border border-border bg-card p-4 shadow-card"
        :class="{ 'border-red-500': a.status === 'error' }"
      >
        <div class="flex items-center justify-between mb-2">
          <div class="font-semibold flex items-center gap-2">
            <Icon icon="mdi:language-javascript" class="text-green-500" />
            {{ a.name }}
          </div>
          <span class="text-xs px-2 py-0.5 rounded-full font-medium" :class="statusClass(a.status)">
            {{ $t(statusLabel(a.status)) }}
          </span>
        </div>

        <div class="text-xs text-muted-foreground space-y-0.5 mb-3 font-mono">
          <div class="flex items-center gap-3">
            <span v-if="a.pid" class="flex items-center gap-1"><Icon icon="mdi:identifier" /> PID {{ a.pid }}</span>
            <span v-if="a.auto_start" class="flex items-center gap-1"><Icon icon="mdi:power" /> {{ $t('autoStartOnAppStart') }}</span>
          </div>
          <div>{{ a.entry_path }}</div>
          <div v-if="a.last_error" class="text-red-500">{{ a.last_error }}</div>
        </div>

        <div class="flex gap-2 flex-wrap">
          <button
            v-if="a.status !== 'running'"
            class="btn primary"
            :disabled="!!acting[a.id]"
            @click="start(a)"
          >
            <Icon icon="mdi:play" /> {{ $t('start') }}
          </button>
          <button v-else class="btn" :disabled="!!acting[a.id]" @click="stop(a)">
            <Icon icon="mdi:stop" /> {{ $t('stop') }}
          </button>
          <button class="btn" :disabled="!!acting[a.id] || a.status === 'running'" @click="openEdit(a)">
            <Icon icon="mdi:pencil" /> {{ $t('edit') }}
          </button>
          <button class="btn" @click="showLog(a)"><Icon icon="mdi:file-document-outline" /> {{ $t('viewLogs') }}</button>
          <button class="btn danger" :disabled="!!acting[a.id] || a.status === 'running'" @click="remove(a)">
            <Icon icon="mdi:delete" /> {{ $t('delete') }}
          </button>
        </div>
      </div>
    </div>

    <!-- 添加 / 编辑（仅可通过关闭按钮关闭） -->
    <div v-if="editTarget !== null" class="overlay">
      <div class="dialog">
        <div class="head">
          <b>{{ editTarget.id ? $t('editNodeApp') : $t('addNodeApp') }}</b>
          <button class="dialog-close" @click="editTarget = null"><Icon icon="mdi:close" /></button>
        </div>
        <div class="dialog-body">
        <div class="field">
          <label>{{ $t('nodeAppName') }}</label>
          <input v-model="form.name" class="input" :placeholder="$t('nodeAppName')" :title="$t('nodeAppNameHint')" />
        </div>
        <div class="field">
          <label>{{ $t('entryPath') }}</label>
          <div class="row">
            <input v-model="form.entry_path" class="input" :placeholder="$t('entryPath')" />
            <button class="btn" @click="browse"><Icon icon="mdi:folder-open" /> {{ $t('browse') }}</button>
          </div>
        </div>
        <div class="field">
          <label>{{ $t('nodeVersion') }}</label>
          <select v-model="form.node_installed_id" class="input">
            <option value="">{{ $t('autoSelect') }}</option>
            <option v-for="n in installedNodes" :key="n.id" :value="n.id">{{ n.name }} {{ n.version }}</option>
          </select>
        </div>
        <div class="field">
          <label>{{ $t('presets') }}</label>
          <div class="preset-row">
            <button v-for="p in PRESETS" :key="p.label" class="btn btn-sm" @click="applyPreset(p)">
              <Icon icon="mdi:auto-fix" /> {{ p.label }}
            </button>
          </div>
        </div>
        <div class="field">
          <label>{{ $t('startArgs') }}</label>
          <textarea v-model="form.argsText" class="input ta" rows="2" :placeholder="$t('programArgsHint')"></textarea>
        </div>
        <div class="field">
          <label>{{ $t('environmentVariables') }}</label>
          <textarea v-model="form.envText" class="input ta" rows="3" :placeholder="'PORT=3000\nDB_URL=mysql://...'"></textarea>
        </div>
        <div class="row-chk">
          <label class="chk"><input type="checkbox" v-model="form.auto_start" /> {{ $t('autoStartOnAppStart') }}</label>
          <label class="chk"><input type="checkbox" v-model="form.auto_restart" /> {{ $t('autoRestart') }}</label>
          <label class="fld"><span>{{ $t('startupOrder') }}</span><input v-model.number="form.startup_order" class="input num" type="number" /></label>
        </div>
        </div>
        <div class="foot">
          <button class="btn" @click="editTarget = null">{{ $t('cancel') }}</button>
          <button class="btn primary" :disabled="saving" @click="save">{{ $t('save') }}</button>
        </div>
      </div>
    </div>

    <!-- 日志查看（仅可通过关闭按钮关闭） -->
    <div v-if="logApp" class="overlay">
      <div class="dialog log-dialog">
        <div class="head"><b>{{ $t('logs') }} - {{ logApp.name }}</b><button class="dialog-close" @click="logApp = null"><Icon icon="mdi:close" /></button></div>
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
import { invoke } from '@/utils/ipc'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import type { NodeApp } from '@/models/node-app'
import type { InstalledSoftware } from '@/models/software'

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
  node_installed_id: '',
  argsText: '',
  envText: '',
  auto_start: false,
  startup_order: 0,
  auto_restart: false,
})
const installedNodes = ref<InstalledSoftware[]>([])
const entryChanged = ref(false)
const acting = ref<Record<string, 'start' | 'stop'>>({})

// 预设模板：一键填充常用启动参数（入口仍由用户选择）
const PRESETS = [
  { label: 'HTTP 服务', args: '--port 3000' },
  { label: 'Express 服务', args: '--port 3000' },
  { label: 'TS 即时运行', args: '--import tsx' },
]
function applyPreset(p: (typeof PRESETS)[number]) {
  form.value.argsText = (form.value.argsText ? form.value.argsText + '\n' : '') + p.args
}
let timer: ReturnType<typeof setInterval> | null = null
let unlistenAutoRestartGiveUp: UnlistenFn | null = null

async function load() {
  loading.value = true
  try {
    apps.value = await invoke<NodeApp[]>('list_node_apps')
    const list = await invoke<InstalledSoftware[]>('list_installed_software')
    installedNodes.value = list.filter((s) => s.key === 'node')
  } catch (e: any) {
    console.error('load node apps failed:', e)
  } finally {
    loading.value = false
  }
}

async function start(a: NodeApp) {
  acting.value[a.id] = 'start'
  try {
    await invoke('start_node_app', { id: a.id })
  } catch (e: any) {
    alert(String(e))
  } finally {
    delete acting.value[a.id]
    await load()
  }
}
async function stop(a: NodeApp) {
  acting.value[a.id] = 'stop'
  try {
    await invoke('stop_node_app', { id: a.id })
  } catch (e: any) {
    alert(String(e))
  } finally {
    delete acting.value[a.id]
    await load()
  }
}
async function remove(a: NodeApp) {
  if (!confirm(t('confirmDeleteNodeApp') + `「${a.name}」？`)) return
  await invoke('delete_node_app', { id: a.id })
  await load()
}

function openEdit(a?: NodeApp) {
  entryChanged.value = false
  editTarget.value = a ?? {
    id: '',
    name: '',
    entry_path: '',
    node_installed_id: '',
    args: [],
    env_vars: [],
    auto_start: false,
    startup_order: 0,
    auto_restart: false,
    status: 'stopped',
    pid: null,
    last_error: null,
    log_path: '',
  }
  form.value = a
    ? {
        name: a.name,
        entry_path: a.entry_path,
        node_installed_id: a.node_installed_id,
        argsText: a.args.join('\n'),
        envText: a.env_vars.map(([k, v]) => `${k}=${v}`).join('\n'),
        auto_start: a.auto_start,
        startup_order: a.startup_order,
        auto_restart: a.auto_restart,
      }
    : { name: '', entry_path: '', node_installed_id: '', argsText: '', envText: '', auto_start: false, startup_order: 0, auto_restart: false }
}

async function browse() {
  const r = await open({ filters: [{ name: 'JS', extensions: ['js', 'mjs', 'cjs', 'ts'] }] })
  if (typeof r === 'string') {
    // 编辑已有应用并更换入口：先提示新文件将替换运行目录中的历史文件
    if (editTarget.value?.id && !confirm(t('replaceEntryConfirm'))) return
    form.value.entry_path = r
    entryChanged.value = true
  }
}

async function save() {
  if (!form.value.name.trim() || !form.value.entry_path.trim()) {
    alert(t('configRequiredMissing', { field: t('nodeAppName') }))
    return
  }
  saving.value = true
  try {
    const env_vars: [string, string][] = form.value.envText
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const idx = line.indexOf('=')
        return idx > 0
          ? ([line.slice(0, idx).trim(), line.slice(idx + 1).trim()] as [string, string])
          : null
      })
      .filter((x): x is [string, string] => x !== null)
    const base = {
      name: form.value.name.trim(),
      node_installed_id: form.value.node_installed_id || '',
      args: form.value.argsText.split('\n').map((s) => s.trim()).filter(Boolean),
      env_vars,
      auto_start: form.value.auto_start,
      startup_order: form.value.startup_order,
      auto_restart: form.value.auto_restart,
    }
    if (editTarget.value?.id) {
      await invoke('update_node_app', {
        id: editTarget.value.id,
        params: { ...base, entry_path: entryChanged.value ? form.value.entry_path.trim() : null },
      })
    } else {
      await invoke('add_node_app', { params: { ...base, entry_path: form.value.entry_path.trim() } })
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

function statusClass(s: string): string {
  const map: Record<string, string> = {
    running: 'bg-green-100 text-green-700',
    stopped: 'bg-muted text-muted-foreground',
    error: 'bg-red-100 text-red-700',
    starting: 'bg-amber-100 text-amber-700',
    stopping: 'bg-amber-100 text-amber-700',
  }
  return map[s] ?? 'bg-muted text-muted-foreground'
}
function statusLabel(s: string): string {
  const map: Record<string, string> = {
    running: 'running',
    stopped: 'stopped',
    error: 'error',
    starting: 'starting',
    stopping: 'stopping',
  }
  return map[s] ?? 'unknown'
}

onMounted(async () => {
  load()
  timer = setInterval(load, 3000)
  unlistenAutoRestartGiveUp = await listen<{ kind: string }>('auto-restart-giveup', (e) => {
    if (e.payload?.kind === 'node') load()
  })
})
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
  unlistenAutoRestartGiveUp?.()
})
</script>

<style scoped>
.page { padding: 8px 0; }
.placeholder { padding: 32px; text-align: center; color: var(--color-muted-foreground); }
.empty { padding: 48px; text-align: center; color: var(--color-muted-foreground); }
.empty-icon { font-size: 40px; opacity: 0.5; }
.empty p { margin: 8px 0 16px; }
.overlay { position: fixed; inset: 0; z-index: 50; display: flex; align-items: center; justify-content: center; background: oklch(0 0 0 / 0.4); }
.dialog { width: 460px; max-width: 92vw; max-height: 85vh; border-radius: 10px; border: 1px solid var(--color-border); background: var(--color-card); padding: 16px; display: flex; flex-direction: column; overflow: hidden; }
.log-dialog { width: 720px; }
.head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; flex: none; }

.field { margin-bottom: 10px; }
.dialog-body { flex: 1 1 auto; min-height: 0; overflow-y: auto; }
.field label, .fld span { display: block; font-size: 12px; color: var(--color-muted-foreground); margin-bottom: 4px; }
.row { display: flex; gap: 6px; }
.preset-row { display: flex; gap: 6px; flex-wrap: wrap; }
.input { width: 100%; height: 32px; padding: 0 10px; background: var(--color-muted); border: 1px solid transparent; border-radius: 6px; color: var(--color-foreground); font-size: 13px; outline: none; }
.input.ta { height: auto; padding: 6px 10px; font-family: ui-monospace, monospace; font-size: 12px; }
.input.num { width: 72px; }
.input:focus { border-color: var(--color-primary); background: var(--color-card); }
.row-chk { display: flex; align-items: center; gap: 16px; margin: 12px 0; }
.chk { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--color-muted-foreground); cursor: pointer; }
.fld { display: inline-flex; align-items: center; gap: 6px; }
.fld span { margin: 0; }
.foot { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; padding-top: 12px; border-top: 1px solid var(--color-border); flex: none; }
.btn { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 12px; border-radius: 6px; cursor: pointer; font-size: 13px; border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); white-space: nowrap; }
.btn:hover { background: var(--color-muted); }
.btn.primary { background: var(--color-primary); color: var(--color-primary-foreground); border-color: var(--color-primary); }
.btn.primary:hover { background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background)); }
.btn.danger { background: var(--color-destructive); color: white; border-color: var(--color-destructive); }
.btn.danger:hover { opacity: 0.9; }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn-sm { height: 26px; padding: 0 9px; font-size: 12px; }
.logbox { background: #0d1117; color: #58a6ff; font-family: ui-monospace, monospace; font-size: 12px; padding: 12px; overflow-y: auto; max-height: 55vh; border-radius: 6px; }
.logbox pre { margin: 0; white-space: pre-wrap; word-break: break-all; line-height: 1.4; }
.empty-hint { color: var(--color-muted-foreground); text-align: center; padding: 24px 0; }
</style>