<template>
  <Teleport to="body">
    <div class="dialog-overlay">
      <div class="dialog-panel">
        <div class="dialog-hd">
          <h2>{{ $t('viewLogs') }} - {{ appName }}</h2>
          <div class="flex items-center gap-2">
            <input v-model="keyword" class="search-input" :placeholder="$t('keyword')" />
            <input v-model="fromAt" type="datetime-local" class="search-input time" :title="$t('fromTime')" />
            <input v-model="toAt" type="datetime-local" class="search-input time" :title="$t('toTime')" />
            <label class="chk"><input type="checkbox" v-model="useRegex" /> {{ $t('useRegex') }}</label>
            <select
              v-if="showLevel"
              v-model="level"
              class="level-sel"
              :disabled="onlyErrors"
              @change="onLevelChange"
            >
              <option value="">{{ $t('levelAll') }}</option>
              <option v-for="lv in levelOptions" :key="lv" :value="lv">{{ lv }}</option>
            </select>
            <label class="chk"><input type="checkbox" v-model="onlyErrors" /> {{ $t('onlyErrors') }}</label>
            <label class="chk"><input type="checkbox" v-model="realtime" @change="onRealtimeToggle" /> {{ $t('realtime') }}</label>
            <label class="chk"><input type="checkbox" v-model="autoScroll" /> {{ $t('autoScroll') }}</label>
            <button class="btn" :disabled="!sources.length || downloading" @click="download">
              <Icon icon="mdi:download" /> {{ $t('backupDownload') }}
            </button>
            <button class="btn" @click="loadTail"><Icon icon="mdi:refresh" /> {{ $t('refresh') }}</button>
            <button class="btn" @click="$emit('close')"><Icon icon="mdi:close" /></button>
          </div>
        </div>

        <!-- 源 tab（按 level 多源） -->
        <div v-if="sources.length" class="source-tabs">
          <button
            v-for="(s, idx) in sources"
            :key="idx"
            class="source-tab"
            :class="{ active: idx === activeSource }"
            @click="selectSource(idx)"
          >
            <Icon icon="mdi:file" />
            {{ s.label }}
            <span v-if="s.has_levels" class="lvl-badge" :title="$t('level')">L</span>
          </button>
        </div>

        <div class="log-box" ref="logBox" @scroll.passive="onScroll">
          <div v-if="error" class="p-4 text-red-400 font-sans">{{ error }}</div>
          <div v-else-if="!sources.length && !loading" class="p-4 text-gray-500 font-sans">{{ $t('noLogs') }}</div>
          <div v-else-if="loading" class="p-4 text-gray-500 font-sans">{{ $t('loading') }}</div>
          <template v-else>
            <button
              v-if="hasMore"
              class="history-btn"
              :disabled="loadingHistory"
              @click="loadHistory"
            >
              ↑ {{ $t('loadEarlier') }}
            </button>
            <div v-if="filtered.length === 0" class="p-4 text-gray-500 font-sans">{{ $t('noMatches') }}</div>
            <div v-for="item in filtered" :key="item.idx" class="log-line" v-html="highlight(item.raw)"></div>
            <div v-if="truncated" class="p-2 text-xs text-gray-400 font-sans">{{ $t('logTruncated') }}</div>
          </template>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { save } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import type { LogSource } from '@/models/software'

const { t } = useI18n()

const props = defineProps<{ appId: string; appName: string; logPath?: string }>()
defineEmits<{ close: [] }>()

const sources = ref<LogSource[]>([])
const activeSource = ref(0)
const lines = ref<string[]>([])
const error = ref('')
const keyword = ref('')
const useRegex = ref(false)
const level = ref('')
const onlyErrors = ref(false)
const downloading = ref(false)
const fromAt = ref('')
const toAt = ref('')
const realtime = ref(true)
const autoScroll = ref(true)
const loading = ref(false)
const loadingHistory = ref(false)
let timer: ReturnType<typeof setInterval> | null = null
let lastEndOffset: number | null = null
const startOffset = ref(0)
const archiveIndex = ref(0)
const hasMore = ref(false)
const truncated = ref(false)
const logBox = ref<HTMLElement | null>(null)

