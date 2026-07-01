<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:package-variant-closed"
      :title="$t('softwareManagement')"
      :subtitle="$t('installedSoftware')"
    >
      <template #actions>
        <button class="btn" @click="loadInstalled" :disabled="loading">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
      </template>
    </PageHeader>

    <div v-if="loading && installed.length === 0">
      <EmptyState icon="mdi:loading" :title="$t('loading')" :description="''" />
    </div>

    <div v-else-if="installed.length === 0">
      <EmptyState
        icon="mdi:package-variant-closed"
        :title="$t('noInstalledSoftware')"
        :description="$t('noInstalledSoftwareDesc')"
      />
    </div>

    <div v-else class="content">
      <div class="installed-list">
        <div v-for="item in installed" :key="item.id" class="installed-row">
          <div class="row-icon">
            <Icon :icon="sourceIcon(item)" />
          </div>
          <div class="row-main">
            <div class="row-name">
              {{ item.name }}
              <span v-if="item.is_custom" class="tag custom">{{ $t('custom') }}</span>
              <span v-else-if="isBuiltin(item)" class="tag builtin">{{ $t('offline') }}</span>
            </div>
            <div class="row-path mono">{{ item.install_path }}</div>
          </div>
          <div class="row-source">{{ sourceLabel(item) }}</div>
          <button
            class="btn danger small"
            @click="openUninstall(item)"
            :disabled="uninstallingId === item.id"
          >
            <Icon icon="mdi:delete" />
            <span v-if="uninstallingId === item.id">{{ $t('uninstalling') }}</span>
            <span v-else>{{ $t('uninstall') }}</span>
          </button>
        </div>
      </div>
    </div>

    <UninstallConfirmDialog
      v-if="selectedSoftware && showConfirmDialog"
      :software="selectedSoftware"
      :uninstalling="uninstallingId === selectedSoftware.id"
      @confirm="confirmUninstall"
      @cancel="cancelUninstall"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import UninstallConfirmDialog from '../components/UninstallConfirmDialog.vue'
import type { InstalledSoftware } from '@/models/software'

const installed = ref<InstalledSoftware[]>([])
const loading = ref(false)
const selectedSoftware = ref<InstalledSoftware | null>(null)
const showConfirmDialog = ref(false)
const uninstallingId = ref<string | null>(null)

function isBuiltin(item: InstalledSoftware): boolean {
  return 'Builtin' in item.source
}

function sourceIcon(item: InstalledSoftware): string {
  if (item.is_custom) return 'mdi:upload'
  if (isBuiltin(item)) return 'mdi:package-variant-closed'
  return 'mdi:download'
}

function sourceLabel(item: InstalledSoftware): string {
  if (item.is_custom) return 'Custom'
  if (isBuiltin(item)) return 'Builtin'
  return 'Mirror'
}

async function loadInstalled() {
  loading.value = true
  try {
    installed.value = await invoke('list_installed_software') as InstalledSoftware[]
  } catch (e) {
    console.error('Failed to load installed:', e)
  } finally {
    loading.value = false
  }
}

function openUninstall(item: InstalledSoftware) {
  selectedSoftware.value = item
  showConfirmDialog.value = true
}

function cancelUninstall() {
  showConfirmDialog.value = false
  selectedSoftware.value = null
}

async function confirmUninstall() {
  if (!selectedSoftware.value) return
  const id = selectedSoftware.value.id
  uninstallingId.value = id
  try {
    await invoke('uninstall_software', { installedId: id })
    showConfirmDialog.value = false
    selectedSoftware.value = null
    await loadInstalled()
  } catch (e) {
    console.error('Failed to uninstall:', e)
  } finally {
    uninstallingId.value = null
  }
}

onMounted(() => {
  loadInstalled()
})
</script>

<style scoped>
.content {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.installed-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.installed-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 16px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
}
.row-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
  font-size: 20px;
}
.row-main {
  flex: 1;
  min-width: 0;
}
.row-name {
  font-size: 14px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.row-path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
}
.tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
}
.tag.custom {
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
}
.tag.builtin {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.row-source {
  font-size: 11px;
  color: var(--color-muted-foreground);
  flex-shrink: 0;
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
.btn.small {
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
}
.btn.danger {
  background: var(--color-destructive);
  color: white;
  border-color: var(--color-destructive);
}
.btn.danger:hover {
  background: color-mix(in oklch, var(--color-destructive) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
