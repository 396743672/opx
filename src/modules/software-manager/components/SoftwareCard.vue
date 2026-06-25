<template>
  <card>
    <card-header class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h3 class="font-semibold">{{ software.name }}</h3>
        <status-badge :status="software.status" />
      </div>
      <div class="text-sm text-muted-foreground">v{{ software.version }}</div>
    </card-header>
    <card-content>
      <div class="space-y-2 text-sm">
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('port') }}</span>
          <span>{{ software.port }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('installPath') }}</span>
          <span>{{ software.install_path }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('autoStart') }}</span>
          <span>{{ software.auto_start_on_app_start ? '✅' : '❌' }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('startupOrder') }}</span>
          <span>{{ software.startup_order }}</span>
        </div>
      </div>
    </card-content>
    <card-footer class="flex justify-end gap-2">
      <button v-if="software.status === 'Stopped'" @click="onStart" class="px-3 py-1 rounded-md bg-primary text-primary-foreground">
        {{ $t('start') }}
      </button>
      <button v-if="software.status === 'Running'" @click="onStop" class="px-3 py-1 rounded-md bg-secondary">
        {{ $t('stop') }}
      </button>
      <button @click="onEditConfig" class="px-3 py-1 rounded-md bg-secondary">
        {{ $t('editConfig') }}
      </button>
      <button @click="onUninstall" class="px-3 py-1 rounded-md bg-destructive/10 text-destructive">
        {{ $t('uninstall') }}
      </button>
    </card-footer>
  </card>
</template>

<script setup lang="ts">
import type { InstalledSoftware } from '@/models/software'
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  software: InstalledSoftware
  onStart: (id: string) => Promise<void>
  onStop: (id: string) => Promise<void>
  onEditConfig: (id: string) => void
  onUninstall: (id: string) => Promise<void>
}>(), {})

const { t } = useI18n()
</script>