const TAIL_LIMIT = 2000
const MAX_BUFFER = 20000

const levelOptions = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE', 'FATAL', 'PANIC', 'CRITICAL', 'NOTICE']
const showLevel = computed(() => sources.value[activeSource.value]?.has_levels ?? false)
const effectiveLevel = computed(() => (onlyErrors.value ? 'ERROR' : level.value || null))
function onLevelChange() {
  if (level.value) onlyErrors.value = false
}

function parseLogTs(line: string): number | null {
  const m = line.match(/(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})/)
  if (!m) return null
  const d = new Date(`${m[1]}T${m[2]}`)
  return isNaN(d.getTime()) ? null : d.getTime()
}
// 搜索过滤：关键字 + 时间范围（保留稳定原始索引作 key，避免过滤时行错位闪烁）
const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase()
  const from = fromAt.value ? new Date(fromAt.value).getTime() : null
  const to = toAt.value ? new Date(toAt.value).getTime() : null
  return lines.value
    .map((raw, idx) => ({ raw, idx, lower: raw.toLowerCase(), ts: parseLogTs(raw) }))
    .filter((x) => {
      if (kw && !x.lower.includes(kw)) return false
      if ((from != null || to != null) && x.ts == null) return false
      if (from != null && x.ts! < from) return false
      if (to != null && x.ts! > to) return false
      return true
    })
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
function onScroll() {
  const el = logBox.value
  if (!el) return
  if (el.scrollTop + el.clientHeight < el.scrollHeight - 8) autoScroll.value = false
}

// notify 实时监听：注册当前源主文件，变化事件触发增量读取（轮询保留为兜底）
const watchId = ref<number | null>(null)
const unlistenFn = ref<UnlistenFn | null>(null)
async function registerWatch(path: string) {
  if (watchId.value != null) await invoke('unwatch_log_file', { id: watchId.value }).catch(() => {})
  watchId.value = null
  if (unlistenFn.value) {
    unlistenFn.value()
    unlistenFn.value = null
  }
  try {
    watchId.value = await invoke<number>('watch_log_file', { path })
    unlistenFn.value = await listen<{ id: number }>('log-file-changed', (e) => {
      if (e.payload.id === watchId.value) loadTail()
    })
  } catch {
    // 监听失败静默，轮询兜底
  }
}

async function loadSources() {
  stopPolling()
  loading.value = true
  error.value = ''
  sources.value = []
  activeSource.value = 0
  lastEndOffset = null
  archiveIndex.value = 0
  hasMore.value = false
  lines.value = []
  try {
    sources.value = await invoke<LogSource[]>('list_springboot_log_sources', { appId: props.appId })
    if (sources.value.length) {
      await loadTail()
      await registerWatch(sources.value[activeSource.value].path)
      if (realtime.value) startPolling()
    }
  } catch (e: any) {
    error.value = '无法读取日志: ' + (typeof e === 'string' ? e : (e?.message || ''))
  } finally {
    loading.value = false
  }
}

async function loadTail() {
  if (!sources.value.length) return
  error.value = ''
  try {
    const chunk = await invoke<any>('read_springboot_log', {
      appId: props.appId,
      sourceIndex: activeSource.value,
      archiveIndex: 0,
      offset: lastEndOffset, // 首次 null → tail 尾部；之后 end_offset → 仅读新增行（实时轮询增量）
      before: false,
      limit: TAIL_LIMIT,
      keyword: keyword.value.trim() || null,
      regex: useRegex.value,
      level: effectiveLevel.value,
    })
    if (lastEndOffset === null) {
      lines.value = chunk.lines
    } else if (chunk.lines.length) {
      lines.value.push(...chunk.lines)
      if (lines.value.length > MAX_BUFFER) {
        lines.value = lines.value.slice(lines.value.length - MAX_BUFFER)
      }
    }
    lastEndOffset = chunk.end_offset ?? lastEndOffset
    startOffset.value = chunk.start_offset
    hasMore.value = chunk.has_more
    truncated.value = chunk.truncated
    if (autoScroll.value) {
      await nextTick()
      if (logBox.value) logBox.value.scrollTop = logBox.value.scrollHeight
    }
  } catch (e: any) {
    error.value = '无法读取日志: ' + (typeof e === 'string' ? e : (e?.message || ''))
  }
}

