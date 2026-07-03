<template>
  <div class="source-tab">
    <div ref="containerRef" class="editor-container" />
    <div class="tab-footer">
      <span class="hint">{{ $t('configEditSourceHint') }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import * as monaco from 'monaco-editor'
import { invoke } from '@tauri-apps/api/core'
import type { InstalledSoftware } from '@/models/software'

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{
  'update:dirty': [boolean]
  'update:content': [string]
}>()

const containerRef = ref<HTMLElement>()
let editor: monaco.editor.IStandaloneCodeEditor | null = null
let originalContent = ''

onMounted(async () => {
  if (!containerRef.value) return
  let content = ''
  try {
    content = await invoke<string>('read_config_source', {
      installedId: props.software.id,
    })
  } catch (e) {
    console.error('read source failed:', e)
  }
  originalContent = content
  editor = monaco.editor.create(containerRef.value, {
    value: content,
    language: detectLanguage(props.software.key),
    theme: 'vs-dark',
    automaticLayout: true,
    minimap: { enabled: false },
    fontSize: 13,
    lineNumbers: 'on',
    scrollBeyondLastLine: false,
  })
  editor.onDidChangeModelContent(() => {
    const value = editor!.getValue()
    emit('update:dirty', value !== originalContent)
    emit('update:content', value)
  })
})

onBeforeUnmount(() => {
  editor?.dispose()
  editor = null
})

function detectLanguage(key: string): string {
  switch (key) {
    case 'mysql':
      return 'ini'
    case 'redis':
      return 'ini'
    case 'nginx':
      return 'plaintext'
    default:
      return 'plaintext'
  }
}

defineExpose({
  getContent: () => editor?.getValue() ?? '',
})
</script>

<style scoped>
.source-tab {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.editor-container {
  height: 320px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  overflow: hidden;
}
.tab-footer {
  display: flex;
  justify-content: flex-end;
  font-size: 11px;
  color: var(--color-muted-foreground);
}
</style>
