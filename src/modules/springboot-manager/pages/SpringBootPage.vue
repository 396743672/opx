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
        <button class="btn" @click="showGroupManager = true">
          <Icon icon="mdi:cog" /> {{ $t('groupConfig') }}
        </button>
        <button class="btn" @click="showGlobalEnv = true">
          <Icon icon="mdi:earth" /> 全局变量
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
      <button
        v-if="activeGroup && groupApps.length > 0"
        class="btn primary"
        :disabled="startingGroup"
        @click="startGroup"
      >
        <Icon :icon="startingGroup ? 'mdi:loading' : 'mdi:play'" :class="{ spinning: startingGroup }" />
        {{ $t('startAll') }}
      </button>
      <button
        v-if="activeGroup && groupApps.some(a => a.status === AppStatus.Running)"
        class="btn"
        :disabled="stoppingGroup"
        @click="stopGroup"
      >
        <Icon :icon="stoppingGroup ? 'mdi:loading' : 'mdi:stop'" :class="{ spinning: stoppingGroup }" />
        {{ $t('stopAll') }}
      </button>
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
        @delete="handleDelete"
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
    <GlobalEnvDialog
      v-if="showGlobalEnv"
      @close="showGlobalEnv = false"
    />

    <!-- Delete confirm -->
    <Teleport to="body">
      <div v-if="deleteTarget" class="overlay" @click.self="deleteTarget = null">
        <div class="confirm-box">
          <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /> {{ $t('confirmDelete') }}</div>
          <p class="confirm-msg">{{ $t('confirmDeleteMsg') }}「{{ deleteTarget.name }}」？</p>
          <div class="confirm-actions">
            <button class="btn" @click="deleteTarget = null">{{ $t('cancel') }}</button>
            <button class="btn danger" @click="doDelete">{{ $t('delete') }}</button>
          </div>
        </div>
      </div>
    </Teleport>
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
import GlobalEnvDialog from '../components/GlobalEnvDialog.vue'
import { useSpringBootStore } from '../stores/springboot'
import type { SpringBootApp } from '@/models/springboot'
import { AppStatus } from '@/models/springboot'

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

// Global env dialog
const showGlobalEnv = ref(false)

// 一键启动分组
const startingGroup = ref(false)
const stoppingGroup = ref(false)
const groupApps = computed(() => {
  if (!activeGroup.value) return []
  return store.apps
    .filter(a => a.group === activeGroup.value)
    .sort((a, b) => a.startup_order - b.startup_order)
})
async function startGroup() {
  if (startingGroup.value) return
  startingGroup.value = true
  try {
    for (const app of groupApps.value) {
      if (app.status === AppStatus.Running) continue
      await store.startApp(app.id)
      await new Promise(r => setTimeout(r, 1000))
    }
  } finally {
    startingGroup.value = false
    store.fetchApps()
  }
}
async function stopGroup() {
  if (stoppingGroup.value) return
  stoppingGroup.value = true
  try {
    // 逆序停止：先停 order 大的
    const sorted = [...groupApps.value].sort((a, b) => b.startup_order - a.startup_order)
    for (const app of sorted) {
      if (app.status !== AppStatus.Running) continue
      await store.stopApp(app.id)
    }
  } finally {
    stoppingGroup.value = false
    store.fetchApps()
  }
}

// Delete confirm
const deleteTarget = ref<SpringBootApp | null>(null)

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

function handleDelete(id: string) {
  const app = store.apps.find(a => a.id === id)
  if (app) deleteTarget.value = app
}

async function doDelete() {
  if (!deleteTarget.value) return
  await store.deleteApp(deleteTarget.value.id)
  deleteTarget.value = null
  store.fetchApps()
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
</style>
