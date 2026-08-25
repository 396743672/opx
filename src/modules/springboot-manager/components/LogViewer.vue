<template>
  <Teleport to="body">
    <div class="dialog-overlay">
      <div class="dialog-panel">
        <div class="dialog-hd">
          <h2>{{ $t('viewLogs') }} - {{ appName }}</h2>
          <div class="flex items-center gap-2">
            <input v-model="keyword" class="search-input" :placeholder="$t('keyword')" />
            <label class="chk"><input type="checkbox" v-model="realtime" @change="onRealtimeToggle" /> {{ $t('realtime') }}</label>
            <label class="chk"><input type="checkbox" v-model="autoScroll" /> {{ $t('autoScroll') }}</label>
            <button class="btn" @click="loadTail"><Icon icon="mdi:refresh" /> {{ $t('refresh') }}</button>
            <button class="btn" @click="$emit('close')"><Icon icon="mdi:close" /></button>
          </div>
        </div>
        <div class="log-box" ref="logBox" @scroll.passive="onScroll">
          <div v-if="error" class="p-4 text-red-400 font-sans">{{ error }}</div>
          <div v-else-if="lines.length === 0" class="p-4 text-gray-500 font-sans">{{ $t('noLogs') }}</div>
          <div v-else-if="filtered.length === 0" class="p-4 text-gray-500 font-sans">{{ $t('noMatches') }}</div>
          <div v-for="item in filtered" :key="item.idx" class="log-line" v-html="highlight(item.raw)"></div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ appId: string; appName: string; logPath: string }>()
defineEmits<{ close: [] }>()

const lines = ref<string[]>([])
const error = ref('')
const keyword = ref('')
const realtime = ref(true)
const autoScroll = ref(true)
let timer: ReturnType<typeof setInterval> | null = null
let offset = 0
const logBox = ref<HTMLElement | null>(null)

// 搜索过滤：保留稳定原始索引作 key，避免过滤时行错位闪烁
const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase()
  if (!kw) return lines.value.map((raw, idx) => ({ raw, idx }))
  return lines.value
    .map((raw, idx) => ({ raw, idx, lower: raw.toLowerCase() }))
    .filter((x) => x.lower.includes(kw))
    .map(({ raw, idx }) => ({ raw, idx }))
})

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function highlight(raw: string): string {
  const escaped = escapeHtml(raw)
  const kw = keyword.value.trim()
  if (!kw) return escaped
  try {
    return escaped.replace(
      new RegExp(escapeRegExp(escapeHtml(kw)), 'gi'),
      (m) => `<mark class="hl">${m}</mark>`,
    )
  } catch {
    return escaped
  }
}

function onRealtimeToggle() {
  if (realtime.value) startPolling()
  else stopPolling()
}

function startPolling() {
  if (timer) return
  timer = setInterval(loadTail, 2000)
}

function stopPolling() {
  if (timer) { clearInterval(timer); timer = null }
}

// 用户手动上翻/下滚时关闭自动跟随，避免新日志强制把视口拉回底部
function onScroll() {
  const el = logBox.value
  if (!el) return
  if (el.scrollTop + el.clientHeight < el.scrollHeight - 8) autoScroll.value = false
}

async function loadTail() {
  error.value = ''
  if (!props.logPath) { error.value = '日志路径未配置'; return }
  try {
    const chunk = await invoke<{ lines: string[], offset: number }>('read_springboot_log', { path: props.logPath, offset })
    if (offset === 0) lines.value = chunk.lines
    else lines.value.push(...chunk.lines)
    offset = chunk.offset
    // 自动跟随：勾选 autoScroll 时滚动到底（不依赖 watch——push 不触发 ref 替换 watch）
    if (autoScroll.value) {
      await nextTick()
      if (logBox.value) logBox.value.scrollTop = logBox.value.scrollHeight
    }
  } catch (e: any) {
    error.value = '无法读取日志: ' + (typeof e === 'string' ? e : (e?.message || ''))
  }
}

async function firstLoad() {
  await loadTail()
  startPolling()
}
onMounted(firstLoad)
onBeforeUnmount(() => { if (timer) clearInterval(timer) })
</script>

<style scoped>
.dialog-overlay {
  position: fixed; inset: 0; z-index: 50;
  display: flex; align-items: center; justify-content: center;
  background: oklch(0 0 0 / 0.5);
}
.dialog-panel {
  width: 90vw; max-width: 900px; max-height: 80vh;
  border-radius: 10px; border: 1px solid var(--color-border);
  background: var(--color-card); box-shadow: var(--shadow-popover);
  display: flex; flex-direction: column; overflow: hidden;
}
.dialog-hd {
  display: flex; align-items: center; justify-content: space-between;
  padding: 14px 20px; border-bottom: 1px solid var(--color-border);
}
.dialog-hd h2 { font-size: 15px; font-weight: 600; margin: 0; }
.log-box {
  background: #0d1117; color: #58a6ff;
  font-family: ui-monospace, monospace; font-size: 12px;
  padding: 16px; overflow-y: auto; flex: 1; min-height: 50vh;
  white-space: pre-wrap; word-break: break-all;
}
.log-line { line-height: 1.4; }
.log-line .hl {
  background: rgba(255, 200, 0, 0.35);
  color: #ffd54a;
  border-radius: 2px;
  padding: 0 1px;
}
.search-input {
  height: 28px; padding: 0 10px; width: 180px;
  border-radius: 6px; font-size: 12px;
  border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground);
  outline: none;
}
.search-input:focus { border-color: var(--color-primary); }
.chk {
  display: inline-flex; align-items: center; gap: 4px;
  font-size: 12px; color: var(--color-muted-foreground);
  cursor: pointer; user-select: none;
}
.chk input { cursor: pointer; }
.btn {
  display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 10px;
  border-radius: 6px; cursor: pointer; font-size: 12px;
  border: 1px solid var(--color-border); background: var(--color-card);
  color: var(--color-foreground);
}
.btn:hover { background: var(--color-muted); }
</style>
