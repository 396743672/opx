<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-xl font-semibold">{{ t('installedSoftware') }}</h2>
      <button @click="openInstallDialog" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
        {{ t('installNewSoftware') }}
      </button>
    </div>
    <software-list
      :installed-software="installedSoftware"
      :on-start="handleStart"
      :on-stop="handleStop"
      :on-edit-config="handleEditConfig"
      :on-uninstall="handleUninstall"
    />
    <install-dialog
      v-if="showInstallDialog"
      :available="availableSoftware"
      @cancel="closeInstallDialog"
      @install="handleInstall"
    />
    <config-editor
      v-if="showConfigEditor"
      :content="currentConfig"
      @cancel="closeConfigEditor"
      @save="handleSaveConfig"
    />
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import SoftwareList from '../components/SoftwareList.vue'
import InstallDialog from '../components/InstallDialog.vue'
import ConfigEditor from '../components/ConfigEditor.vue'
import { useSoftwareManager } from '../composables/use-software-manager'
import { useAppStore } from '@/stores/app'
import { ref } from 'vue'

const { t } = useI18n()
const appStore = useAppStore()
appStore.setCurrentTitle(t('softwareManagement'))

const {
  availableSoftware,
  installedSoftware,
  loadAvailable,
  loadInstalled,
  installSoftware,
  startSoftware,
  stopSoftware,
  uninstallSoftware,
} = useSoftwareManager()

const showInstallDialog = ref(false)
const showConfigEditor = ref(false)
const currentConfig = ref('')
const currentEditingId = ref<string | null>(null)

function openInstallDialog() {
  showInstallDialog.value = true
}

function closeInstallDialog() {
  showInstallDialog.value = false
}

async function handleInstall(params: { key: string; version: string; install_path: string }) {
  await installSoftware(params)
  closeInstallDialog()
}

function handleEditConfig(id: string) {
  // TODO: load config from backend
  currentEditingId.value = id
  currentConfig.value = ''
  showConfigEditor.value = true
}

function closeConfigEditor() {
  showConfigEditor.value = false
  currentEditingId.value = null
}

async function handleSaveConfig(content: string) {
  // TODO: save config to backend
  closeConfigEditor()
  await loadInstalled()
}

async function handleStart(id: string) {
  await startSoftware(id)
}

async function handleStop(id: string) {
  await stopSoftware(id)
}

async function handleUninstall(id: string) {
  await uninstallSoftware(id)
}
</script>
