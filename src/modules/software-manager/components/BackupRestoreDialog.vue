<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog wide">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon icon="mdi:backup-restore" />
            {{ $t('backup') }}
            <span v-if="current" class="title-sub">{{ current.name }} {{ current.version }}</span>
          </div>
          <button class="dialog-close" @click="$emit('close')">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <!-- 实例切换 -->
        <div class="toolbar">
          <label class="field">
            <span class="field-label">{{ $t('selectInstance') }}</span>
            <select v-model="selectedId" class="select">
              <option v-for="i in instanceList" :key="i.id" :value="i.id">
                {{ i.name }} {{ i.version }}
              </option>
            </select>
          </label>
        </div>

        <!-- Tab 栏 -->
        <div class="tab-bar">
          <button class="tab" :class="{ active: tab === 'snapshots' }" @click="switchTab('snapshots')">
            {{ $t('snapshots') }}
          </button>
          <button class="tab" :class="{ active: tab === 'reset' }" @click="switchTab('reset')">
            {{ $t('resetInstance') }}
          </button>
        </div>

        <!-- Tab A：快照列表 -->
        <div v-if="tab === 'snapshots'" class="tab-pane">
          <!-- 创建快照 -->
          <div class="create-box">
            <input v-model="snapName" class="input" :placeholder="$t('snapshotNamePlaceholder')" />
            <input v-model="snapNote" class="input" :placeholder="$t('snapshotNotePlaceholder')" />
            <select v-model="snapMode" class="select">
              <option value="StopAndBackup">{{ $t('backupModeStop') }}</option>
              <option value="Hot">{{ $t('backupModeHot') }}</option>
            </select>
            <button class="btn primary" :disabled="creating" @click="onCreate">
              <Icon icon="mdi:camera-plus" /> {{ $t('createSnapshot') }}
            </button>
          </div>

          <!-- 定时自动备份（R1：按分钟间隔自动 Hot 快照，0 = 关闭） -->
          <div class="schedule-box">
            <label class="sched-toggle">
              <input type="checkbox" v-model="scheduleEnabled" @change="onScheduleChange" />
              {{ $t('scheduledBackup') }}
            </label>
            <template v-if="scheduleEnabled">
              <label class="sched-field">
                {{ $t('scheduleInterval') }}
                <select v-model.number="scheduleValue" class="select num" @change="onScheduleChange">
                  <option v-for="n in scheduleOptions" :key="n" :value="n">{{ n }}</option>
                </select>
                <select v-model="scheduleUnit" class="select" @change="onScheduleChange">
                  <option value="min">{{ $t('minuteUnit') }}</option>
                  <option value="hour">{{ $t('hourUnit') }}</option>
                  <option value="day">{{ $t('dayUnit') }}</option>
                </select>
              </label>
            </template>
          </div>

          <div v-if="loadingSnaps" class="text-muted py-4 text-center">{{ $t('loading') }}</div>
          <div v-else-if="snapshots.length === 0" class="text-muted py-4 text-center">{{ $t('noSnapshots') }}</div>
          <div v-else class="snap-list">
            <div v-for="s in snapshots" :key="s.id" class="snap-row">
              <div class="snap-info">
                <div class="snap-title">
                  <Icon icon="mdi:file-cabinet" />
                  <span class="snap-name">{{ s.name || s.id }}</span>
                  <span v-if="s.major_version != null" class="snap-mv">v{{ s.major_version }}.x</span>
                </div>
                <div class="snap-meta">
                  <span>{{ formatTime(s.created_at) }}</span>
                  <span>{{ $t('snapshotVersion') }}: {{ s.source_version }}</span>
                  <span>{{ $t('snapshotSize') }}: {{ formatBytes(s.size_bytes) }}</span>
                </div>
                <div v-if="s.note" class="snap-note">{{ s.note }}</div>
              </div>
              <div class="snap-actions">
                <button class="btn btn-sm" :disabled="restoring" @click="onRestore(s)">
                  <span v-if="restoring" class="spinner" />
                  {{ restoring ? $t('restoring') : $t('restore') }}
                </button>
                <button class="btn btn-sm danger" :disabled="deleting" @click="onDelete(s)">
                  {{ $t('deleteSnapshot') }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Tab B：一键重置 -->
        <div v-else class="tab-pane">
          <div class="reset-warn">
            <Icon icon="mdi:alert-outline" />
            <span>{{ $t('resetDanger') }}</span>
          </div>

          <label class="chk-block">
            <input type="checkbox" v-model="resetAck" />
            <span>{{ $t('resetConfirmCheckbox') }}</span>
          </label>

          <label class="field">
            <span class="field-label">{{ $t('resetTypeName', { name: current?.name ?? '' }) }}</span>
            <input v-model="resetInput" class="input" :placeholder="current?.name ?? ''" />
          </label>

          <div class="dialog-footer">
            <button class="btn danger" :disabled="!canReset || resetting" @click="onReset">
              <Icon v-if="!resetting" icon="mdi:delete-forever" />
              <span v-else class="spinner" />
              {{ resetting ? $t('resetting') : $t('resetInstance') }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useOpsStore } from '../stores/ops'
import { useLifecycleStore } from '../stores/lifecycle'
import { confirmAsync } from '@/composables/useConfirm'
import { toast } from '@/composables/useToast'
import { formatBytes } from '@/utils/format'
import { SoftwareStatus, type BackupMode, type InstalledSoftware, type SnapshotMeta } from '@/models/software'

const { t } = useI18n()
const ops = useOpsStore()
const lifecycleStore = useLifecycleStore()

const props = defineProps<{
  software: InstalledSoftware
  instances?: InstalledSoftware[]
  initialTab?: 'snapshots' | 'reset'
}>()
defineEmits<{ close: [] }>()

const tab = ref<'snapshots' | 'reset'>(props.initialTab ?? 'snapshots')
const selectedId = ref(props.software.id)

const instanceList = computed<InstalledSoftware[]>(() => {
  const list = props.instances ?? []
  if (list.length === 0) return [props.software]
  if (!list.some((i) => i.id === props.software.id)) return [props.software, ...list]
  return list
})

const current = computed(() => instanceList.value.find((i) => i.id === selectedId.value) ?? props.software)

// ---- Tab A 状态 ----
const snapshots = ref<SnapshotMeta[]>([])
const loadingSnaps = ref(false)
const creating = ref(false)
const restoring = ref(false)
const deleting = ref(false)
const snapName = ref('')
const snapNote = ref('')
const snapMode = ref<BackupMode>('StopAndBackup')

// ---- Tab B 状态 ----
const resetAck = ref(false)
const resetInput = ref('')
const resetting = ref(false)

const canReset = computed(
  () => resetAck.value && resetInput.value === (current.value?.name ?? '') && !resetting.value,
)

async function loadSnapshots() {
  loadingSnaps.value = true
  try {
    const list = await ops.loadSnapshots(selectedId.value)
    // 按创建时间降序：最新快照排在最上面（后端按 manifest 写入顺序返回，未排序）
    snapshots.value = list.slice().sort((a, b) => b.created_at.localeCompare(a.created_at))
  } catch (e) {
    console.error('load snapshots failed:', e)
    snapshots.value = []
  } finally {
    loadingSnaps.value = false
  }
}

// ---- 定时自动备份 (R1) ----
const scheduleEnabled = ref(false)
const scheduleValue = ref(1)
const scheduleUnit = ref<'min' | 'hour' | 'day'>('hour')
const UNIT_MINUTES = { min: 1, hour: 60, day: 1440 }
const schedulePresets: Record<'min' | 'hour' | 'day', number[]> = {
  min: [10, 15, 20, 30, 45, 60],
  hour: [1, 2, 3, 4, 6, 8, 12],
  day: [1, 2, 3, 4, 5, 7],
}
const scheduleOptions = computed(() => {
  const base = schedulePresets[scheduleUnit.value]
  // 反解出的值可能不在预设里，补进选项避免下拉为空
  return base.includes(scheduleValue.value) ? base : [scheduleValue.value, ...base]
})
function minutesToParts(m: number) {
  if (m % 1440 === 0) return { value: m / 1440, unit: 'day' as const }
  if (m % 60 === 0) return { value: m / 60, unit: 'hour' as const }
  return { value: m, unit: 'min' as const }
}
async function loadSchedule() {
  try {
    const m = await ops.getBackupSchedule(selectedId.value)
    scheduleEnabled.value = m > 0
    if (m > 0) {
      const parts = minutesToParts(m)
      scheduleValue.value = parts.value
      scheduleUnit.value = parts.unit
    } else {
      scheduleValue.value = 1
      scheduleUnit.value = 'hour'
    }
  } catch {
    scheduleEnabled.value = false
  }
}
async function onScheduleChange() {
  const minutes = scheduleEnabled.value ? scheduleValue.value * UNIT_MINUTES[scheduleUnit.value] : 0
  try {
    await ops.setBackupSchedule(selectedId.value, minutes)
  } catch (e) {
    console.error('set backup schedule failed:', e)
  }
}

function switchTab(next: 'snapshots' | 'reset') {
  if (next === tab.value) return
  tab.value = next
  if (next === 'snapshots') loadSnapshots()
}

async function onCreate() {
  creating.value = true
  try {
    await ops.createSnapshot(selectedId.value, snapMode.value, snapName.value, snapNote.value)
    snapName.value = ''
    snapNote.value = ''
    snapMode.value = 'StopAndBackup'
    await loadSnapshots()
  } catch (e) {
    console.error('create snapshot failed:', e)
  } finally {
    creating.value = false
  }
}

async function onRestore(s: SnapshotMeta) {
  const id = selectedId.value
  // 运行态必须先停服（决策 2）
  if (lifecycleStore.getStatus(id) === SoftwareStatus.Running) {
    toast(t('restoreNeedStop'), 'err')
    return
  }
  if (!(await confirmAsync(t('restoreConfirm')))) return
  restoring.value = true
  try {
    await ops.restoreSnapshot(id, s.id, false)
    toast(t('restoreSuccess'), 'ok')
  } catch (e: unknown) {
    const msg = String(e)
    // 跨大版本/跨来源不一致：提示可强制恢复（决策 8）
    if (msg.includes('强制恢复') || msg.includes('force')) {
      if (await confirmAsync(t('restoreForceConfirm'), { danger: true })) {
        try {
          await ops.restoreSnapshot(id, s.id, true)
          toast(t('restoreSuccess'), 'ok')
        } catch (e2: unknown) {
          toast(String(e2), 'err')
        }
      }
    } else {
      toast(msg, 'err')
    }
  } finally {
    restoring.value = false
  }
}

async function onDelete(s: SnapshotMeta) {
  if (!(await confirmAsync(t('deleteConfirm'), { danger: true }))) return
  deleting.value = true
  try {
    await ops.deleteSnapshot(selectedId.value, s.id)
    await loadSnapshots()
    toast(t('deleteSnapshotSuccess'), 'ok')
  } catch (e) {
    console.error('delete snapshot failed:', e)
    toast(t('deleteSnapshotFailed'), 'err')
  } finally {
    deleting.value = false
  }
}

async function onReset() {
  if (!canReset.value) return
  if (!(await confirmAsync(t('resetDanger'), { danger: true }))) return
  resetting.value = true
  try {
    await ops.resetInstance(selectedId.value)
    toast(t('resetSuccess'), 'ok')
    resetInput.value = ''
    resetAck.value = false
  } catch (e: unknown) {
    toast(String(e), 'err')
  } finally {
    resetting.value = false
  }
}

function formatTime(rfc: string): string {
  const d = new Date(rfc)
  if (isNaN(d.getTime())) return rfc
  return d.toLocaleString()
}

// 切换实例 → 刷新快照
watch(selectedId, () => {
  loadSchedule()
  if (tab.value === 'snapshots') loadSnapshots()
})

onMounted(() => {
  loadSchedule()
  if (tab.value === 'snapshots') loadSnapshots()
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
  width: 640px;
  max-width: 96vw;
  max-height: 90vh;
  overflow: hidden;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
  display: flex;
  flex-direction: column;
}
.dialog.wide {
  width: 720px;
}
.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
  flex: none;
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
.title-sub {
  font-size: 12px;
  font-weight: 400;
  color: var(--color-muted-foreground);
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
  background: var(--color-destructive);
  color: var(--color-destructive-foreground);
}
.toolbar {
  display: flex;
  align-items: flex-end;
  gap: 12px;
  margin-bottom: 12px;
  flex: none;
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
.tab-bar {
  display: flex;
  gap: 4px;
  padding: 4px;
  background: var(--color-muted);
  border-radius: 6px;
  margin-bottom: 16px;
  flex: none;
}
.tab {
  flex: 1;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  text-align: center;
  color: var(--color-muted-foreground);
  border: none;
  background: transparent;
}
.tab.active {
  background: var(--color-card);
  color: var(--color-primary);
  font-weight: 500;
}
.tab-pane {
  flex: 1 1 auto;
  min-height: 180px;
  overflow-y: auto;
}
.create-box {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 14px;
}
.create-box .input {
  flex: 1;
  min-width: 120px;
}
.schedule-box {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  margin-bottom: 14px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-muted);
}
.sched-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--color-foreground);
  cursor: pointer;
}
.sched-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.sched-field .select {
  height: 28px;
  padding: 0 6px;
}
.sched-field .num {
  width: 64px;
}
.text-muted {
  color: var(--color-muted-foreground);
  font-size: 13px;
}
.py-4 {
  padding-top: 16px;
  padding-bottom: 16px;
}
.text-center {
  text-align: center;
}
.snap-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.snap-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-muted);
}
.snap-info {
  min-width: 0;
  flex: 1;
}
.snap-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}
.snap-title svg {
  width: 16px;
  height: 16px;
  color: var(--color-primary);
}
.snap-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.snap-mv {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--color-info);
  color: var(--color-card);
}
.snap-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 4px;
}
.snap-note {
  font-size: 11px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  font-style: italic;
}
.snap-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.reset-warn {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 6px;
  background: color-mix(in oklch, var(--color-destructive) 10%, transparent);
  color: var(--color-destructive);
  font-size: 12px;
  margin-bottom: 14px;
}
.reset-warn svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
.chk-block {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  margin-bottom: 12px;
  cursor: pointer;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 10px;
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid var(--color-border);
}
.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid currentColor;
  border-right-color: transparent;
  border-radius: 50%;
  display: inline-block;
  animation: spin 0.7s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
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
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.danger {
  color: var(--color-destructive);
  border-color: color-mix(in oklch, var(--color-destructive) 40%, var(--color-border));
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
