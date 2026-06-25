<template>
  <header class="header-container">
    <div class="header-left">
      <button @click="toggleSidebar" class="menu-btn">
        ☰
      </button>
      <h1 class="page-title">{{ $t(currentTitle) }}</h1>
    </div>
    <div class="header-right">
      <ThemeToggle />
      <LanguageToggle />
      <UserMenu />
    </div>
  </header>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useI18n } from 'vue-i18n'
import ThemeToggle from './ThemeToggle.vue'
import LanguageToggle from './LanguageToggle.vue'
import UserMenu from './UserMenu.vue'

const appStore = useAppStore()
const { t } = useI18n()

const currentTitle = computed(() => appStore.currentTitle)

const toggleSidebar = () => {
  appStore.toggleSidebar()
}
</script>

<style scoped>
.header-container {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 2rem;
  background: var(--card);
  border-bottom: 1px solid var(--border);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.menu-btn {
  background: none;
  border: none;
  font-size: 1.5rem;
  cursor: pointer;
  padding: 0.5rem;
}

.page-title {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 600;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 1rem;
}
</style>