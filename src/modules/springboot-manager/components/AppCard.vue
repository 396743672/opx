<template>
  <card class="h-full">
    <card-header class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h3 class="font-semibold">{{ app.name }}</h3>
        <status-badge :status="app.status" />
      </div>
      <div class="text-sm text-muted-foreground">v{{ app.version }}</div>
    </card-header>
    <card-content>
      <div class="space-y-2 text-sm">
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('port') }}</span>
          <span>{{ app.port }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">env</span>
          <span>{{ app.env }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('autoStart') }}</span>
          <span>{{ app.auto_start_on_app_start ? '✅' : '❌' }}</span>
        </div>
        <div class="flex justify-between">
          <span class="text-muted-foreground">{{ $t('startupOrder') }}</span>
          <span>{{ app.startup_order }}</span>
        </div>
      </div>
    </card-content>
    <card-footer class="flex justify-end gap-2">
      <button v-if="app.status === 'Stopped'" @click="onStart" class="px-3 py-1 rounded-md bg-primary text-primary-foreground">
        {{ $t('start') }}
      </button>
      <button v-if="app.status === 'Running'" @click="onStop" class="px-3 py-1 rounded-md bg-secondary">
        {{ $t('stop') }}
      </button>
      <button @click="onRestart" class="px-3 py-1 rounded-md bg-secondary">
        {{ $t('restart') }}
      </button>
      <button @click="onEdit" class="px-3 py-1 rounded-md bg-secondary">
        {{ $t('edit') }}
      </button>
      <button @click="onDelete" class="px-3 py-1 rounded-md bg-destructive/10 text-destructive">
        {{ $t('delete') }}
      </button>
    </card-footer>
  </card>
</template>

<script setup lang="ts">
import type { SpringApp } from '@/models/springboot'
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  app: SpringApp
  onStart: (id: string) => Promise<void>
  onStop: (id: string) => Promise<void>
  onRestart: (id: string) => Promise<void>
  onEdit: (id: string) => void
  onDelete: (id: string) => Promise<void>
}>(), {})

const { t } = useI18n()
</script>