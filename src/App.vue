<template>
  <MainLayout />
</template>

<script setup lang="ts">
import MainLayout from '@/layouts/MainLayout.vue'
import { onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'

const settingsStore = useSettingsStore()

onMounted(async () => {
  await settingsStore.loadSettings()
})
</script>