async function loadHistory() {
  if (!hasMore.value) return
  loadingHistory.value = true
  try {
    const chunk = await invoke<any>('read_springboot_log', {
      appId: props.appId,
      sourceIndex: activeSource.value,
      archiveIndex: archiveIndex.value,
      offset: startOffset.value,
      before: true,
      limit: TAIL_LIMIT,
      keyword: keyword.value.trim() || null,
      regex: useRegex.value,
      level: effectiveLevel.value,
    })
    lines.value = [...chunk.lines, ...lines.value]
    startOffset.value = chunk.start_offset
    archiveIndex.value = chunk.archive_index ?? archiveIndex.value
    hasMore.value = chunk.has_more
    truncated.value = chunk.truncated
  } catch (e: any) {
    error.value = '无法读取日志: ' + (typeof e === 'string' ? e : (e?.message || ''))
  } finally {
    loadingHistory.value = false
  }
}

async function download() {
  if (!sources.value.length) return
  downloading.value = true
  try {
    const src = sources.value[activeSource.value]
    const base = src.path.split(/[\\/]/).pop() ?? 'log.txt'
    const dest = await save({ defaultPath: base, title: t('backupDownload') })
    if (!dest) return
    await invoke('download_springboot_log', {
      appId: props.appId,
      sourceIndex: activeSource.value,
      destPath: dest,
    })
  } catch (e: any) {
    error.value = '下载失败: ' + (typeof e === 'string' ? e : (e?.message || ''))
  } finally {
    downloading.value = false
  }
}

async function selectSource(idx: number) {
  if (activeSource.value === idx) return
  stopPolling()
  activeSource.value = idx
  lastEndOffset = null
  archiveIndex.value = 0
  hasMore.value = false
  lines.value = []
  await loadTail()
  const src = sources.value[idx]
  if (src) await registerWatch(src.path)
  if (realtime.value) startPolling()
}

async function firstLoad() {
  await loadSources()
}
onMounted(firstLoad)
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
  if (watchId.value != null) invoke('unwatch_log_file', { id: watchId.value }).catch(() => {})
  if (unlistenFn.value) unlistenFn.value()
})
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
.source-tabs {
  display: flex; gap: 4px; padding: 8px 12px 0;
  border-bottom: 1px solid var(--color-border);
}
.source-tab {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 6px 12px; border-radius: 6px 6px 0 0;
  font-size: 12px; cursor: pointer;
  color: var(--color-muted-foreground);
  background: transparent; border: none;
}
.source-tab.active {
  background: var(--color-muted); color: var(--color-primary); font-weight: 500;
}
.lvl-badge {
  font-size: 9px; padding: 0 4px; border-radius: 999px;
  background: color-mix(in oklch, var(--color-primary) 15%, transparent);
  color: var(--color-primary);
}
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
.history-btn {
  display: inline-block; margin-bottom: 8px;
  padding: 4px 10px; border-radius: 6px;
  cursor: pointer; font-size: 12px;
  border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground);
}
.search-input {
  height: 28px; padding: 0 10px; width: 180px;
  border-radius: 6px; font-size: 12px;
  border: 1px solid var(--color-border);
  background: var(--color-card); color: var(--color-foreground);
  outline: none;
}
.search-input:focus { border-color: var(--color-primary); }
.level-sel {
  height: 28px; padding: 0 8px; border-radius: 6px; font-size: 12px;
  border: 1px solid var(--color-border); background: var(--color-card); color: var(--color-foreground); outline: none;
}
.level-sel:focus { border-color: var(--color-primary); }
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