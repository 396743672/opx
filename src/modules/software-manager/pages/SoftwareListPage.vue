<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:package-variant-closed"
      :title="$t('softwareManagement')"
      :subtitle="$t('installedSoftware')"
    >
      <template #actions>
        <button class="btn" @click="onStopAll" :disabled="Object.keys(actingStates).length > 0">
          <Icon icon="mdi:stop-circle-outline" /> {{ $t('stopAllSoftware') }}
        </button>
        <button class="btn" @click="loadInstalled" :disabled="loading">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
        <button class="btn" @click="showLogSearch = true">
          <Icon icon="mdi:magnify" /> {{ $t('globalLogSearch') }}
        </button>
      </template>
    </PageHeader>

    <GlobalLogSearchDialog v-if="showLogSearch" @close="showLogSearch = false" />

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

    <div v-else-if="grouped.length === 0">
      <EmptyState
        icon="mdi:package-variant-closed"
        :title="$t('noManageableSoftware')"
        :description="$t('noManageableSoftwareDesc')"
      />
    </div>

    <div v-else class="content">
      <CategoryTabs v-model="activeCategory" :tabs="categoryTabs" />
      <div v-for="group in displayGroups" :key="group.category" class="category-section">
        <div class="category-title">
          <Icon :icon="group.icon" />
          {{ $t(group.label) }}
          <span class="count">{{ group.items.length }}</span>
        </div>
        <div class="instance-grid">
          <SoftwareInstanceRow
            v-for="item in group.items"
            :key="item.id"
            :software="mergeStatus(item)"
            :acting-states="actingStates"
            :upgrade-to="upgradeMap[item.key]"
            :rollback-to="rollbackMap[item.key]"
            :deps-name-map="depsNameMap"
            @start="onStart(item)"
            @stop="onStop(item)"
            @config="onConfig(item)"
            @startup-settings="onStartupSettings(item)"
            @deps="onDeps(item)"
            @uninstall="onUninstall(item)"
            @log="onLog(item)"
            @backup="onBackup(item)"
            @reset="onReset(item)"
            @upgrade="onUpgrade(item)"
            @rollback="onRollback(item)"
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
      @close="onStartupSettingsClose"
    />
    <DependenciesDialog
      v-if="depsTarget"
      :software="depsTarget"
      :all-software="installed"
      @close="depsTarget = null"
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
      @uninstalled="onUninstalled"
    />

    <LogViewerDialog
      v-if="logTarget"
      :software="logTarget"
      :instances="manageableInstances"
      @close="logTarget = null"
    />
    <BackupRestoreDialog
      v-if="backupTarget"
      :software="backupTarget"
      :instances="manageableInstances"
      :initial-tab="backupInitialTab"
      @close="backupTarget = null"
    />
  </div>

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
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SoftwareInstanceRow from '../components/SoftwareInstanceRow.vue'
import CategoryTabs from '../components/CategoryTabs.vue'
import type { CategoryTab } from '../components/CategoryTabs.vue'
import ConfigEditDialog from '../components/ConfigEditDialog.vue'
import StartupSettingsDialog from '../components/StartupSettingsDialog.vue'
import DependenciesDialog from '../components/DependenciesDialog.vue'
import CustomStartCommandDialog from '../components/CustomStartCommandDialog.vue'
import UninstallBlockedDialog from '../components/UninstallBlockedDialog.vue'
import LogViewerDialog from '../components/LogViewerDialog.vue'
import GlobalLogSearchDialog from '../components/GlobalLogSearchDialog.vue'
import BackupRestoreDialog from '../components/BackupRestoreDialog.vue'
import InstallProgressDialog from '../components/InstallProgressDialog.vue'
import { useLifecycleStore } from '../stores/lifecycle'
import { useInstallStore } from '../stores/install'
import { SoftwareCategory, SoftwareStatus, type InstalledSoftware } from '@/models/software'
import type { UpgradeInfo } from '@/models/software'

const lifecycleStore = useLifecycleStore()
const { t } = useI18n()

