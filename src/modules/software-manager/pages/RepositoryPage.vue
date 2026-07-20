<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:package-variant-closed"
      :title="$t('softwareRepository')"
      :subtitle="$t('installNewSoftware')"
    >
      <template #actions>
        <button class="btn" @click="refreshCatalog" :disabled="catalogStore.loading">
          <Icon :icon="catalogStore.loading ? 'mdi:loading' : 'mdi:refresh'" :class="{ spinning: catalogStore.loading }" />
          {{ catalogStore.loading ? $t('fetchingVersions') : $t('refreshCatalog') }}
        </button>
      </template>
    </PageHeader>

    <div v-if="catalogStore.loading && catalogStore.entries.length === 0">
      <EmptyState
        icon="mdi:loading"
        :title="$t('loading')"
        :description="''"
      />
    </div>

    <div v-else class="content">
      <div
        v-for="(entries, category) in catalogStore.groupedEntries"
        :key="category"
        class="category-section"
        v-show="entries.length > 0"
      >
        <div class="category-title">
          <Icon :icon="categoryIcon(category)" /> {{ categoryName(category) }}
        </div>
        <div class="sw-grid">
          <SoftwareCard
            v-for="entry in entries"
            :key="entry.key"
            :entry="entry"
            :installed-version="getInstalledVersion(entry.key)"
            :is-default-jre="entry.key === 'jre' && defaultJreId !== null"
            @install="openInstall"
          />
        </div>
      </div>

      <div class="category-section">
        <div class="category-title">
          <Icon icon="mdi:plus-box" /> {{ $t('uploadCustom') }}
        </div>
        <div class="custom-card" @click="openCustomInstall">
          <div class="sw-icon">
            <Icon icon="mdi:upload" />
          </div>
          <div class="custom-body">
            <h3>{{ $t('uploadCustom') }}</h3>
            <p>{{ $t('supportedFormats') }}</p>
          </div>
          <button class="btn primary">
            <Icon icon="mdi:upload" /> {{ $t('install') }}
          </button>
        </div>
      </div>
    </div>

    <InstallDialog
      v-if="showInstallDialog && selectedEntry"
      :entry="selectedEntry"
      @cancel="showInstallDialog = false"
      @installed="onInstalled"
    />
    <CustomInstallDialog
      v-if="showCustomDialog"
      @cancel="showCustomDialog = false"
      @installed="onInstalled"
    />

    <!-- 右下角浮动进度通知容器（Teleport 到 body 确保 fixed 相对窗口） -->
    <Teleport to="body">
    <div v-if="installStore.activeTasks.length" class="progress-panel">
      <div class="progress-panel-header" @click="allCollapsed = !allCollapsed">
        <Icon icon="mdi:download" class="text-primary" />
        <span>{{ $t('downloading') }} ({{ installStore.activeTasks.length }})</span>
        <Icon :icon="allCollapsed ? 'mdi:chevron-up' : 'mdi:chevron-down'" class="text-muted-foreground ml-auto" />
      </div>
      <div v-show="!allCollapsed" class="progress-panel-body">
        <InstallProgressDialog
          v-for="task in installStore.activeTasks"
          :key="task.id"
          :task="task"
        />
      </div>
    </div>
    </Teleport>
  </div>
  <Teleport to="body">
    <div v-if="catalogStore.error" class="overlay" @click.self="catalogStore.error = null">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ $t('refreshCatalogFailed') }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="catalogStore.error = null">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SoftwareCard from '../components/SoftwareCard.vue'
import InstallDialog from '../components/InstallDialog.vue'
import InstallProgressDialog from '../components/InstallProgressDialog.vue'
import CustomInstallDialog from '../components/CustomInstallDialog.vue'
import { useCatalogStore } from '../stores/catalog'
import { useInstallStore } from '../stores/install'
import { SoftwareCategory, type CatalogEntry, type InstalledSoftware } from '@/models/software'
import type { AppSettings } from '@/models/settings'

const { t } = useI18n()

const catalogStore = useCatalogStore()
const installStore = useInstallStore()

const showInstallDialog = ref(false)
const showCustomDialog = ref(false)
const selectedEntry = ref<CatalogEntry | null>(null)
const allCollapsed = ref(false)
const installedList = ref<InstalledSoftware[]>([])
const defaultJreId = ref<string | null>(null)

let completedUnlisten: UnlistenFn | null = null

