<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:leaf"
      :title="$t('springBoot')"
      :subtitle="$t('applicationList')"
    >
      <template #actions>
        <button class="btn" @click="refreshAll" :disabled="store.loading">
          <Icon
            :icon="store.loading ? 'mdi:loading' : 'mdi:refresh'"
            :class="{ spinning: store.loading }"
          />
          {{ $t('refresh') }}
        </button>
        <button class="btn primary" @click="openAddDialog">
          <Icon icon="mdi:plus" /> {{ $t('addApplication') }}
        </button>
        <button v-if="store.groups.length > 0" class="btn" @click="showGroupManager = true">
          <Icon icon="mdi:cog" /> {{ $t('groupConfig') }}
        </button>
      </template>
    </PageHeader>

    <!-- Group tabs -->
    <div v-if="store.groups.length > 0" class="flex gap-2 mb-4 flex-wrap">
      <button
        class="tab-btn"
        :class="{ active: activeGroup === null }"
        @click="activeGroup = null"
      >{{ $t('all') }}</button>
      <button
        v-for="g in store.groups"
        :key="g.id"
        class="tab-btn"
        :class="{ active: activeGroup === g.name }"
        @click="activeGroup = g.name"
      >{{ g.name }}</button>
    </div>

    <!-- Loading state -->
    <div v-if="store.loading && store.apps.length === 0" class="py-8">
      <EmptyState icon="mdi:loading" :title="$t('loading')" />
    </div>

    <!-- Empty state -->
    <div v-else-if="filteredApps.length === 0">
      <EmptyState
        icon="mdi:spring"
        :title="$t('comingSoon')"
        :description="$t('comingSoonDesc')"
      >
        <template #action>
          <button class="btn primary" @click="openAddDialog">
            <Icon icon="mdi:plus" /> {{ $t('addApplication') }}
          </button>
        </template>
      </EmptyState>
    </div>

    <!-- App grid -->
    <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
      <AppCard
        v-for="app in filteredApps"
        :key="app.id"
        :app="app"
        @start="id => handleAction('start', id)"
        @stop="id => handleAction('stop', id)"
        @restart="id => handleAction('restart', id)"
        @config="handleConfig"
        @replace="handleReplace"
        @monitor="handleMonitor"
        @logs="handleLogs"
      />
    </div>

    <!-- Dialogs -->
    <AppFormDialog
      v-if="showFormDialog"
      :app="editingApp"
      @saved="onFormSaved"
      @cancel="closeFormDialog"
    />
    <JvmMetricsDialog
      v-if="showJvmDialog"
      :app-id="monitoringAppId"
      :app-name="monitoringAppName"
      @close="closeJvmDialog"
    />
    <LogViewer
      v-if="showLogViewer"
      :app-id="logViewingAppId"
      :app-name="logViewingAppName"
      :log-path="logViewingAppLogPath"
      @close="closeLogViewer"
    />
    <GroupManager
      v-if="showGroupManager"
      :groups="store.groups"
      @close="closeGroupManager"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'
import AppCard from '../components/AppCard.vue'
import AppFormDialog from '../components/AppFormDialog.vue'
import JvmMetricsDialog from '../components/JvmMetricsDialog.vue'
import LogViewer from '../components/LogViewer.vue'
import GroupManager from '../components/GroupManager.vue'
import { useSpringBootStore } from '../stores/springboot'
import type { SpringBootApp } from '@/models/springboot'

useI18n()

const store = useSpringBootStore()
const activeGroup = ref<string | null>(null)

// Form dialog
const showFormDialog = ref(false)
const editingApp = ref<SpringBootApp | null>(null)

// Monitor dialog
const showJvmDialog = ref(false)
const monitoringAppId = ref('')
const monitoringAppName = ref('')

// Log viewer
const showLogViewer = ref(false)
const logViewingAppId = ref('')
const logViewingAppName = ref('')
const logViewingAppLogPath = ref('')

// Group manager
const showGroupManager = ref(false)

// Event listener cleanup
let unlisten: UnlistenFn | null = null

const filteredApps = computed(() => {
  if (activeGroup.value === null) return store.apps
  return store.apps.filter(a => a.group === activeGroup.value)
})

async function refreshAll() {
  await Promise.all([store.fetchApps(), store.fetchGroups()])
}

function openAddDialog() {
  editingApp.value = null
  showFormDialog.value = true
}

function closeFormDialog() {
  showFormDialog.value = false
  editingApp.value = null
}

function onFormSaved() {
  showFormDialog.value = false
  editingApp.value = null
  store.fetchApps()
}

function closeJvmDialog() {
  showJvmDialog.value = false
  monitoringAppId.value = ''
  monitoringAppName.value = ''
}

function closeLogViewer() {
  showLogViewer.value = false
  logViewingAppId.value = ''
  logViewingAppName.value = ''
  logViewingAppLogPath.value = ''
}

function closeGroupManager() {
  showGroupManager.value = false
  store.fetchGroups()
}

async function handleAction(action: 'start' | 'stop' | 'restart', id: string) {
  try {
    if (action === 'start') await store.startApp(id)
    else if (action === 'stop') await store.stopApp(id)
    else await store.restartApp(id)
  } finally {
    store.fetchApps()
  }
}

function handleConfig(id: string) {
  const app = store.apps.find(a => a.id === id)
  if (app) {
    editingApp.value = app
    showFormDialog.value = true
  }
}

async function handleReplace(id: string) {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'JAR', extensions: ['jar'] }],
  })
  if (selected && typeof selected === 'string') {
    await store.replaceJar(id, selected)
    await store.fetchApps()
  }
}

function handleMonitor(id: string) {
  const app = store.apps.find(a => a.id === id)
  if (app) {
    monitoringAppId.value = id
    monitoringAppName.value = app.name
    showJvmDialog.value = true
  }
}

function handleLogs(id: string) {
  const app = store.apps.find(a => a.id === id)
  if (app) {
    logViewingAppId.value = id
    logViewingAppName.value = app.name
    logViewingAppLogPath.value = app.log_path
    showLogViewer.value = true
  }
}

onMounted(async () => {
  await Promise.all([
    store.fetchApps(),
    store.fetchGroups(),
    store.fetchJdkList(),
    store.fetchDependencyCandidates(),
  ])

  unlisten = await listen('springboot-status-changed', () => {
    store.fetchApps()
  })
})

onBeforeUnmount(() => {
  if (unlisten) unlisten()
})
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
  white-space: nowrap;
}
.btn:hover {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.tab-btn {
  padding: 6px 14px;
  border-radius: 6px;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-muted-foreground);
  cursor: pointer;
  transition: all 0.15s;
}
.tab-btn:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.tab-btn.active {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