const installed = ref<InstalledSoftware[]>([])
const showLogSearch = ref(false)
const loading = ref(false)
const configTarget = ref<InstalledSoftware | null>(null)
const startupTarget = ref<InstalledSoftware | null>(null)
const depsTarget = ref<InstalledSoftware | null>(null)
const customTarget = ref<InstalledSoftware | null>(null)
const uninstallTarget = ref<InstalledSoftware | null>(null)
const logTarget = ref<InstalledSoftware | null>(null)
const backupTarget = ref<InstalledSoftware | null>(null)
// 备份/恢复对话框初始 Tab：backup 按钮打开快照列表，reset 按钮直接定位到一键重置（Tab B）
const backupInitialTab = ref<'snapshots' | 'reset'>('snapshots')
// 防重：记录每个软件当前正在执行的操作（'start' | 'stop'），用于防止重复点击
const actingStates = ref<Record<string, 'start' | 'stop'>>({})
const installStore = useInstallStore()
// key → 目标升级版本（无可升级则无该 key）
const upgradeMap = ref<Record<string, string>>({})
// key → 可回滚的旧版本（同 key 存在 <ver>.bak 备份时）
const rollbackMap = ref<Record<string, string>>({})
const allCollapsed = ref(false)
let pollTimer: ReturnType<typeof setInterval> | null = null
let completedUnlisten: UnlistenFn | null = null

function applyUpgrades(list: UpgradeInfo[]) {
  const map: Record<string, string> = {}
  const rb: Record<string, string> = {}
  for (const u of list) {
    if (u.target_version) map[u.key] = u.target_version
    if (u.rollback_to) rb[u.key] = u.rollback_to
  }
  upgradeMap.value = map
  rollbackMap.value = rb
}

/** 内置检测先行返回；随后并行在线刷新，失败静默回退 */
async function loadUpgrades() {
  try {
    const list = await invoke<UpgradeInfo[]>('check_upgrades')
    applyUpgrades(list)
    refreshUpgrades(list.map((u) => u.key))
  } catch (e) {
    console.error('check_upgrades failed:', e)
  }
}

async function refreshUpgrades(keys: string[]) {
  const uniq = [...new Set(keys)]
  await Promise.allSettled(
    uniq.map((k) => invoke('fetch_remote_versions_for', { key: k })),
  )
  try {
    applyUpgrades(await invoke<UpgradeInfo[]>('check_upgrades'))
  } catch (e) {
    console.error('refresh upgrades failed:', e)
  }
}

/** 替换式升级：停旧→装新→迁移数据，列表保持一条记录（后端 upgrade_software） */
async function onUpgrade(item: InstalledSoftware) {
  if (installStore.hasActiveTask(item.key)) return // 该软件已有安装/升级任务防重
  try {
    const installId = (await invoke('upgrade_software', { installedId: item.id })) as string
    installStore.createTask(installId, item.key, `升级 ${item.name}`)
  } catch (e) {
    console.error('upgrade failed:', e)
  }
}

/** 一键回滚：恢复升级时保留的 .bak 备份（后端 rollback_software），完成后刷新列表与可升级徽标 */
async function onRollback(item: InstalledSoftware) {
  if (installStore.hasActiveTask(item.key)) return
  try {
    await invoke('rollback_software', { installedId: item.id })
    loadInstalled()
    loadUpgrades()
  } catch (e) {
    console.error('rollback failed:', e)
  }
}

interface Group {
  category: string
  label: string
  icon: string
  items: InstalledSoftware[]
}

const grouped = computed<Group[]>(() => {
  const groups: Record<string, Group> = {
    runtime: { category: 'runtime', label: 'runtime', icon: 'mdi:language-java', items: [] },
    database: { category: 'database', label: 'database', icon: 'mdi:database', items: [] },
    cache: { category: 'cache', label: 'cache', icon: 'mdi:lightning-bolt', items: [] },
    webserver: { category: 'webserver', label: 'webServer', icon: 'mdi:web', items: [] },
    storage: { category: 'storage', label: 'objectStorage', icon: 'mdi:storage', items: [] },
    registry: { category: 'registry', label: 'registry', icon: 'mdi:hexagon-multiple', items: [] },
    messagequeue: { category: 'messagequeue', label: 'messageQueue', icon: 'mdi:message-text-outline', items: [] },
    search: { category: 'search', label: 'search', icon: 'mdi:magnify', items: [] },
    timeseries: { category: 'timeseries', label: 'timeSeries', icon: 'mdi:chart-line', items: [] },
    custom: { category: 'custom', label: 'custom', icon: 'mdi:upload', items: [] },
  }
  // 后端 list_installed_software 已为每个已装软件附加 catalog category，
  // 这里按 category 归组即可——新增软件只需在 catalog 中标好分类，无需改本映射。
  const CATEGORY_TO_GROUP: Partial<Record<SoftwareCategory, keyof typeof groups>> = {
    [SoftwareCategory.Runtime]: 'runtime',
    [SoftwareCategory.Database]: 'database',
    [SoftwareCategory.Cache]: 'cache',
    [SoftwareCategory.WebServer]: 'webserver',
    [SoftwareCategory.Storage]: 'storage',
    [SoftwareCategory.Registry]: 'registry',
    [SoftwareCategory.MessageQueue]: 'messagequeue',
    [SoftwareCategory.Search]: 'search',
    [SoftwareCategory.TimeSeries]: 'timeseries',
  }
  const list = [...installed.value]
  list.sort((a, b) => a.key.localeCompare(b.key) || a.version.localeCompare(b.version))
  for (const sw of list) {
    // 自定义软件无 catalog 分类；其余按 category 归组（未知分类丢到 custom 保持可见）
    const g = sw.is_custom || !sw.category
      ? 'custom'
      : (CATEGORY_TO_GROUP[sw.category] ?? 'custom')
    groups[g].items.push(sw)
  }
  return Object.values(groups).filter((g) => g.items.length > 0)
})

