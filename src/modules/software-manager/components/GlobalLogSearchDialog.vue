<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="head">
          <div class="title">
            <Icon icon="mdi:magnify" /> {{ $t('globalLogSearch') }}
          </div>
          <button class="close" @click="$emit('close')"><Icon icon="mdi:close" /></button>
        </div>

        <div class="search-row">
          <input
            v-model="keyword"
            class="input"
            :placeholder="$t('keyword')"
            @keyup.enter="doSearch"
          />
          <button class="btn primary" :disabled="searching || !keyword.trim()" @click="doSearch">
            <Icon :icon="searching ? 'mdi:loading' : 'mdi:magnify'" :class="{ spinning: searching }" />
            {{ $t('search') }}
          </button>
        </div>

        <div v-if="hits.length" class="meta">
          {{ hits.length }} {{ $t('matches') }}
        </div>
        <div v-else-if="searched && !searching" class="empty">{{ $t('noMatches') }}</div>

        <div class="list" v-if="hits.length">
          <div v-for="(h, i) in hits" :key="i" class="hit" @click="openInstance(h)">
            <div class="hit-top">
              <span class="tag">{{ h.software_name }}</span>
              <span class="src">{{ h.source_label }}</span>
              <span class="open-hint"><Icon icon="mdi:open-in-new" /> {{ $t('viewLogs') }}</span>
            </div>
            <pre class="line">{{ h.line }}</pre>
          </div>
        </div>

        <LogViewerDialog
          v-if="activeInst"
          :software="activeInst"
          @close="activeInst = null"
        />
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import type { LogHit, InstalledSoftware } from '@/models/software'
import LogViewerDialog from './LogViewerDialog.vue'

defineEmits<{ close: [] }>()

const keyword = ref('')
const hits = ref<LogHit[]>([])
const searching = ref(false)
const searched = ref(false)
const installedMap = ref<Record<string, InstalledSoftware>>({})
const activeInst = ref<InstalledSoftware | null>(null)

async function doSearch() {
  if (!keyword.value.trim()) return
  searching.value = true
  searched.value = true
  try {
    hits.value = await invoke<LogHit[]>('search_all_logs', { keyword: keyword.value.trim() })
    // 缓存已装软件表，供命中点击跳转到该实例日志
    const list = await invoke<InstalledSoftware[]>('list_installed_software')
    const map: Record<string, InstalledSoftware> = {}
    for (const s of list) map[s.id] = s
    installedMap.value = map
  } catch {
    hits.value = []
  } finally {
    searching.value = false
  }
}

function openInstance(h: LogHit) {
  const inst = installedMap.value[h.installed_id]
  if (inst) activeInst.value = inst
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.4);
}
.dialog {
  width: 720px;
  max-width: 90vw;
  max-height: 85vh;
  display: flex;
  flex-direction: column;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 16px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
}
.close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  border-radius: 4px;
  cursor: pointer;
}
.close:hover {
  background: var(--color-destructive);
  color: var(--color-destructive-foreground);
}
.search-row {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}
.input {
  flex: 1;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  outline: none;
}
.input:focus {
  border-color: var(--color-primary);
  background: var(--color-card);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 14px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.meta {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.empty {
  padding: 24px;
  text-align: center;
  color: var(--color-muted-foreground);
  font-size: 13px;
}
.list {
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.hit {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 6px 8px;
  background: var(--color-muted);
  cursor: pointer;
  transition: border-color 0.15s;
}
.hit:hover {
  border-color: var(--color-primary);
}
.hit-top {
  display: flex;
  gap: 8px;
  margin-bottom: 4px;
}
.open-hint {
  margin-left: auto;
  font-size: 11px;
  color: var(--color-primary);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.tag {
  font-size: 11px;
  padding: 0 6px;
  border-radius: 999px;
  background: color-mix(in oklch, var(--color-primary) 15%, transparent);
  color: var(--color-primary);
}
.src {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.line {
  margin: 0;
  font-family: ui-monospace, monospace;
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--color-foreground);
}
</style>