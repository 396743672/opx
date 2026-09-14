<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:history" :title="$t('auditLog')">
      <template #actions>
        <button class="btn" @click="loadAll" :disabled="loading">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
        <button class="btn" @click="onExport" :disabled="exporting || entries.length === 0">
          <Icon icon="mdi:download" /> {{ $t('auditExport') }}
        </button>
      </template>
    </PageHeader>

    <!-- 概览 -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-4">
      <StatCard :label="$t('auditToday')" :value="String(stats?.today ?? 0)" icon="mdi:calendar-today" accent="primary" />
      <StatCard
        :label="$t('auditTotal')"
        :value="String(stats?.total ?? 0)"
        :sub="stats ? $t('auditFailed', { n: stats.failed }) : undefined"
        icon="mdi:counter"
        accent="chart-2"
      />
      <StatCard :label="$t('auditLast')" :value="lastTs" icon="mdi:clock-outline" accent="chart-3" />
      <div class="rounded-lg border border-border bg-card p-4 shadow-card">
        <div class="text-xs text-muted-foreground mb-2">{{ $t('auditTopActions') }}</div>
        <div v-if="!stats?.by_action.length" class="text-sm text-muted-foreground">—</div>
        <div v-else class="flex flex-col gap-1">
          <div v-for="a in stats.by_action.slice(0, 3)" :key="a.action" class="flex items-center justify-between text-sm">
            <span class="truncate">{{ a.action }}</span>
            <span class="tnum text-muted-foreground">{{ a.count }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- 过滤栏 -->
    <div class="flex flex-wrap items-center gap-2 mb-3">
      <select v-model.number="days" class="input" @change="reload">
        <option :value="1">{{ $t('auditRangeToday') }}</option>
        <option :value="3">{{ $t('auditRange3d') }}</option>
        <option :value="7">{{ $t('auditRange7d') }}</option>
      </select>
      <select v-model="action" class="input" @change="reload">
        <option value="">{{ $t('auditAllActions') }}</option>
        <option v-for="a in stats?.by_action ?? []" :key="a.action" :value="a.action">{{ a.action }}</option>
      </select>
      <select v-model="resultFilter" class="input" @change="reload">
        <option value="">{{ $t('auditAllResults') }}</option>
        <option value="ok">{{ $t('auditResultOk') }}</option>
        <option value="fail">{{ $t('auditResultFail') }}</option>
        <option value="running">{{ $t('auditResultRunning') }}</option>
        <option value="__none">{{ $t('auditResultUnset') }}</option>
      </select>
      <input v-model="keyword" class="input grow" :placeholder="$t('auditKeywordPlaceholder')" @keyup.enter="reload" />
      <button class="btn" @click="reload"><Icon icon="mdi:magnify" /> {{ $t('auditKeyword') }}</button>
    </div>

    <!-- 表格 -->
    <div v-if="loading" class="py-8 text-center text-sm text-muted-foreground">{{ $t('loading') }}</div>
    <div v-else-if="entries.length === 0" class="py-8 text-center text-sm text-muted-foreground">{{ $t('auditEmpty') }}</div>
    <table v-else class="audit-table">
      <thead>
        <tr>
          <th>{{ $t('auditTimeCol') }}</th>
          <th>{{ $t('auditActionCol') }}</th>
          <th>{{ $t('auditTargetCol') }}</th>
          <th>{{ $t('auditDetailCol') }}</th>
          <th>{{ $t('auditResultCol') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(e, i) in entries" :key="e.ts + i" :class="{ 'row-fail': resultKind(e) === 'fail' }">
          <td class="tnum">{{ formatTs(e.ts) }}</td>
          <td><span class="action-chip">{{ e.action }}</span></td>
          <td class="truncate-cell">{{ e.target }}</td>
          <td class="truncate-cell">{{ e.detail || '—' }}</td>
          <td>
            <span class="result-chip" :class="`result-${resultKind(e)}`">{{ resultLabel(e) }}</span>
            <span v-if="e.error" class="result-error" :title="e.error">{{ e.error }}</span>
          </td>
        </tr>
      </tbody>
    </table>

    <!-- 分页 -->
    <div v-if="total > 0" class="flex items-center justify-end gap-2 mt-3">
      <span class="text-xs text-muted-foreground">{{ $t('perPage') }}</span>
      <select v-model.number="pageSize" class="input w-20" @change="onPageSizeChange">
        <option v-for="n in PAGE_SIZE_OPTIONS" :key="n" :value="n">{{ n }}</option>
      </select>
      <span class="text-xs text-muted-foreground tnum">
        {{ $t('auditPageInfo', { from: pageFrom, to: pageTo, total }) }}
      </span>
      <button class="btn" :disabled="page === 0 || loading" @click="goPage(page - 1)">
        {{ $t('prevPage') }}
      </button>
      <button class="btn" :disabled="!truncated || loading" @click="goPage(page + 1)">
        {{ $t('nextPage') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import { toast } from '@/composables/useToast'
import type { AuditEntry, AuditStats } from '@/models/audit'

const { t } = useI18n()

const days = ref(7)
const action = ref('')
const keyword = ref('')
const resultFilter = ref('')
const entries = ref<AuditEntry[]>([])
const stats = ref<AuditStats | null>(null)
const truncated = ref(false)
const page = ref(0)
const total = ref(0)
const pageSize = ref(50)
const PAGE_SIZE_OPTIONS = [20, 50, 100, 200]
const loading = ref(false)
const exporting = ref(false)

const lastTs = computed(() => (stats.value?.last_ts ? formatTs(stats.value.last_ts) : t('auditNone')))
// 分页展示区间（1 基）
const pageFrom = computed(() => (total.value === 0 ? 0 : page.value * pageSize.value + 1))
const pageTo = computed(() => page.value * pageSize.value + entries.value.length)

function formatTs(iso: string): string {
  const d = new Date(iso)
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString()
}

type ResultKind = 'ok' | 'fail' | 'running' | 'none'

function resultKind(e: AuditEntry): ResultKind {
  if (e.result === 'ok' || e.result === 'fail' || e.result === 'running') return e.result
  return 'none'
}

function resultLabel(e: AuditEntry): string {
  switch (resultKind(e)) {
    case 'ok':
      return t('auditResultOk')
    case 'fail':
      return t('auditResultFail')
    case 'running':
      return t('auditResultRunning')
    default:
      return t('auditResultNone')
  }
}

async function loadEntries() {
  loading.value = true
  try {
    const q = await invoke<{ entries: AuditEntry[]; total: number; truncated: boolean }>('list_audit_entries', {
      days: days.value,
      action: action.value || null,
      keyword: keyword.value.trim() || null,
      // '' = 全部（传 null）；'__none' = 未采集（传空串）
      result: resultFilter.value === '' ? null : resultFilter.value === '__none' ? '' : resultFilter.value,
      limit: pageSize.value,
      offset: page.value * pageSize.value,
    })
    entries.value = q.entries
    total.value = q.total
    truncated.value = q.truncated
  } catch (e) {
    toast(String(e), 'err')
  } finally {
    loading.value = false
  }
}

async function loadStats() {
  try {
    stats.value = await invoke<AuditStats>('audit_stats', { days: 7 })
  } catch {
    stats.value = null
  }
}

// 过滤条件变化 → 回到第一页
function reload() {
  page.value = 0
  loadEntries()
}

function goPage(n: number) {
  if (n < 0 || n === page.value) return
  // 只有「向后翻」才需要检查是否还有下一页（此前误拦了上一页）
  if (n > page.value && !truncated.value) return
  page.value = n
  loadEntries()
}

function onPageSizeChange() {
  page.value = 0
  loadEntries()
}

async function loadAll() {
  await Promise.all([loadEntries(), loadStats()])
}

async function onExport() {
  exporting.value = true
  try {
    const dest = await save({ defaultPath: 'audit-log.csv', title: t('auditExport') })
    if (!dest) return
    await invoke('export_audit_entries', {
      days: days.value,
      action: action.value || null,
      keyword: keyword.value.trim() || null,
      // '' = 全部（传 null）；'__none' = 未采集（传空串）
      result: resultFilter.value === '' ? null : resultFilter.value === '__none' ? '' : resultFilter.value,
      destPath: dest,
    })
    toast(t('auditExportDone'), 'ok')
  } catch (e) {
    toast(String(e), 'err')
  } finally {
    exporting.value = false
  }
}

onMounted(loadAll)
</script>

<style scoped>
.input {
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
.grow {
  flex: 1;
  min-width: 160px;
}
.audit-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.audit-table th,
.audit-table td {
  text-align: left;
  padding: 8px 10px;
  border-bottom: 1px solid var(--color-border);
}
.audit-table th {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-muted-foreground);
}
.action-chip {
  display: inline-block;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
  font-weight: 600;
}
.truncate-cell {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-fail {
  background: color-mix(in oklch, var(--color-destructive, #ef4444) 6%, transparent);
}
.result-chip {
  display: inline-block;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
}
.result-ok {
  background: color-mix(in oklch, var(--color-chart-2, #22c55e) 16%, transparent);
  color: var(--color-chart-2, #22c55e);
}
.result-fail {
  background: color-mix(in oklch, var(--color-destructive, #ef4444) 16%, transparent);
  color: var(--color-destructive, #ef4444);
}
.result-running {
  background: color-mix(in oklch, var(--color-chart-3, #f59e0b) 16%, transparent);
  color: var(--color-chart-3, #f59e0b);
}
.result-none {
  color: var(--color-muted-foreground);
}
.result-error {
  display: inline-block;
  max-width: 260px;
  margin-left: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
  font-size: 11px;
  color: var(--color-destructive, #ef4444);
}
.tnum {
  font-variant-numeric: tabular-nums;
}
</style>
