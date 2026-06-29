<template>
  <aside
    :class="[
      'border-r border-border bg-card flex flex-col transition-all duration-300 overflow-hidden flex-shrink-0',
      collapsed ? 'w-14' : 'w-56',
    ]"
  >
    <nav class="flex-1 overflow-y-auto overflow-x-hidden py-3">
      <div v-for="group in groups" :key="group.label" :class="collapsed ? 'mb-3' : 'mb-4'">
        <!-- 分组标题（展开态） -->
        <div
          v-if="!collapsed"
          class="px-4 mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {{ $t(group.label) }}
        </div>

        <!-- 导航项 -->
        <div
          v-for="item in group.items"
          :key="item.path"
          :title="collapsed ? $t(item.titleKey) : undefined"
          @click="navigate(item.path)"
          :class="[
            'relative flex items-center gap-3 mx-2 px-3 py-2 rounded-md cursor-pointer transition-colors mb-0.5',
            collapsed && 'justify-center px-0',
            currentPath === item.path
              ? 'text-primary bg-primary/10 font-medium'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted',
          ]"
        >
          <!-- 激活指示条 -->
          <span
            v-if="currentPath === item.path"
            class="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 rounded-r-full bg-primary"
          ></span>
          <Icon :icon="item.icon" class="text-xl flex-shrink-0" />
          <span v-show="!collapsed" class="text-sm whitespace-nowrap">{{
            $t(item.titleKey)
          }}</span>
        </div>
      </div>
    </nav>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'

useI18n()
const router = useRouter()
const route = useRoute()

interface Props {
  collapsed: boolean
}
defineProps<Props>()

const currentPath = computed(() => route.path)

interface NavItem {
  path: string
  titleKey: string
  icon: string
}
interface NavGroup {
  label: string
  items: NavItem[]
}

const groups: NavGroup[] = [
  {
    label: 'monitor',
    items: [{ path: '/dashboard', titleKey: 'systemMonitor', icon: 'mdi:gauge' }],
  },
  {
    label: 'management',
    items: [
      { path: '/software', titleKey: 'softwareManagement', icon: 'mdi:package-variant-closed' },
      { path: '/repository', titleKey: 'softwareRepository', icon: 'mdi:download-box' },
      { path: '/springboot', titleKey: 'springBoot', icon: 'mdi:leaf' },
    ],
  },
  {
    label: 'system',
    items: [{ path: '/settings', titleKey: 'settings', icon: 'mdi:cog' }],
  },
]

const navigate = (path: string) => {
  router.push(path)
}
</script>
