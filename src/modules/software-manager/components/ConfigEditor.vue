<template>
  <dialog open>
    <h3 class="text-lg font-semibold mb-4">{{ t('editConfig') }}</h3>
    <textarea
      v-model="content"
      class="w-full h-64 rounded-md border border-border p-3 font-mono text-sm"
    ></textarea>
    <div class="flex justify-end gap-2 mt-6">
      <button @click="onCancel" class="px-4 py-2 rounded-md bg-secondary">
        {{ t('cancel') }}
      </button>
      <button @click="handleSave" class="px-4 py-2 rounded-md bg-primary text-primary-foreground">
        {{ t('save') }}
      </button>
    </div>
  </dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  content: string
  onCancel: () => void
  onSave: (content: string) => Promise<void>
}>(), {})

const { t } = useI18n()
const content = ref(content)

const handleSave = async () => {
  await onSave(content.value)
  onCancel()
}
</script>