function getInstalledVersion(key: string): string | null {
  const found = installedList.value.find((s) => s.key === key && !s.is_custom)
  return found ? found.version : null
}

function categoryName(cat: SoftwareCategory): string {
  if (cat === SoftwareCategory.Database) return t('categoryDatabase')
  if (cat === SoftwareCategory.Runtime) return t('categoryRuntime')
  if (cat === SoftwareCategory.Cache) return t('categoryCache')
  if (cat === SoftwareCategory.WebServer) return t('categoryWebServer')
  return cat
}

function categoryIcon(cat: SoftwareCategory): string {
  if (cat === SoftwareCategory.Database) return 'mdi:database'
  if (cat === SoftwareCategory.Runtime) return 'mdi:play-circle'
  if (cat === SoftwareCategory.Cache) return 'mdi:database'
  if (cat === SoftwareCategory.WebServer) return 'mdi:web'
  return 'mdi:package-variant-closed'
}

function openInstall(entry: CatalogEntry) {
  selectedEntry.value = entry
  showInstallDialog.value = true
}

function openCustomInstall() {
  showCustomDialog.value = true
}

function onInstalled(_id: string) {
  showInstallDialog.value = false
  showCustomDialog.value = false
  // 刷新已安装列表
  loadInstalled()
}

async function refreshCatalog() {
  await catalogStore.refreshCatalog()
}

async function loadInstalled() {
  try {
    installedList.value = await invoke('list_installed_software') as InstalledSoftware[]
  } catch (e) {
    console.error('Failed to load installed:', e)
  }
}

async function loadDefaultJre() {
  try {
    const settings = await invoke('get_settings') as AppSettings
    defaultJreId.value = settings.jre_default_id ?? null
  } catch (e) {
    console.error('Failed to load settings:', e)
  }
}

onMounted(async () => {
  await catalogStore.loadCatalog()
  await loadInstalled()
  await loadDefaultJre()
  await installStore.initEvents()
  // 监听安装完成事件，自动刷新已安装列表与默认 JRE
  completedUnlisten = await listen('install-progress', (event) => {
    const payload = event.payload as any
    if (payload.phase === 'completed') {
      loadInstalled()
      loadDefaultJre()
    }
  })
})

onUnmounted(() => {
  installStore.cleanup()
  if (completedUnlisten) {
    completedUnlisten()
    completedUnlisten = null
  }
})
</script>

<style scoped>
.content {
  display: flex;
  flex-direction: column;
  gap: 24px;
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
  transition: background 0.15s;
}
.btn:hover {
  background: var(--color-muted);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.category-section {
  margin-bottom: 24px;
}
.category-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-muted-foreground);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.category-title svg {
  color: var(--color-primary);
}
.sw-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 14px;
}
@media (max-width: 1100px) {
  .sw-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
@media (max-width: 720px) {
  .sw-grid {
    grid-template-columns: 1fr;
  }
}
.custom-card {
  border: 1px dashed var(--color-border);
  background: color-mix(in oklch, var(--color-card) 60%, transparent);
  border-radius: var(--radius-lg);
  padding: 18px;
  display: flex;
  align-items: center;
  gap: 16px;
  cursor: pointer;
}
.custom-card:hover {
  border-color: var(--color-primary);
  background: color-mix(in oklch, var(--color-primary) 6%, transparent);
}
.custom-card .sw-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--color-warning) 14%, transparent);
  color: var(--color-warning);
  font-size: 22px;
}
.custom-card .custom-body {
  flex: 1;
}
.custom-card h3 {
  font-size: 15px;
  font-weight: 600;
}
.custom-card p {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
}
.refresh-error {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-radius: 8px;
  background: color-mix(in oklch, var(--color-destructive) 10%, transparent);
  border: 1px solid color-mix(in oklch, var(--color-destructive) 30%, transparent);
  color: var(--color-destructive);
  font-size: 13px;
  margin-bottom: 16px;
}
.refresh-error svg {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

/* 右下角浮动进度面板 */
.progress-panel {
  position: fixed;
  bottom: 16px;
  right: 16px;
  z-index: 100;
  width: 360px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-popover);
  box-shadow: var(--shadow-popover);
  overflow: hidden;
}
.progress-panel-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  user-select: none;
  background: var(--color-muted);
}
.progress-panel-header svg {
  width: 16px;
  height: 16px;
}
.ml-auto {
  margin-left: auto;
}
.progress-panel-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 6px 8px;
  max-height: 360px;
  overflow-y: auto;
}
</style>
