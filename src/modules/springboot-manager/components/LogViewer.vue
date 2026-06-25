<template>
  <card>
    <card-header class="flex items-center justify-between">
      <h3 class="font-semibold">{{ $t('viewLogs') }}</h3>
      <button @click="refresh" class="px-3 py-1 rounded-md bg-primary/10 hover:bg-primary/20">
        {{ $t('refresh') }}
      </button>
    </card-header>
    <card-content>
      <div class="relative">
        <pre class="max-h-96 overflow-auto p-3 bg-secondary rounded-md text-xs font-mono">
          {{ logs }}
        </pre>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'

withDefaults(defineProps<{
  appId: string
  logPath: string
}>(), {})

const { t } = useI18n()
const logs = ref('')

async function refresh() {
  try {
    const result = await invoke<string>('get_application_logs', { appId: props.appId, logPath: props.logPath })
    logs.value = result
  } catch (e) {
    console.error(e)
    logs.value = `Failed to load logs: ${e}`
  }
}

onMounted(() => {
  refresh()
})
</script>