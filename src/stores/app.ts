import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useAppStore = defineStore('app', () => {
  const sidebarCollapsed = ref(false)
  const currentTitle = ref('OPX')

  const toggleSidebar = () => {
    sidebarCollapsed.value = !sidebarCollapsed.value
  }

  return {
    sidebarCollapsed,
    currentTitle,
    toggleSidebar,
    setCurrentTitle: (title: string) => currentTitle.value = title,
  }
})
