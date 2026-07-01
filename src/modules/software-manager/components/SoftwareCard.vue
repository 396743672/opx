<template>
  <div class="sw-card">
    <div class="sw-card-top">
      <div class="sw-icon">
        <Icon :icon="entry.icon" />
      </div>
      <div class="sw-meta">
        <div class="sw-name">
          {{ entry.name }}
          <span v-if="entry.key === 'jre' || entry.key === 'mysql'" class="sw-tag">LTS</span>
        </div>
        <div class="sw-desc">{{ entry.description }}</div>
      </div>
    </div>
    <div class="sw-installed">
      <span v-if="installedVersion" class="pill">
        <span class="dot"></span> {{ $t('installed') }} {{ installedVersion }}
      </span>
      <span v-if="isDefaultJre" class="sw-default">
        <Icon icon="mdi:star" /> {{ $t('defaultJre') }}
      </span>
    </div>
    <div class="sw-footer">
      <span class="sw-versions">
        {{ $t('versionAvailable', { count: entry.versions.length }) }}
      </span>
      <button
        class="btn primary"
        @click="$emit('install', entry)"
        :disabled="isInstalling"
      >
        <Icon icon="mdi:download" /> {{ $t('install') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useInstallStore } from '../stores/install'
import type { CatalogEntry } from '@/models/software'

const props = defineProps<{
  entry: CatalogEntry
  installedVersion: string | null
  isDefaultJre: boolean
}>()

defineEmits<{
  install: [entry: CatalogEntry]
}>()

const installStore = useInstallStore()

const isInstalling = computed(() => installStore.hasActiveTask(props.entry.key))
</script>

<style scoped>
.sw-card {
  border: 1px solid var(--color-border);
  background: var(--color-card);
  border-radius: var(--radius-lg);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  box-shadow: var(--shadow-card);
  transition: border-color 0.15s;
}
.sw-card:hover {
  border-color: color-mix(in oklch, var(--color-primary) 40%, var(--color-border));
}
.sw-card-top {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}
.sw-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
  font-size: 22px;
}
.sw-meta {
  flex: 1;
  min-width: 0;
}
.sw-name {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
}
.sw-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--color-success) 14%, transparent);
  color: var(--color-success);
}
.sw-desc {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  line-height: 1.4;
}
.sw-installed {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--color-muted-foreground);
  min-height: 18px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: 999px;
  background: color-mix(in oklch, var(--color-success) 10%, transparent);
  color: var(--color-success);
  border: 1px solid color-mix(in oklch, var(--color-success) 25%, transparent);
}
.pill .dot {
  width: 5px;
  height: 5px;
  border-radius: 999px;
  background: var(--color-success);
}
.sw-default {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: 999px;
  font-size: 10px;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
  border: 1px solid color-mix(in oklch, var(--color-primary) 30%, transparent);
}
.sw-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 2px;
}
.sw-versions {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
  transition: background 0.15s;
}
.btn:hover {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
