<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:dns-outline" :title="$t('dnsAccounts')" :subtitle="$t('dnsAccountsDesc')">
      <template #actions>
        <button class="btn primary" @click="openNew">
          <Icon icon="mdi:plus" /> {{ $t('newDnsAccount') }}
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
      v-if="!loading && accounts.length === 0"
      icon="mdi:dns-outline"
      :title="$t('noDnsAccounts')"
      :description="$t('noDnsAccountsDesc')"
    />

    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div
        v-for="a in accounts"
        :key="a.id"
        class="rounded-lg border border-border bg-card p-4 shadow-card"
      >
        <div class="flex items-center justify-between mb-2">
          <div class="font-semibold flex items-center gap-2 min-w-0">
            <Icon icon="mdi:dns" class="text-primary" />
            <span class="truncate">{{ a.name }}</span>
          </div>
          <span class="text-xs px-2 py-0.5 rounded-full bg-sky-100 text-sky-700 flex-shrink-0">
            {{ a.provider }}
          </span>
        </div>

        <div class="text-xs mb-2" :class="a.tested_at ? 'text-muted-foreground' : 'text-amber-600'">
          <template v-if="a.tested_at">
            {{ $t('testedAt', { time: fmtTime(a.tested_at) }) }}
            <template v-if="a.zones.length">
              · {{ $t('testOkZones', { n: a.zones.length }) }}
            </template>
          </template>
          <template v-else>{{ $t('notTested') }}</template>
        </div>

        <div v-if="a.zones.length" class="mb-3">
          <div class="text-[11px] text-muted-foreground mb-1">{{ $t('zonesCached') }}</div>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="z in a.zones"
              :key="z"
              class="text-[11px] font-mono px-1.5 py-0.5 rounded bg-muted text-foreground"
            >
              {{ z }}
            </span>
          </div>
        </div>

        <div class="flex gap-2 flex-wrap">
          <button class="btn" @click="openEdit(a)">
            <Icon icon="mdi:pencil" /> {{ $t('edit') }}
          </button>
          <button class="btn danger" @click="delTarget = a">
            <Icon icon="mdi:delete" /> {{ $t('delete') }}
          </button>
        </div>
      </div>
    </div>

    <DnsAccountDialog
      v-if="editing"
      :account="editing"
      :is-new="isNew"
      @close="onSaved"
      @saved="onSaved"
    />

    <Teleport to="body">
      <div v-if="delTarget" class="overlay" @click.self="delTarget = null">
        <div class="confirm-box">
          <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /> {{ $t('confirmDelete') }}</div>
          <p class="confirm-msg">{{ $t('confirmDeleteDnsAccount', { name: delTarget.name }) }}</p>
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
import { invoke } from '@/utils/ipc'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import DnsAccountDialog from '../components/DnsAccountDialog.vue'
import { emptyAccount, type DnsAccount } from '@/models/dns-account'

const accounts = ref<DnsAccount[]>([])
const loading = ref(false)
const editing = ref<DnsAccount | null>(null)
const isNew = ref(false)
const delTarget = ref<DnsAccount | null>(null)
const pageError = ref('')

function fmtTime(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString()
}

async function load() {
  loading.value = true
  try {
    accounts.value = await invoke<DnsAccount[]>('list_dns_accounts')
  } catch (e) {
    pageError.value = String(e)
  } finally {
    loading.value = false
  }
}

function openNew() {
  isNew.value = true
  editing.value = emptyAccount()
}
function openEdit(a: DnsAccount) {
  isNew.value = false
  editing.value = a
}
function onSaved() {
  editing.value = null
  load()
}

async function doDelete() {
  if (!delTarget.value) return
  const a = delTarget.value
  delTarget.value = null
  pageError.value = ''
  try {
    await invoke('delete_dns_account', { id: a.id })
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