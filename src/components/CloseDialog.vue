<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
    @click.self="$emit('cancel')"
  >
    <div class="w-[360px] rounded-lg border border-border bg-card shadow-popover p-5">
      <h2 class="text-base font-semibold mb-4">{{ $t('closeDialogTitle') }}</h2>

      <div class="space-y-2 mb-4">
        <button
          class="w-full flex items-center gap-3 px-3 py-2.5 rounded-md border text-left transition-colors cursor-pointer"
          :class="choice === 'tray'
            ? 'border-primary bg-primary/10 text-foreground'
            : 'border-border hover:bg-muted'"
          @click="choice = 'tray'"
        >
          <Icon icon="mdi:window-close" class="text-xl text-primary" />
          <span class="text-sm font-medium">{{ $t('closeToTray') }}</span>
        </button>

        <button
          class="w-full flex items-center gap-3 px-3 py-2.5 rounded-md border text-left transition-colors cursor-pointer"
          :class="choice === 'exit'
            ? 'border-destructive bg-destructive/10 text-foreground'
            : 'border-border hover:bg-muted'"
          @click="choice = 'exit'"
        >
          <Icon icon="mdi:power" class="text-xl text-destructive" />
          <span class="text-sm font-medium">{{ $t('exitProgram') }}</span>
        </button>
      </div>

      <p
        v-if="choice === 'exit'"
        class="text-xs text-muted-foreground mb-4 flex items-start gap-1.5"
      >
        <Icon icon="mdi:information-outline" class="text-sm mt-0.5 flex-shrink-0" />
        {{ $t('exitHint') }}
      </p>

      <label class="flex items-center gap-2 mb-4 text-xs text-muted-foreground cursor-pointer select-none">
        <input v-model="remember" type="checkbox" class="accent-primary" />
        {{ $t('rememberChoice') }}
      </label>

      <div class="flex justify-end gap-2">
        <button
          class="px-3 py-1.5 text-sm rounded-md hover:bg-muted transition-colors cursor-pointer"
          @click="$emit('cancel')"
        >
          {{ $t('cancel') }}
        </button>
        <button
          class="px-3 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors cursor-pointer"
          @click="$emit('choose', choice, remember)"
        >
          {{ $t('confirm') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'

useI18n()

interface Props {
  defaultChoice?: 'tray' | 'exit'
}
const props = withDefaults(defineProps<Props>(), {
  defaultChoice: 'tray',
})

const choice = ref<'tray' | 'exit'>(props.defaultChoice)
const remember = ref(false)

defineEmits<{
  choose: [choice: 'tray' | 'exit', remember: boolean]
  cancel: []
}>()
</script>