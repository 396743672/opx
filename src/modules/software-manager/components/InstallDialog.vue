<template>
  <dialog open>
    <h3 class="text-lg font-semibold mb-4">{{ t('installNewSoftware') }}</h3>
    <div class="space-y-4">
      <div>
        <label class="block text-sm font-medium mb-1">{{ t('name') }}</label>
        <select v-model="selectedKey" class="w-full rounded-md border border-border px-3 py-2">
          <option v-for="sw in available" :key="sw.key" :value="sw.key">
            {{ sw.name }}
          </option>
        </select>
      </div>
      <div>
        <label class="block text-sm font-medium mb-1">{{ t('version') }}</label>
        <select v-model="selectedVersion" class="w-full rounded-md border border-border px-3 py-2">
          <option v-for="v in availableVersions" :key="v" :value="v">
            {{ v }}
          </option>
        </select>
      </div>
      <div>
        <label class="block text-sm font-medium mb-1">{{ t('installPath') }}</label>
        <input v-model="installPath" type="text" class="w-full rounded-md border border-border px-3 py-2" />
      </div>
    </div>
    <div class="flex justify-end gap-2 mt-6">
      <button @click="onCancel" class="px-4 py-2 rounded-md bg-secondary">
        {{ t('cancel') }}
      </button>
      <button @click="handleInstall" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
        {{ t('install') }}
      </button>
    </div>
  </dialog>
</template>

<script setup lang="ts">
import type { SoftwareMeta } from '@/models/software'
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  available: SoftwareMeta[]
  onCancel: () => void
  onInstall: (params: { key: string; version: string; install_path: string }) => Promise<void>
}>(), {})

const { t } = useI18n()
const selectedKey = ref('')
const selectedVersion = ref('')
const installPath = ref('')

const availableVersions = computed(() => {
  const sw = available.find(s => s.key === selectedKey.value)
  return sw?.available_versions || []
})

// set default version when software changes
watch(selectedKey, (key) => {
  const sw = available.find(s => s.key === key)
  if (sw) {
    selectedVersion.value = sw.default_version
  }
})

const handleInstall = async () => {
  await onInstall({
    key: selectedKey.value,
    version: selectedVersion.value,
    install_path: installPath.value || `apps/${selectedKey.value}`,
  })
  onCancel()
}
</script>
