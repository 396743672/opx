<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:package-variant-closed"
      :title="$t('softwareRepository')"
      :subtitle="$t('installNewSoftware')"
    >
      <template #actions>
        <button
          v-if="selectedCount > 0"
          class="btn primary batch-install-btn"
          :disabled="batchInstalling"
          @click="batchInstall"
        >
          <Icon v-if="batchInstalling" icon="mdi:loading" class="spinning" />
          <Icon v-else icon="mdi:download-multiple" />
          {{ $t('batchInstall') }} ({{ selectedCount }})
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
      <CategoryTabs v-model="activeCategory" :tabs="categoryTabs" />

      <template v-if="activeCategory === 'all'">
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
              :selectable="isSelectable(entry)"
              :selected="selectedKeys.has(entry.key)"
              @install="openInstall"
              @toggle-select="toggleSelect(entry)"
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
      </template>

      <template v-else>
        <div class="sw-grid">
          <SoftwareCard
            v-for="entry in activeEntries"
            :key="entry.key"
            :entry="entry"
            :installed-version="getInstalledVersion(entry.key)"
            :is-default-jre="entry.key === 'jre' && defaultJreId !== null"
            :selectable="isSelectable(entry)"
            :selected="selectedKeys.has(entry.key)"
            @install="openInstall"
            @toggle-select="toggleSelect(entry)"
          />
        </div>
      </template>
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
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@/utils/ipc'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SoftwareCard from '../components/SoftwareCard.vue'
import CategoryTabs from '../components/CategoryTabs.vue'
import type { CategoryTab } from '../components/CategoryTabs.vue'
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

// 批量选择安装状态
const selectedKeys = ref<Set<string>>(new Set())
const batchInstalling = ref(false)
const selectedCount = computed(() => selectedKeys.value.size)

// 分类 Tab：全部 + 各非空分类
const activeCategory = ref<string>('all')

const categoryTabs = computed<CategoryTab[]>(() => {
  const all = catalogStore.groupedEntries
  const total = Object.values(all).reduce((n, e) => n + e.length, 0)
  const tabs: CategoryTab[] = [
    { key: 'all', label: t('all'), icon: 'mdi:view-grid-outline', count: total },
  ]
  for (const [cat, entries] of Object.entries(all)) {
    if (entries.length === 0) continue
    const c = cat as SoftwareCategory
    tabs.push({ key: cat, label: categoryName(c), icon: categoryIcon(c), count: entries.length })
  }
  return tabs
})

// 当前选中分类的条目；'all' 时返回空（由模板走全部分类小节）
const activeEntries = computed<CatalogEntry[]>(() => {
  if (activeCategory.value === 'all') return []
  return catalogStore.groupedEntries[activeCategory.value as SoftwareCategory] ?? []
})

// 分类变化后当前分类消失则回落「全部」
watch(
  () => Object.keys(catalogStore.groupedEntries),
  (keys) => {
    if (activeCategory.value !== 'all' && !keys.includes(activeCategory.value)) {
      activeCategory.value = 'all'
    }
  },
)

let completedUnlisten: UnlistenFn | null = null

function getInstalledVersion(key: string): string | null {
  const found = installedList.value.find((s) => s.key === key && !s.is_custom)
  return found ? found.version : null
}

/** 已安装及安装中的条目不可批量选择 */
function isSelectable(entry: CatalogEntry): boolean {
  return getInstalledVersion(entry.key) === null && !installStore.hasActiveTask(entry.key)
}

function toggleSelect(entry: CatalogEntry) {
  const next = new Set(selectedKeys.value)
  if (next.has(entry.key)) next.delete(entry.key)
  else next.add(entry.key)
  selectedKeys.value = next
}

/** 批量安装：每个选中条目用 catalog 默认版本 + 首个镜像源，逐条调用 install_software */
async function batchInstall() {
  const entries = Object.values(catalogStore.groupedEntries)
    .flat()
    .filter((e) => selectedKeys.value.has(e.key))
  if (entries.length === 0) return
  batchInstalling.value = true
  try {
    for (const entry of entries) {
      const version =
        entry.versions.find((v) => v.version === entry.default_version) ?? entry.versions[0]
      if (!version) continue
      try {
        const installId = await invoke('install_software', {
          params: {
            key: entry.key,
            version: version.version,
            mirror_index: 0,
            set_as_default_jre: false,
          },
        }) as string
        installStore.createTask(installId, entry.key, `${entry.name} ${version.version}`)
      } catch (e) {
        console.error(`Failed to install ${entry.key}:`, e)
      }
    }
  } finally {
    batchInstalling.value = false
    selectedKeys.value = new Set()
  }
}

function categoryName(cat: SoftwareCategory): string {
  if (cat === SoftwareCategory.Database) return t('categoryDatabase')
  if (cat === SoftwareCategory.Runtime) return t('categoryRuntime')
  if (cat === SoftwareCategory.Cache) return t('categoryCache')
  if (cat === SoftwareCategory.WebServer) return t('categoryWebServer')
  if (cat === SoftwareCategory.Registry) return t('categoryRegistry')
  if (cat === SoftwareCategory.Storage) return t('categoryStorage')
  if (cat === SoftwareCategory.MessageQueue) return t('categoryMessageQueue')
  if (cat === SoftwareCategory.Search) return t('categorySearch')
  if (cat === SoftwareCategory.TimeSeries) return t('categoryTimeSeries')
  return cat
}

function categoryIcon(cat: SoftwareCategory): string {
  if (cat === SoftwareCategory.Database) return 'mdi:database'
  if (cat === SoftwareCategory.Runtime) return 'mdi:play-circle'
  if (cat === SoftwareCategory.Cache) return 'mdi:database'
  if (cat === SoftwareCategory.WebServer) return 'mdi:web'
  if (cat === SoftwareCategory.Registry) return 'mdi:hexagon-multiple'
  if (cat === SoftwareCategory.Storage) return 'mdi:storage'
  if (cat === SoftwareCategory.MessageQueue) return 'mdi:message-text-outline'
  if (cat === SoftwareCategory.Search) return 'mdi:magnify'
  if (cat === SoftwareCategory.TimeSeries) return 'mdi:chart-line'
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