// 分类 Tab：全部 + 各非空分类
const activeCategory = ref<string>('all')

const categoryTabs = computed<CategoryTab[]>(() => {
  const tabs: CategoryTab[] = [
    { key: 'all', label: t('all'), icon: 'mdi:view-grid-outline', count: manageableInstances.value.length },
  ]
  for (const g of grouped.value) {
    tabs.push({ key: g.category, label: t(g.label), icon: g.icon, count: g.items.length })
  }
  return tabs
})

// 当前选中的分类组；'all' 时返回全部分组（保持原分类小节布局）
const displayGroups = computed(() => {
  if (activeCategory.value === 'all') return grouped.value
  const g = grouped.value.find((x) => x.category === activeCategory.value)
  return g ? [g] : []
})

// 分组变化（如卸载/安装）后当前分类消失则回落「全部」
watch(grouped, (val) => {
  if (activeCategory.value !== 'all' && !val.some((g) => g.category === activeCategory.value)) {
    activeCategory.value = 'all'
  }
})

// 可运维实例（排除 Runtime 类软件，与 SoftwareInstanceRow.canOps 一致）
const manageableInstances = computed(() =>
  installed.value.filter((s) => s.category !== SoftwareCategory.Runtime),
)

// 软件 id → 显示名（卡片展示已配置依赖的友好名用）
const depsNameMap = computed(() => {
  const map: Record<string, string> = {}
  for (const s of installed.value) map[s.id] = s.name
  return map
})

