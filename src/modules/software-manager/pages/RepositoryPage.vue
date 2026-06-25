<template>
  <div class="space-y-4">
    <h2 class="text-xl font-semibold">{{ $t('softwareRepository') }}</h2>
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      <card v-for="sw in availableSoftware" :key="sw.key" class="h-full">
        <card-header>
          <h3 class="font-semibold">{{ sw.name }}</h3>
        </card-header>
        <card-content>
          <p class="text-sm text-muted-foreground mb-4">{{ sw.description }}</p>
          <div class="space-y-1 text-sm">
            <div class="flex justify-between">
              <span>{{ $t('availableVersions') }}</span>
              <span>{{ sw.available_versions.length }}</span>
            </div>
            <div class="flex justify-between">
              <span>{{ $t('defaultVersion') }}</span>
              <span>{{ sw.default_version }}</span>
            </div>
          </div>
        </card-content>
        <card-footer class="flex justify-end">
          <button
            class="px-4 py-2 rounded-md bg-primary text-primary-foreground"
            @click="goToInstall"
          >
            {{ $t('install') }}
          </button>
        </card-footer>
      </card>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useSoftwareManager } from '../composables/use-software-manager'
import { useAppStore } from '@/stores/app'
import { useRouter } from 'vue-router'

const { t } = useI18n()
const appStore = useAppStore()
appStore.setCurrentTitle(t('softwareRepository'))

const router = useRouter()

const { availableSoftware } = useSoftwareManager()

function goToInstall() {
  router.push({ name: 'software' })
}
</script>
