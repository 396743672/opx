<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:web-box" :title="$t('websiteManagement')" :subtitle="$t('websiteList')">
      <template #actions>
        <button class="btn primary" @click="openNew">
          <Icon icon="mdi:web-plus" /> {{ $t('newSite') }}
        </button>
      </template>
    </PageHeader>

    <Teleport to="body">
      <div v-if="pageError" class="overlay" @click.self="pageError = ''">
        <div class="confirm-box">
          <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
          <p class="confirm-msg">{{ pageError }}</p>
          <div class="confirm-actions">
            <button class="btn primary" @click="pageError = ''">{{ $t('confirm') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <EmptyState
      v-if="!loading && sites.length === 0"
      icon="mdi:web-box"
      :title="$t('noSites')"
      :description="$t('noSitesDesc')"
    />

    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div
        v-for="s in sites"
        :key="s.id"
        class="rounded-lg border border-border bg-card p-4 shadow-card"
      >
        <div class="flex items-center justify-between mb-2">
          <div class="font-semibold flex items-center gap-2">
            <Icon icon="mdi:web" class="text-primary" /> {{ s.name }}
          </div>
          <div class="flex items-center gap-1.5">
            <span
              v-if="s.custom_conf"
              class="text-xs px-2 py-0.5 rounded-full bg-purple-100 text-purple-700 inline-flex items-center gap-1"
            >
              <Icon icon="mdi:code-tags" /> {{ $t('customConfBadge') }}
            </span>
            <span
              class="text-xs px-2 py-0.5 rounded-full"
              :class="s.enabled && nginxRunning ? 'bg-green-100 text-green-700' : 'bg-muted text-muted-foreground'"
              :title="s.enabled && !nginxRunning ? $t('nginxNotRunning') : ''"
            >
              {{ s.enabled && nginxRunning ? $t('running') : $t('stopped') }}
            </span>
          </div>
        </div>
        <div class="text-sm font-mono text-sky-600 mb-2">{{ addr(s) }}</div>
        <div class="flex flex-wrap gap-1 mb-3">
          <span
            v-for="(l, i) in s.locations"
            :key="i"
            class="text-[11px] font-mono px-1.5 py-0.5 rounded"
            :class="l.kind === 'Proxy' ? 'bg-amber-100 text-amber-800' : 'bg-indigo-50 text-indigo-700'"
          >
            {{ l.path }} {{ l.kind === 'Proxy' ? '→ ' + (l.target || '') : $t('typeStatic') }}
          </span>
        </div>
        <div class="flex gap-2 flex-wrap">
          <button class="btn" :disabled="s.enabled" :title="s.enabled ? $t('runningSiteConfigDisabled') : ''" @click="openEdit(s)">
            <Icon icon="mdi:pencil" /> {{ $t('editSite') }}
          </button>
          <button class="btn" :class="s.enabled ? 'primary' : ''" @click="toggle(s)">
            <Icon :icon="s.enabled ? 'mdi:stop' : 'mdi:play'" /> {{ s.enabled ? $t('stop') : $t('start') }}
          </button>
          <button class="btn danger" :disabled="s.enabled && nginxRunning" :title="s.enabled && nginxRunning ? $t('deleteRunningHint') : ''" @click="remove(s)">
            <Icon icon="mdi:delete" /> {{ $t('delete') }}
          </button>
        </div>
      </div>
    </div>

    <SiteEditDialog v-if="editing" :site="editing" :is-new="isNew" @close="editing = null" @saved="onSaved" />

    <Teleport to="body">
      <div v-if="delTarget" class="overlay" @click.self="delTarget = null">
        <div class="confirm-box">
          <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /> {{ $t('confirmDelete') }}</div>
          <p class="confirm-msg">{{ $t('confirmDeleteSite', { name: delTarget.name }) }}</p>
          <div class="confirm-actions">
            <button class="btn" @click="delTarget = null">{{ $t('cancel') }}</button>
            <button class="btn danger" @click="doDelete">{{ $t('delete') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SiteEditDialog from '../components/SiteEditDialog.vue'
import { emptySite, type Site } from '@/models/website'

const { t } = useI18n()
const sites = ref<Site[]>([])
const loading = ref(false)
const editing = ref<Site | null>(null)
const isNew = ref(false)
const nginxRunning = ref(false)
const delTarget = ref<Site | null>(null)
const pageError = ref('')

function addr(s: Site): string {
  return s.server_name && s.server_name.trim() ? `${s.server_name}:${s.listen}` : `:${s.listen}`
}

async function load() {
  loading.value = true
  try {
    sites.value = await invoke<Site[]>('list_websites')
    // 检查 nginx 是否在运行
    const installed = await invoke<any[]>('list_installed_software')
    const nginx = installed.find((s: any) => s.key === 'nginx')
    nginxRunning.value = nginx?.status === 'Running'
  } catch (e) {
    console.error(e)
  } finally {
    loading.value = false
  }
}

function openNew() {
  isNew.value = true
  editing.value = emptySite()
}
function openEdit(s: Site) {
  isNew.value = false
  editing.value = JSON.parse(JSON.stringify(s))
}
function onSaved() {
  editing.value = null
  load()
}

async function toggle(s: Site) {
  // 若启用站点但 nginx 未运行，先提示
  if (!s.enabled && !nginxRunning.value) {
    pageError.value = t('nginxNotRunning')
    return
  }
  pageError.value = ''
  try {
    await invoke('set_website_enabled', { id: s.id, enabled: !s.enabled })
    load()
  } catch (e) {
    pageError.value = String(e)
  }
}

function remove(s: Site) {
  delTarget.value = s
}

async function doDelete() {
  if (!delTarget.value) return
  const s = delTarget.value
  delTarget.value = null
  pageError.value = ''
  try {
    await invoke('delete_website', { id: s.id })
    load()
  } catch (e) {
    pageError.value = String(e)
  }
}

onMounted(load)
</script>

<style scoped>
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
.btn.danger {
  background: var(--color-destructive);
  color: white;
  border-color: var(--color-destructive);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn svg {
  width: 16px;
  height: 16px;
}
.overlay {
  position: fixed; inset: 0; z-index: 60;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5); backdrop-filter: blur(4px);
}
.confirm-box {
  width: 380px; padding: 24px;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
}
.confirm-title {
  font-size: 15px; font-weight: 600; display: flex; align-items: center; gap: 8px; margin-bottom: 12px;
}
.confirm-title svg { color: var(--color-destructive); }
.confirm-msg { font-size: 13px; color: var(--color-muted-foreground); margin-bottom: 20px; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
