<template>
  <card class="h-full">
    <card-header class="flex items-center justify-between">
      <h3 class="font-semibold">{{ $t('processList') }}</h3>
      <button @click="refresh" class="px-3 py-1 text-sm rounded-md bg-primary/10 hover:bg-primary/20">
        {{ $t('refresh') }}
      </button>
    </card-header>
    <card-content>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b">
              <th class="text-left py-2 px-2">PID</th>
              <th class="text-left py-2 px-2">{{ $t('name') }}</th>
              <th class="text-right py-2 px-2">CPU %</th>
              <th class="text-right py-2 px-2">Mem %</th>
              <th class="text-center py-2 px-2">{{ $t('status') }}</th>
              <th class="text-center py-2 px-2">{{ $t('action') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="proc in processes" :key="proc.pid" class="border-b">
              <td class="py-2 px-2 font-mono">{{ proc.pid }}</td>
              <td class="py-2 px-2 font-medium">{{ proc.name }}</td>
              <td class="py-2 px-2 text-right">{{ proc.cpu_usage.toFixed(1) }}</td>
              <td class="py-2 px-2 text-right">{{ proc.memory_usage.toFixed(1) }}</td>
              <td class="py-2 px-2 text-center">{{ proc.status }}</td>
              <td class="py-2 px-2 text-center">
                <button
                  @click="onKill(proc.pid)"
                  class="px-2 py-1 text-xs rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20"
                >
                  {{ $t('killProcess') }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { ProcessInfo } from '@/models/system'
import { useConfirm } from 'vue-climati'
withDefaults(defineProps<{
  processes: ProcessInfo[]
  onRefresh: () => void
  onKill: (pid: number) => Promise<void>
}>(), {})

const { t } = useI18n()
const { confirm } = useConfirm()

async function refresh() {
  onRefresh()
}

async function onKill(pid: number) {
  const ok = await confirm({
    title: t('killProcess'),
    description: t('confirmKillProcess'),
  })
  if (ok) {
    await onKill(pid)
  }
}
</script>