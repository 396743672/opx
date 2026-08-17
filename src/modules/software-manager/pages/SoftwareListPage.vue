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

    <div v-else-if="grouped.length === 0">
      <EmptyState
        icon="mdi:package-variant-closed"
        :title="$t('noManageableSoftware')"
        :description="$t('noManageableSoftwareDesc')"
      />
    </div>

    <div v-else class="content">
      <div v-for="group in grouped" :key="group.category" class="category-section">
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
            @start="onStart(item)"
            @stop="onStop(item)"
            @config="onConfig(item)"
            @startup-settings="onStartupSettings(item)"
            @uninstall="onUninstall(item)"
            @log="onLog(item)"
            @backup="onBackup(item)"
            @reset="onReset(item)"
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
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import SoftwareInstanceRow from '../components/SoftwareInstanceRow.vue'
import ConfigEditDialog from '../components/ConfigEditDialog.vue'
import StartupSettingsDialog from '../components/StartupSettingsDialog.vue'
import CustomStartCommandDialog from '../components/CustomStartCommandDialog.vue'
import UninstallBlockedDialog from '../components/UninstallBlockedDialog.vue'
import LogViewerDialog from '../components/LogViewerDialog.vue'
import BackupRestoreDialog from '../components/BackupRestoreDialog.vue'
import { useLifecycleStore } from '../stores/lifecycle'
import { SoftwareStatus, type InstalledSoftware } from '@/models/software'

const lifecycleStore = useLifecycleStore()
useI18n()

const installed = ref<InstalledSoftware[]>([])
const loading = ref(false)
const configTarget = ref<InstalledSoftware | null>(null)
const startupTarget = ref<InstalledSoftware | null>(null)
const customTarget = ref<InstalledSoftware | null>(null)
const uninstallTarget = ref<InstalledSoftware | null>(null)
const logTarget = ref<InstalledSoftware | null>(null)
const backupTarget = ref<InstalledSoftware | null>(null)
// 备份/恢复对话框初始 Tab：backup 按钮打开快照列表，reset 按钮直接定位到一键重置（Tab B）
const backupInitialTab = ref<'snapshots' | 'reset'>('snapshots')
// 防重：记录每个软件当前正在执行的操作（'start' | 'stop'），用于防止重复点击
const actingStates = ref<Record<string, 'start' | 'stop'>>({})
let pollTimer: ReturnType<typeof setInterval> | null = null

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
    custom: { category: 'custom', label: 'custom', icon: 'mdi:upload', items: [] },
  }
  // JRE 也纳入管理页（提供卸载入口），放在 runtime 分组
  const list = [...installed.value]
  list.sort((a, b) => a.key.localeCompare(b.key) || a.version.localeCompare(b.version))
  for (const sw of list) {
    let g: keyof typeof groups
    if (sw.is_custom) g = 'custom'
    else if (sw.key === 'jre' || sw.key === 'jdk') g = 'runtime'
    else if (sw.key === 'mysql' || sw.key === 'postgresql' || sw.key === 'mongodb') g = 'database'
    else if (sw.key === 'redis') g = 'cache'
    else if (sw.key === 'nginx') g = 'webserver'
    else if (sw.key === 'minio' || sw.key === 'rustfs') g = 'storage'
    else if (sw.key === 'nacos') g = 'registry'
    else continue
    groups[g].items.push(sw)
  }
  return Object.values(groups).filter((g) => g.items.length > 0)
})

// 可运维实例（排除 JRE/JDK 运行时依赖，与 SoftwareInstanceRow.canOps 一致）
const manageableInstances = computed(() =>
  installed.value.filter((s) => s.key !== 'jre' && s.key !== 'jdk'),
)

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
  await lifecycleStore.initListener()
  await loadInstalled()
  // 30s 兜底轮询（事件丢失时仍能同步状态）
  pollTimer = setInterval(loadInstalled, 30_000)
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
</style>
