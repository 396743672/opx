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
      <div v-for="group in grouped" :key="group.category" class="category-section">
        <div class="category-title">
          <Icon :icon="group.icon" />
          {{ $t(group.label) }}
          <span class="count">{{ group.items.length }}</span>
        </div>
        <div class="instance-list">
          <SoftwareInstanceRow
            v-for="item in group.items"
            :key="item.id"
            :software="mergeStatus(item)"
            @start="onStart(item)"
            @stop="onStop(item)"
            @config="onConfig(item)"
            @startup-settings="onStartupSettings(item)"
            @uninstall="onUninstall(item)"
          />
        </div>
      </div>
    </div>

    <ConfigEditDialog
      v-if="configTarget"
      :software="configTarget"
      @close="configTarget = null"
    />
    <StartupSettingsDialog
      v-if="startupTarget"
      :software="startupTarget"
      @close="startupTarget = null"
    />
    <CustomStartCommandDialog
      v-if="customTarget"
      :software="customTarget"
      @close="customTarget = null"
    />
    <UninstallBlockedDialog
      v-if="uninstallTarget"
      :software="uninstallTarget"
      @close="uninstallTarget = null"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SoftwareInstanceRow from '../components/SoftwareInstanceRow.vue'
import ConfigEditDialog from '../components/ConfigEditDialog.vue'
import StartupSettingsDialog from '../components/StartupSettingsDialog.vue'
import CustomStartCommandDialog from '../components/CustomStartCommandDialog.vue'
import UninstallBlockedDialog from '../components/UninstallBlockedDialog.vue'
import { useLifecycleStore } from '../stores/lifecycle'
import type { InstalledSoftware, SoftwareStatus } from '@/models/software'

const lifecycleStore = useLifecycleStore()

const installed = ref<InstalledSoftware[]>([])
const loading = ref(false)
const configTarget = ref<InstalledSoftware | null>(null)
const startupTarget = ref<InstalledSoftware | null>(null)
const customTarget = ref<InstalledSoftware | null>(null)
const uninstallTarget = ref<InstalledSoftware | null>(null)
let pollTimer: ReturnType<typeof setInterval> | null = null

interface Group {
  category: string
  label: string
  icon: string
  items: InstalledSoftware[]
}

const grouped = computed<Group[]>(() => {
  const groups: Record<string, Group> = {
    database: { category: 'database', label: 'database', icon: 'mdi:database', items: [] },
    cache: { category: 'cache', label: 'cache', icon: 'mdi:lightning-bolt', items: [] },
    webserver: { category: 'webserver', label: 'webServer', icon: 'mdi:web', items: [] },
    storage: { category: 'storage', label: 'objectStorage', icon: 'mdi:storage', items: [] },
    custom: { category: 'custom', label: 'custom', icon: 'mdi:upload', items: [] },
  }
  // JRE 不进管理页（独立展示在别处）
  const list = installed.value.filter((s) => s.key !== 'jre')
  list.sort((a, b) => a.key.localeCompare(b.key) || a.version.localeCompare(b.version))
  for (const sw of list) {
    let g: keyof typeof groups
    if (sw.is_custom) g = 'custom'
    else if (sw.key === 'mysql') g = 'database'
    else if (sw.key === 'redis') g = 'cache'
    else if (sw.key === 'nginx') g = 'webserver'
    else if (sw.key === 'minio' || sw.key === 'rustfs') g = 'storage'
    else continue
    groups[g].items.push(sw)
  }
  return Object.values(groups).filter((g) => g.items.length > 0)
})

function mergeStatus(item: InstalledSoftware): InstalledSoftware {
  const liveStatus = lifecycleStore.getStatus(item.id)
  const livePid = lifecycleStore.getPid(item.id)
  const liveError = lifecycleStore.getError(item.id)
  return {
    ...item,
    // lifecycle store 有实时状态时优先用，否则回退到后端列表快照
    status: (liveStatus !== 'Unknown' ? liveStatus : item.status) as SoftwareStatus,
    pid: livePid ?? item.pid,
    last_error: liveError ?? item.last_error,
  }
}

async function loadInstalled() {
  loading.value = true
  try {
    installed.value = (await invoke('list_installed_software')) as InstalledSoftware[]
  } catch (e) {
    console.error('Failed to load installed:', e)
  } finally {
    loading.value = false
  }
}

async function onStart(item: InstalledSoftware) {
  // 自定义软件未配置启动命令时弹对话框
  if (item.is_custom && !item.custom_start_command) {
    customTarget.value = item
    return
  }
  try {
    await invoke('start_software', { installedId: item.id })
  } catch (e) {
    console.error('start failed:', e)
  }
}

async function onStop(item: InstalledSoftware) {
  try {
    await invoke('stop_software', { installedId: item.id })
  } catch (e) {
    console.error('stop failed:', e)
  }
}

function onConfig(item: InstalledSoftware) {
  configTarget.value = item
}

function onStartupSettings(item: InstalledSoftware) {
  startupTarget.value = item
}

function onUninstall(item: InstalledSoftware) {
  uninstallTarget.value = item
}

onMounted(async () => {
  await lifecycleStore.initListener()
  await loadInstalled()
  // 30s 兜底轮询（事件丢失时仍能同步状态）
  pollTimer = setInterval(loadInstalled, 30_000)
})

onBeforeUnmount(() => {
  lifecycleStore.destroyListener()
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
})
</script>

<style scoped>
.content {
  display: flex;
  flex-direction: column;
  gap: 28px;
}
.category-section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.category-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-muted-foreground);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  display: flex;
  align-items: center;
  gap: 8px;
}
.category-title svg {
  width: 14px;
  height: 14px;
  color: var(--color-primary);
}
.category-title .count {
  margin-left: auto;
  font-size: 11px;
  text-transform: none;
  letter-spacing: 0;
}
.instance-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
