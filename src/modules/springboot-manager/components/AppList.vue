<template>
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
    <app-card
      v-for="app in applications"
      :key="app.id"
      :app="app"
      :on-start="handleStart"
      :on-stop="handleStop"
      :on-restart="handleRestart"
      :on-edit="handleEdit"
      :on-delete="handleDelete"
    />
  </div>
</template>

<script setup lang="ts">
import type { SpringApp } from '@/models/springboot'
import AppCard from './AppCard.vue'
withDefaults(defineProps<{
  applications: SpringApp[]
  onStart: (id: string) => Promise<void>
  onStop: (id: string) => Promise<void>
  onRestart: (id: string) => Promise<void>
  onEdit: (id: string) => void
  onDelete: (id: string) => Promise<void>
}>(), {})

const handleStart = async (id: string) => {
  await onStart(id)
}

const handleStop = async (id: string) => {
  await onStop(id)
}

const handleRestart = async (id: string) => {
  await onRestart(id)
}

const handleEdit = (id: string) => {
  onEdit(id)
}

const handleDelete = async (id: string) => {
  if (window.confirm(t('confirmDelete'))) {
    await onDelete(id)
  }
}

import { useI18n } from 'vue-i18n'
const { t } = useI18n()
</script>