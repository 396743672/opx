<template>
  <div class="fixed inset-0 z-[70] flex items-center justify-center bg-background text-foreground p-6">
    <div class="w-full max-w-sm rounded-2xl border border-border bg-card shadow-card p-7 flex flex-col gap-5">
      <div class="flex flex-col items-center gap-2">
        <div class="flex items-center justify-center w-12 h-12 rounded-xl bg-primary/10 text-primary">
          <Icon icon="mdi:lock-outline" class="text-2xl" />
        </div>
        <h1 class="text-lg font-semibold tracking-tight">{{ $t('lockLockedTitle') }}</h1>
        <p class="text-sm text-muted-foreground text-center">{{ $t('lockLockedSubtitle') }}</p>
      </div>
      <form @submit.prevent="onSubmit" class="flex flex-col gap-3">
        <input
          ref="pwInput"
          v-model="password"
          type="password"
          :placeholder="$t('lockPasswordPlaceholder')"
          class="h-10 px-3 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary font-mono"
          :disabled="busy"
        />
        <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
        <button type="submit" class="btn h-10" :disabled="busy || !password">
          {{ busy ? $t('loading') : $t('lockUnlock') }}
        </button>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { useLockStore } from '@/stores/lock'

const { t } = useI18n()
const lockStore = useLockStore()
const password = ref('')
const error = ref('')
const busy = ref(false)
const pwInput = ref<HTMLInputElement | null>(null)

onMounted(() => {
  pwInput.value?.focus()
})

async function onSubmit() {
  if (!password.value || busy.value) return
  busy.value = true
  error.value = ''
  try {
    const ok = await lockStore.unlock(password.value)
    if (!ok) error.value = t('lockIncorrect')
  } catch (e) {
    error.value =
      typeof e === 'string' ? e : e instanceof Error ? e.message : t('lockIncorrect')
  } finally {
    busy.value = false
    password.value = ''
  }
}
</script>
