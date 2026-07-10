<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:web-box" :title="$t('websiteManagement')" :subtitle="$t('websiteList')">
      <template #actions>
        <button class="btn primary" @click="openNew">
          <Icon icon="mdi:plus" /> {{ $t('newSite') }}
        </button>
      </template>
    </PageHeader>

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
          <span
            class="text-xs px-2 py-0.5 rounded-full"
            :class="s.enabled ? 'bg-green-100 text-green-700' : 'bg-muted text-muted-foreground'"
          >
            {{ s.enabled ? $t('running') : $t('stopped') }}
          </span>
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
        <div class="flex gap-2">
          <button class="btn" @click="openEdit(s)"><Icon icon="mdi:pencil" /> {{ $t('editSite') }}</button>
          <button class="btn" @click="toggle(s)">
            {{ s.enabled ? $t('stopped') : $t('running') }}
          </button>
          <button class="btn danger" @click="remove(s)"><Icon icon="mdi:delete" /></button>
        </div>
      </div>
    </div>

    <SiteEditDialog v-if="editing" :site="editing" @close="editing = null" @saved="onSaved" />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SiteEditDialog from '../components/SiteEditDialog.vue'
import { emptySite, type Site } from '@/models/website'

const sites = ref<Site[]>([])
const loading = ref(false)
const editing = ref<Site | null>(null)

function addr(s: Site): string {
  return s.server_name && s.server_name.trim() ? `${s.server_name}:${s.listen}` : `:${s.listen}`
}

async function load() {
  loading.value = true
  try {
    sites.value = await invoke<Site[]>('list_websites')
  } catch (e) {
    console.error(e)
  } finally {
    loading.value = false
  }
}

function openNew() {
  editing.value = emptySite()
}
function openEdit(s: Site) {
  editing.value = JSON.parse(JSON.stringify(s))
}
function onSaved() {
  editing.value = null
  load()
}

async function toggle(s: Site) {
  try {
    await invoke('set_website_enabled', { id: s.id, enabled: !s.enabled })
    load()
  } catch (e) {
    window.alert(String(e))
  }
}

async function remove(s: Site) {
  if (!confirm(`删除站点「${s.name}」？`)) return
  try {
    await invoke('delete_website', { id: s.id })
    load()
  } catch (e) {
    window.alert(String(e))
  }
}

onMounted(load)
</script>
