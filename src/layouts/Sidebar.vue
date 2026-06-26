<template>
  <aside
    :class="[
      'border-r border-border bg-card transition-all duration-300 overflow-hidden',
      collapsed ? 'w-12' : 'w-48'
    ]"
  >
    <nav class="py-2">
      <div
        v-for="item in menuItems"
        :key="item.path"
        @click="navigate(item.path)"
        :class="[
          'flex items-center gap-3 px-3 py-2 mx-2 rounded-md cursor-pointer transition-colors mb-1',
          currentPath === item.path
            ? 'bg-primary text-primary-foreground'
            : 'hover:bg-muted'
        ]"
      >
        <iconify-icon :icon="item.icon" class="text-xl flex-shrink-0" />
        <span v-show="!collapsed" class="whitespace-nowrap">{{ $t(item.titleKey) }}</span>
      </div>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'

const router = useRouter()
const route = useRoute()

interface Props {
  collapsed: boolean
}

defineProps<Props>()

const currentPath = computed(() => route.path)

const menuItems = [
  {
    path: '/dashboard',
    titleKey: 'systemMonitor',
    icon: 'mdi:gauge',
  },
  {
    path: '/software',
    titleKey: 'softwareManagement',
    icon: 'mdi:package-variant-closed',
  },
  {
    path: '/repository',
    titleKey: 'softwareRepository',
    icon: 'mdi:download-box',
  },
  {
    path: '/springboot',
    titleKey: 'springBoot',
    icon: 'mdi:leaf',
  },
  {
    path: '/settings',
    titleKey: 'settings',
    icon: 'mdi:cog',
  },
]

const navigate = (path: string) => {
  router.push(path)
}
</script>

<style scoped>
</style>
