<template>
  <div class="space-y-4">
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      <CpuCard v-if="systemInfo" :cpu-usage="systemInfo.cpu_usage" />
      <MemoryCard
        v-if="systemInfo"
        :memory-used="systemInfo.memory_used"
        :memory-total="systemInfo.memory_total"
        :memory-usage="systemInfo.memory_usage"
      />
    </div>
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <DiskCard v-if="systemInfo" :disks="systemInfo.disks" />
      <ProcessTable
        :processes="processList"
        :on-refresh="loadProcessList"
        :on-kill="killPid"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import CpuCard from '../components/CpuCard.vue'
import MemoryCard from '../components/MemoryCard.vue'
import DiskCard from '../components/DiskCard.vue'
import ProcessTable from '../components/ProcessTable.vue'
import { useSystemMonitor } from '../composables/use-system-monitor'
import { useAppStore } from '@/stores/app'

const { t } = useI18n()
const appStore = useAppStore()
appStore.setCurrentTitle(t('systemMonitor'))

const { systemInfo, processList, loadProcessList, killPid } = useSystemMonitor()
</script>