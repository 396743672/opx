<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog log-dialog">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon icon="mdi:file-document-outline" />
            {{ $t('logViewer') }}
          </div>
          <button class="dialog-close" @click="$emit('close')">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <!-- 工具栏：实例切换 + 下载 -->
        <div class="toolbar">
          <label class="field">
            <span class="field-label">{{ $t('selectInstance') }}</span>
            <select v-model="selectedId" class="select">
              <option v-for="i in instances" :key="i.id" :value="i.id">
                {{ i.name }} {{ i.version }}
              </option>
            </select>
          </label>
          <button class="btn btn-sm" :disabled="!sources.length || downloading" @click="download">
            <Icon icon="mdi:download" /> {{ $t('backupDownload') }}
          </button>
        </div>

        <!-- 日志源标签 -->
        <div v-if="sources.length" class="source-tabs">
          <button
            v-for="(s, idx) in sources"
            :key="idx"
            class="source-tab"
            :class="{ active: idx === activeSource }"
            @click="activeSource = idx"
          >
            <Icon :icon="s.kind === 'ProviderFile' ? 'mdi:file' : 'mdi:console'" />
            {{ s.kind === 'ProviderFile' ? $t('fileLog') : $t('consoleLog') }}
            <span v-if="s.has_levels" class="lvl-badge" :title="$t('level')">L</span>
          </button>
        </div>

        <!-- 过滤工具栏 -->
        <div class="filters">
          <input v-model="keyword" class="input" :placeholder="$t('keyword')" />
          <label class="chk">
            <input type="checkbox" v-model="useRegex" /> {{ $t('useRegex') }}
          </label>
          <select
            v-if="showLevel"
            v-model="level"
            class="select"
            :disabled="onlyErrors"
            @change="onLevelChange"
          >
            <option value="">{{ $t('levelAll') }}</option>
            <option v-for="lv in levelOptions" :key="lv" :value="lv">{{ lv }}</option>
          </select>
          <label class="chk">
            <input type="checkbox" v-model="onlyErrors" /> {{ $t('onlyErrors') }}
          </label>
          <label class="chk">
            <input type="checkbox" v-model="realtime" @change="onRealtimeToggle" /> {{ $t('realtime') }}
          </label>
          <label class="chk">
            <input type="checkbox" v-model="autoScroll" /> {{ $t('autoScroll') }}
          </label>
        </div>

        <!-- 日志主体 -->
        <div class="log-body" ref="logBody">
          <div v-if="loading" class="log-placeholder">{{ $t('loading') }}</div>
          <div v-else-if="errorMsg && !sources.length" class="log-error">{{ errorMsg }}</div>
          <div v-else-if="!sources.length" class="log-placeholder">{{ $t('noLogSources') }}</div>
          <div v-else>
            <button
              v-if="hasMore"
              class="history-btn"
              :disabled="loadingHistory"
              @click="loadHistory"
            >
              ↑ {{ $t('loadEarlier') }}
            </button>
            <div
              v-for="(line, idx) in lines"
              :key="idx"
              class="log-line"
              :class="lineClass(line)"
            >{{ line }}</div>
            <div v-if="truncated" class="log-note">{{ $t('logTruncated') }}</div>
          </div>
        </div>

        <div class="dialog-footer">
          <span class="meta">{{ totalBytes ? formatBytes(totalBytes) : '' }}</span>
          <span v-if="errorMsg && sources.length" class="meta err">{{ errorMsg }}</span>
          <button class="btn" @click="$emit('close')">{{ $t('close') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from 'vue'
import { Icon } from '@iconify/vue'
import { save } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { useOpsStore } from '../stores/ops'
import { formatBytes } from '@/utils/format'
import type { InstalledSoftware, LogSource } from '@/models/software'

const { t } = useI18n()
const ops = useOpsStore()

const props = defineProps<{
  software: InstalledSoftware
  instances?: InstalledSoftware[]
}>()
const emit = defineEmits<{ close: [] }>()

const POLL_INTERVAL = 1500
const TAIL_LIMIT = 2000
const HISTORY_LIMIT = 2000
const MAX_BUFFER = 20000
const levelOptions = [
  'ERROR',
  'WARN',
  'INFO',
  'DEBUG',
  'TRACE',
  'FATAL',
  'PANIC',
  'CRITICAL',
  'NOTICE',
]

const selectedId = ref(props.software.id)
const sources = ref<LogSource[]>([])
const activeSource = ref(0)
const lines = ref<string[]>([])
const startOffset = ref(0)
const endOffset = ref(0)
const totalBytes = ref(0)
const hasMore = ref(false)
const truncated = ref(false)
const keyword = ref('')
const useRegex = ref(false)
const level = ref('')
const onlyErrors = ref(false)
const realtime = ref(true)
const autoScroll = ref(true)
const loading = ref(false)
const loadingHistory = ref(false)
const downloading = ref(false)
const errorMsg = ref<string | null>(null)
const logBody = ref<HTMLElement | null>(null)

let pollTimer: ReturnType<typeof setInterval> | null = null
let filterDebounce: ReturnType<typeof setTimeout> | null = null

const showLevel = computed(() => sources.value[activeSource.value]?.has_levels ?? false)
const effectiveLevel = computed(() => (onlyErrors.value ? 'ERROR' : level.value || null))

async function loadSources() {
  stopPolling()
  loading.value = true
  errorMsg.value = null
  sources.value = []
  activeSource.value = 0
  lines.value = []
  try {
    sources.value = await ops.getLogSources(selectedId.value)
    if (sources.value.length > 0) {
      await tail()
      if (realtime.value) startPolling()
    }
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    loading.value = false
  }
}

async function tail() {
  if (!sources.value.length) return
  try {
    const chunk = await ops.readLog({
      installedId: selectedId.value,
      sourceIndex: activeSource.value,
      offset: null,
      before: false,
      limit: TAIL_LIMIT,
      keyword: keyword.value,
      regex: useRegex.value,
      level: effectiveLevel.value,
    })
    lines.value = chunk.lines
    startOffset.value = chunk.start_offset
    endOffset.value = chunk.end_offset
    totalBytes.value = chunk.total_bytes
    hasMore.value = chunk.has_more
    truncated.value = chunk.truncated
    await nextTick()
    scrollToBottom()
  } catch (e) {
    errorMsg.value = String(e)
  }
}

async function poll() {
  if (!sources.value.length) return
  try {
    const chunk = await ops.readLog({
      installedId: selectedId.value,
      sourceIndex: activeSource.value,
      offset: endOffset.value,
      before: false,
      limit: TAIL_LIMIT,
      keyword: keyword.value,
      regex: useRegex.value,
      level: effectiveLevel.value,
    })
    if (chunk.lines.length > 0) {
      lines.value = [...lines.value, ...chunk.lines]
      if (lines.value.length > MAX_BUFFER) {
        lines.value = lines.value.slice(lines.value.length - MAX_BUFFER)
      }
      startOffset.value = chunk.start_offset
      endOffset.value = chunk.end_offset
      hasMore.value = chunk.has_more
      if (autoScroll.value) {
        await nextTick()
        scrollToBottom()
      }
    } else {
      endOffset.value = chunk.end_offset
    }
  } catch (e) {
    // 轮询失败静默，避免刷屏；手动操作（tail/history）会直接提示错误
    console.error('log poll failed:', e)
  }
}

async function loadHistory() {
  if (!hasMore.value) return
  loadingHistory.value = true
  try {
    const chunk = await ops.readLog({
      installedId: selectedId.value,
      sourceIndex: activeSource.value,
      offset: startOffset.value,
      before: true,
      limit: HISTORY_LIMIT,
      keyword: keyword.value,
      regex: useRegex.value,
      level: effectiveLevel.value,
    })
    lines.value = [...chunk.lines, ...lines.value]
    startOffset.value = chunk.start_offset
    hasMore.value = chunk.has_more
    truncated.value = chunk.truncated
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    loadingHistory.value = false
  }
}

function startPolling() {
  stopPolling()
  pollTimer = setInterval(poll, POLL_INTERVAL)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

function onRealtimeToggle() {
  if (realtime.value) startPolling()
  else stopPolling()
}

function scrollToBottom() {
  const el = logBody.value
  if (el) el.scrollTop = el.scrollHeight
}

async function download() {
  if (!sources.value.length) return
  downloading.value = true
  try {
    const src = sources.value[activeSource.value]
    const base = src.path.split(/[\\/]/).pop() ?? 'log.txt'
    const dest = await save({ defaultPath: base, title: t('backupDownload') })
    if (!dest) return
    await ops.downloadLog(selectedId.value, activeSource.value, dest)
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    downloading.value = false
  }
}

function onLevelChange() {
  // 选择具体级别时关闭「仅错误」预设（仅错误优先级更高，二者互斥）
  if (level.value) onlyErrors.value = false
}

function lineClass(line: string): string {
  const u = line.toUpperCase()
  if (
    u.includes('FATAL') ||
    u.includes('PANIC') ||
    u.includes('CRITICAL') ||
    u.includes('ERROR') ||
    u.includes('ERR')
  )
    return 'lvl-error'
  if (u.includes('WARN')) return 'lvl-warn'
  if (u.includes('DEBUG')) return 'lvl-debug'
  if (u.includes('TRACE')) return 'lvl-trace'
  if (u.includes('NOTICE')) return 'lvl-notice'
  if (u.includes('INFO')) return 'lvl-info'
  return ''
}

// 实例切换 → 重新加载；日志源切换 → 重新 tail；过滤条件变化 → 防抖后重新 tail
watch(selectedId, () => loadSources())
watch(activeSource, () => tail())
watch([keyword, useRegex, level, onlyErrors], () => {
  if (filterDebounce) clearTimeout(filterDebounce)
  filterDebounce = setTimeout(() => tail(), 300)
})

onMounted(() => loadSources())
onBeforeUnmount(() => {
  stopPolling()
  if (filterDebounce) clearTimeout(filterDebounce)
})
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 760px;
  max-width: 96vw;
  max-height: 90vh;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
  display: flex;
  flex-direction: column;
}
.log-dialog {
  height: 86vh;
}
.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
}
.dialog-title {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 15px;
  font-weight: 600;
}
.dialog-title svg {
  color: var(--color-primary);
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.toolbar {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  margin-bottom: 12px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.field-label {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.select,
.input {
  height: 32px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
  font-size: 13px;
}
.input {
  min-width: 160px;
}
.source-tabs {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}
.source-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
  border: 1px solid var(--color-border);
  background: var(--color-muted);
  color: var(--color-muted-foreground);
}
.source-tab.active {
  background: color-mix(in oklch, var(--color-primary) 16%, transparent);
  color: var(--color-primary);
  border-color: var(--color-primary);
}
.source-tab svg {
  width: 14px;
  height: 14px;
}
.lvl-badge {
  font-size: 9px;
  font-weight: 700;
  padding: 0 4px;
  border-radius: 3px;
  background: var(--color-info);
  color: var(--color-card);
}
.filters {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 8px 10px;
  background: var(--color-muted);
  border-radius: 6px;
  margin-bottom: 10px;
}
.chk {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--color-foreground);
  cursor: pointer;
}
.log-body {
  flex: 1;
  overflow-y: auto;
  background: color-mix(in oklch, var(--color-foreground) 4%, var(--color-card));
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 8px 10px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
  line-height: 1.5;
}
.log-placeholder,
.log-error,
.log-note {
  font-family: var(--font-sans, sans-serif);
  font-size: 12px;
  color: var(--color-muted-foreground);
  padding: 8px 2px;
}
.log-error {
  color: var(--color-destructive);
}
.log-note {
  color: var(--color-warning);
}
.history-btn {
  display: block;
  margin: 4px auto 10px;
  padding: 4px 12px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-primary);
  font-size: 12px;
  cursor: pointer;
}
.history-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.log-line {
  white-space: pre-wrap;
  word-break: break-all;
  padding: 0 2px;
}
.log-line.lvl-error {
  color: var(--color-destructive);
}
.log-line.lvl-warn {
  color: var(--color-warning);
}
.log-line.lvl-debug,
.log-line.lvl-trace {
  color: var(--color-muted-foreground);
}
.log-line.lvl-info {
  color: var(--color-info);
}
.log-line.lvl-notice {
  color: var(--color-success);
}
.dialog-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
}
.meta {
  font-size: 11px;
  color: var(--color-muted-foreground);
  font-variant-numeric: tabular-nums;
}
.meta.err {
  color: var(--color-destructive);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
}
.btn:hover {
  background: var(--color-muted);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.btn-sm {
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
}
</style>