function mergeStatus(item: InstalledSoftware): InstalledSoftware {
  const liveStatus = lifecycleStore.getStatus(item.id)
  const livePid = lifecycleStore.getPid(item.id)
  const liveError = lifecycleStore.getError(item.id)
  const status = (liveStatus !== SoftwareStatus.Unknown ? liveStatus : item.status) as SoftwareStatus

  // 根据状态决定显示：
  // - Running/Starting: 显示 pid（进程在跑）
  // - Error: 保留事件带回的 pid（健康检查超时时进程仍活，可停止；启动失败/崩溃时 pid 为 null）
  // - Stopped: 清除 pid 显示
  // - Running/Stopped: 清除 last_error（旧错误不展示）
  // - Error: 显示 last_error（错误原因）
  const pid =
    status === SoftwareStatus.Running ||
    status === SoftwareStatus.Starting ||
    status === SoftwareStatus.Error
      ? (livePid ?? item.pid)
      : null
  const last_error = status === SoftwareStatus.Error ? (liveError ?? item.last_error) : null

  return {
    ...item,
    status,
    pid,
    last_error,
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

async function onStopAll() {
  const running = installed.value.filter(
    (s) => s.status === SoftwareStatus.Running || s.status === SoftwareStatus.Starting
  )
  for (const s of running) {
    if (!actingStates.value[s.id]) {
      actingStates.value[s.id] = 'stop'
      invoke('stop_software', { installedId: s.id }).catch(() => {
        delete actingStates.value[s.id]
      })
    }
  }
}

async function onStart(item: InstalledSoftware) {
  // 防重：如果该软件正在启动或停止中，忽略重复点击
  if (actingStates.value[item.id]) return
  // 自定义软件未配置启动命令时弹对话框
  if (item.is_custom && !item.custom_start_command) {
    customTarget.value = item
    return
  }
  actingStates.value[item.id] = 'start'
  // 安全兜底：如果 status-changed 事件丢失，5 秒后强制清除 acting 状态
  const safetyTimer = setTimeout(() => {
    delete actingStates.value[item.id]
  }, 5000)
  try {
    // 从运行时 store 取初始化密码（如 MySQL 初始化 root 密码），仅首次初始化消费一次；
    // 已初始化或为空则不传，维持后端向后兼容的空密码行为。
    const initPw = lifecycleStore.getInitPassword(item.id)
    const args: Record<string, unknown> = { installedId: item.id }
    if (initPw) (args as any).initPassword = initPw
    await invoke('start_software', args)
  } catch (e) {
    console.error('start failed:', e)
    // 启动命令本身失败（如 validate 阶段拒绝），立即清除 acting 状态 + 取消安全计时器
    clearTimeout(safetyTimer)
    delete actingStates.value[item.id]
  }
}

async function onStop(item: InstalledSoftware) {
  // 防重：如果该软件正在启动或停止中，忽略重复点击
  if (actingStates.value[item.id]) return
  actingStates.value[item.id] = 'stop'
  // 安全兜底：如果 status-changed 事件丢失，5 秒后强制清除 acting 状态
  const safetyTimer = setTimeout(() => {
    delete actingStates.value[item.id]
  }, 5000)
  try {
    await invoke('stop_software', { installedId: item.id })
  } catch (e) {
    console.error('stop failed:', e)
    // 停止命令本身失败，立即清除 acting 状态 + 取消安全计时器
    clearTimeout(safetyTimer)
    delete actingStates.value[item.id]
  }
}

function onConfig(item: InstalledSoftware) {
  // 防御性检查：运行中的软件不允许修改配置
  const status = mergeStatus(item).status
  if (status === SoftwareStatus.Running) return
  configTarget.value = item
}

function onStartupSettings(item: InstalledSoftware) {
  startupTarget.value = item
}

function onDeps(item: InstalledSoftware) {
  depsTarget.value = item
}

function onStartupSettingsClose() {
  startupTarget.value = null
  loadInstalled()
}

function onUninstall(item: InstalledSoftware) {
  uninstallTarget.value = item
}

function onLog(item: InstalledSoftware) {
  logTarget.value = item
}

function onBackup(item: InstalledSoftware) {
  backupInitialTab.value = 'snapshots'
  backupTarget.value = item
}

function onReset(item: InstalledSoftware) {
  backupInitialTab.value = 'reset'
  backupTarget.value = item
}

function onUninstalled() {
  uninstallTarget.value = null
  loadInstalled()
}

onMounted(async () => {
  await installStore.initEvents()
  await loadInstalled()
  loadUpgrades()
  // 30s 兜底轮询（事件丢失时仍能同步状态）
  pollTimer = setInterval(loadInstalled, 30_000)
  // 升级/安装完成时自动刷新已装列表与可升级徽标（与 RepositoryPage 模式一致）
  completedUnlisten = await listen('install-progress', (event) => {
    const payload = event.payload as any
    if (payload.phase === 'completed') {
      loadInstalled()
      loadUpgrades()
    }
  })
})

// 监听 lifecycle store 状态变更：当状态转为 Starting/Stopping/Stopped/Error/Running
// 时清除对应的 actingStates，让按钮恢复可用（不再依赖 finally 中立即清除，
// 避免 start_software 命令立即返回后按钮过早恢复可点击的竞态窗口）。
watch(
  () => lifecycleStore.statuses,
  (newStatuses) => {
    for (const id of Object.keys(actingStates.value)) {
      const s = newStatuses[id]
      if (
        s === SoftwareStatus.Starting ||
        s === SoftwareStatus.Stopping ||
        s === SoftwareStatus.Stopped ||
        s === SoftwareStatus.Running ||
        s === SoftwareStatus.Error
      ) {
        delete actingStates.value[id]
      }
    }
  },
  { deep: true },
)

onBeforeUnmount(() => {
  installStore.cleanup()
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
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
  gap: 28px;
}
.category-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
.instance-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 12px;
}
@media (min-width: 768px) {
  .instance-grid {
    grid-template-columns: repeat(2, 1fr);
  }
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
