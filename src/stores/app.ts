import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useAppStore = defineStore('app', () => {
  const sidebarCollapsed = ref(false)
  const currentTitle = ref('OPX')

  const toggleSidebar = () => {
    sidebarCollapsed.value = !sidebarCollapsed.value
  }

  const setCurrentTitle = (title: string) => {
    currentTitle.value = title
  }

  return {
    sidebarCollapsed,
    currentTitle,
    toggleSidebar,
    setCurrentTitle
  }
})