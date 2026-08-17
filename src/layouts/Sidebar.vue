<template>
  <aside
    :class="[
      'absolute left-0 top-0 bottom-0 z-20 flex flex-col overflow-hidden transition-all duration-300 ease-out',
      'glass-sidebar',
      props.pinned ? 'w-52' : 'w-14',
    ]"
  >
    <!-- 侧边栏头部 — 仅留间距，品牌在顶部导航栏 -->
    <div class="h-3 flex-shrink-0"></div>

    <!-- 导航 -->
    <nav class="flex-1 overflow-y-auto overflow-x-hidden px-2 py-3 space-y-3">
      <div
        class="h-px bg-border/60 mx-2 mb-2 transition-opacity duration-200"
        :class="props.pinned ? 'opacity-100' : 'opacity-0'"
      ></div>
      <div v-for="group in groups" :key="group.label">
        <!-- 分组标题（仅展开态显示） -->
        <div
          v-if="props.pinned"
          class="px-2 mb-1.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground/60"
        >
          {{ $t(group.label) }}
        </div>

        <div class="space-y-0.5">
          <div
            v-for="item in group.items"
            :key="item.path"
            :title="!props.pinned ? $t(item.titleKey) : undefined"
            @click="navigate(item.path)"
            :class="[
              'relative flex items-center gap-3 px-2.5 py-2 rounded-lg cursor-pointer transition-all duration-150',
              'text-muted-foreground hover:text-foreground hover:bg-muted/60',
              currentPath === item.path && 'text-primary bg-primary/8 font-medium',
            ]"
          >
            <!-- 激活指示条 -->
            <span
              v-if="currentPath === item.path"
              class="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-4 rounded-r-full bg-primary"
            ></span>
            <Icon :icon="item.icon" class="text-xl flex-shrink-0" />
            <span
              class="text-sm whitespace-nowrap transition-opacity duration-200"
              :class="props.pinned ? 'opacity-100' : 'opacity-0'"
            >
              {{ $t(item.titleKey) }}
            </span>
          </div>
        </div>
      </div>
    </nav>

    <!-- 底部固定区域 -->
    <div class="px-2 pb-3 pt-1 border-t border-border/40">
      <div
        @click="navigate('/settings')"
        :class="[
          'relative flex items-center gap-3 px-2.5 py-2 rounded-lg cursor-pointer transition-all duration-150',
          'text-muted-foreground hover:text-foreground hover:bg-muted/60',
          currentPath === '/settings' && 'text-primary bg-primary/8 font-medium',
        ]"
      >
        <Icon icon="mdi:cog" class="text-xl flex-shrink-0" />
        <span
          class="text-sm whitespace-nowrap transition-opacity duration-200"
          :class="props.pinned ? 'opacity-100' : 'opacity-0'"
        >
          {{ $t('settings') }}
        </span>
      </div>
      <!-- Pin 按钮 -->
      <button
        @click="emit('update:pinned', !props.pinned)"
        class="flex items-center gap-3 w-full px-2.5 py-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-all duration-150 cursor-pointer"
        :title="props.pinned ? $t('unpinSidebar') : $t('pinSidebar')"
      >
        <Icon
          :icon="props.pinned ? 'mdi:pin' : 'mdi:pin-outline'"
          class="text-xl flex-shrink-0"
        />
        <span
          class="text-sm whitespace-nowrap transition-opacity duration-200"
          :class="props.pinned ? 'opacity-100' : 'opacity-0'"
        >
          {{ props.pinned ? $t('unpinSidebar') : $t('pinSidebar') }}
        </span>
      </button>
    </div>
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

const props = defineProps<{ pinned: boolean }>()
const emit = defineEmits<{ 'update:pinned': [value: boolean] }>()

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
      { path: '/repository', titleKey: 'softwareRepository', icon: 'mdi:download-box' },
      { path: '/software', titleKey: 'softwareManagement', icon: 'mdi:package-variant-closed' },
      { path: '/websites', titleKey: 'websiteManagement', icon: 'mdi:web-box' },
      { path: '/springboot', titleKey: 'springBoot', icon: 'mdi:leaf' },
      { path: '/stacks', titleKey: 'stacks', icon: 'mdi:layers-outline' },
    ],
  },
]

const navigate = (path: string) => {
  router.push(path)
}
</script>
