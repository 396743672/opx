<template>
  <div class="sidebar-container">
    <div class="sidebar-header">
      <h2 v-if="!collapsed">OPX</h2>
      <button @click="toggleSidebar" class="toggle-btn">
        {{ collapsed ? '>' : '<' }}
      </button>
    </div>
    <nav class="sidebar-nav">
      <router-link
        v-for="item in menuItems"
        :key="item.path"
        :to="item.path"
        class="nav-item"
        :class="{ active: $route.path === item.path }"
      >
        <span class="nav-icon">{{ item.icon }}</span>
        <span v-if="!collapsed" class="nav-text">{{ $t(item.label) }}</span>
      </router-link>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from 'vue-i18n'

const appStore = useAppStore()
const { t } = useI18n()

const collapsed = computed(() => appStore.sidebarCollapsed)

const toggleSidebar = () => {
  appStore.toggleSidebar()
}

const menuItems = [
  { path: '/dashboard', icon: '📊', label: 'systemMonitor' },
  { path: '/software', icon: '💻', label: 'softwareManagement' },
  { path: '/repository', icon: '📦', label: 'softwareRepository' },
  { path: '/springboot', icon: '🚀', label: 'springBoot' },
  { path: '/settings', icon: '⚙️', label: 'settings' },
]
</script>

<style scoped>
.sidebar-container {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem;
  border-bottom: 1px solid var(--border);
}

.sidebar-header h2 {
  margin: 0;
  font-size: 1.2rem;
}

.toggle-btn {
  background: none;
  border: none;
  font-size: 1rem;
  cursor: pointer;
  padding: 0.5rem;
}

.sidebar-nav {
  flex: 1;
  padding: 1rem 0;
}

.nav-item {
  display: flex;
  align-items: center;
  padding: 0.75rem 1rem;
  color: var(--foreground);
  text-decoration: none;
  transition: background 0.2s;
  gap: 0.75rem;
}

.nav-item:hover {
  background: var(--background);
}

.nav-item.active {
  background: var(--border);
  font-weight: 500;
}

.nav-icon {
  font-size: 1.2rem;
}

.nav-text {
  flex: 1;
}
</style>