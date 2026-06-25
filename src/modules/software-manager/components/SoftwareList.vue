<template>
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
    <software-card
      v-for="software in installedSoftware"
      :key="software.id"
      :software="software"
      :on-start="handleStart"
      :on-stop="handleStop"
      :on-edit-config="handleEditConfig"
      :on-uninstall="handleUninstall"
    />
  </div>
</template>

<script setup lang="ts">
import type { InstalledSoftware } from '@/models/software'
import SoftwareCard from './SoftwareCard.vue'
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  installedSoftware: InstalledSoftware[]
  onStart: (id: string) => Promise<void>
  onStop: (id: string) => Promise<void>
  onEditConfig: (id: string) => void
  onUninstall: (id: string) => Promise<void>
}>(), {})

const { t } = useI18n()

const handleStart = async (id: string) => {
  await onStart(id)
}

const handleStop = async (id: string) => {
  await onStop(id)
}

const handleEditConfig = (id: string) => {
  onEditConfig(id)
}

const handleUninstall = async (id: string) => {
  if (window.confirm(t('confirmUninstall'))) {
    await onUninstall(id)
  }
}
</script>
