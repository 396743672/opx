<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-xl font-semibold">{{ $t('applicationList') }}</h2>
      <div class="flex gap-2">
        <button @click="startAll" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
          {{ $t('startAll') }}
        </button>
        <button @click="stopAll" class="px-4 py-2 rounded-md bg-secondary">
          {{ $t('stopAll') }}
        </button>
        <button @click="openAddDialog" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
          {{ $t('addApplication') }}
        </button>
      </div>
    </div>
    <app-list
      :applications="applications"
      :on-start="handleStart"
      :on-stop="handleStop"
      :on-restart="handleRestart"
      :on-edit="handleEdit"
      :on-delete="handleDelete"
    />
    <group-config
      :groups="groups"
      :on-delete="deleteGroup"
      @add-group="openAddGroupDialog"
    />
    <edit-dialog
      v-if="showEditDialog"
      :editing="editingApp"
      @cancel="closeEditDialog"
      @save="handleSaveApp"
    />
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import AppList from '../components/AppList.vue'
import EditDialog from '../components/EditDialog.vue'
import GroupConfig from '../components/GroupConfig.vue'
import { useSpringBootManager } from '../composables/use-springboot-manager'
import { useAppStore } from '@/stores/app'
import { ref } from 'vue'
import type { SpringApp } from '@/models/springboot'

const { t } = useI18n()
const appStore = useAppStore()
appStore.setCurrentTitle(t('springBoot'))

const {
  applications,
  groups,
  loadList,
  saveApp,
  deleteApp,
  startApp,
  stopApp,
  restartApp,
  startAll,
  stopAll,
} = useSpringBootManager()

const showEditDialog = ref(false)
const editingApp = ref<SpringApp | null>(null)

function openAddDialog() {
  editingApp.value = null
  showEditDialog.value = true
}

function openEditDialog(app: SpringApp) {
  editingApp.value = app
  showEditDialog.value = true
}

function closeEditDialog() {
  showEditDialog.value = false
  editingApp.value = null
}

async function handleSaveApp(app: SpringApp) {
  await saveApp(app)
  closeEditDialog()
}

async function handleStart(id: string) {
  await startApp(id)
}

async function handleStop(id: string) {
  await stopApp(id)
}

async function handleRestart(id: string) {
  await restartApp(id)
}

function handleEdit(id: string) {
  const app = applications.value.find(a => a.id === id)
  if (app) {
    openEditDialog(app)
  }
}

async function handleDelete(id: string) {
  if (window.confirm(t('confirmDelete'))) {
    await deleteApp(id)
  }
}

function deleteGroup(id: string) {
  // TODO: implement delete group
  console.log('delete group', id)
}

function openAddGroupDialog() {
  // TODO: implement add group dialog
  console.log('open add group dialog')
}
</script>