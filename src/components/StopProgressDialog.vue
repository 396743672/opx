<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
  >
    <div class="w-[360px] rounded-lg border border-border bg-card shadow-popover p-5">
      <div class="flex items-center gap-3 mb-4">
        <Icon
          :icon="done ? 'mdi:check-circle' : 'mdi:progress-clock'"
          class="text-2xl"
          :class="done ? 'text-success' : 'text-primary'"
        />
        <div>
          <h2 class="text-base font-semibold">
            {{ done ? $t('safelyExited') : $t('stoppingServices') }}
          </h2>
          <p v-if="!done && total > 0" class="text-xs text-muted-foreground tnum">
            {{ current }} / {{ total }}
          </p>
        </div>
      </div>

      <div class="h-2 w-full bg-muted rounded-full overflow-hidden mb-3">
        <div
          class="h-full bg-primary rounded-full transition-all duration-300 ease-out"
          :style="{ width: progressPercent + '%' }"
        ></div>
      </div>

      <p v-if="!done" class="text-xs text-muted-foreground truncate">
        {{ currentName }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

useI18n()

interface StopProgressPayload {
  current: number
  total: number
  name: string
  status: string
}

const current = ref(0)
const total = ref(0)
const currentName = ref('')
const done = ref(false)

const progressPercent = computed(() => {
  if (total.value === 0) return done.value ? 100 : 30
  return Math.round((current.value / total.value) * 100)
})

let unlistenProgress: UnlistenFn | null = null
let unlistenComplete: UnlistenFn | null = null

onMounted(async () => {
  unlistenProgress = await listen<StopProgressPayload>('stop-progress', (e) => {
    current.value = e.payload.current
    total.value = e.payload.total
    currentName.value = e.payload.name
  })
  unlistenComplete = await listen('stop-complete', () => {
    done.value = true
    current.value = total.value
  })
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenComplete?.()
})
</script>