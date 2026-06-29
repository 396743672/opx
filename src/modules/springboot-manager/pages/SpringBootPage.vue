<template>
  <div class="animate-fade-in">
    <PageHeader
      icon="mdi:leaf"
      :title="$t('springBoot')"
      :subtitle="$t('applicationList')"
    />
    <EmptyState
      icon="mdi:leaf"
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
