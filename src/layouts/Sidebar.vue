<template>
  <div
    class="h-full border-r border-border transition-all duration-300"
    :class="sidebarCollapsed ? 'w-16' : 'w-64'"
  >
    <div class="p-4 border-b border-border flex items-center justify-between">
      <h1 v-show="!sidebarCollapsed" class="text-xl font-bold">OPX</h1>
      <button @click="toggleSidebar" class="p-2 rounded-md hover:bg-accent">
        <span class="iconify" data-icon="mdi:menu"></span>
      </button>
    </div>
    <nav class="p-2">
      <RouterLink
        v-for="item in menuItems"
        :key="item.name"
        :to="item.path"
        class="flex items-center gap-3 px-3 py-2 rounded-md mb-1 hover:bg-accent transition-colors"
        :class="{ 'bg-primary/10 text-primary': $route.path === item.path }"
      >
        <span class="iconify" :data-icon="item.icon"></span>
        <span v-show="!sidebarCollapsed" class="font-medium">
          {{ $t(item.meta.title) }}
        </span>
      </RouterLink>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const appStore = useAppStore()
const route = useRoute()
const { sidebarCollapsed, toggleSidebar } = appStore

const menuItems = [
  {
    path: '/dashboard',
    name: 'dashboard',
    icon: 'mdi:gauge',
    meta: { title: 'systemMonitor' }
  },
  {
    path: '/software',
    name: 'software',
    icon: 'mdi:package-variant',
    meta: { title: 'softwareManagement' }
  },
  {
    path: '/repository',
    name: 'repository',
    icon: 'mdi:store',
    meta: { title: 'softwareRepository' }
  },
  {
    path: '/springboot',
    name: 'springboot',
    icon: 'mdi:spring-boot',
    meta: { title: 'springBoot' }
  },
  {
    path: '/settings',
    name: 'settings',
    icon: 'mdi:cog',
    meta: { title: 'settings' }
  },
]
</script>

<style scoped>
</style>