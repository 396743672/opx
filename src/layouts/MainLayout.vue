<template>
  <div class="app-container">
    <div class="sidebar" :class="{ collapsed: sidebarCollapsed }">
      <Sidebar />
    </div>
    <div class="main-content">
      <Header />
      <router-view />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import Sidebar from '@/components/Sidebar.vue'
import Header from '@/components/Header.vue'

const appStore = useAppStore()

const sidebarCollapsed = computed(() => appStore.sidebarCollapsed)
</script>

<style scoped>
.app-container {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.sidebar {
  width: 250px;
  background: var(--card);
  border-right: 1px solid var(--border);
  transition: width 0.3s;
}

.sidebar.collapsed {
  width: 60px;
}

.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>