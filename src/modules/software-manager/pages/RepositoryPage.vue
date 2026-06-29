<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:download-box"
      :title="$t('softwareRepository')"
      :subtitle="$t('installNewSoftware')"
    />
    <EmptyState
      icon="mdi:store-search"
      :title="$t('comingSoon')"
      :description="$t('comingSoonDesc')"
    />
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import EmptyState from '@/components/EmptyState.vue'

useI18n()
</script>
