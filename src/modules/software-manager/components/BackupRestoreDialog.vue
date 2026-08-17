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

          <div v-if="loadingSnaps" class="text-muted py-4 text-center">{{ $t('loading') }}</div>
          <div v-else-if="snapshots.length === 0" class="text-muted py-4 text-center">{{ $t('noSnapshots') }}</div>
          <div v-else class="snap-list">
            <div v-for="s in snapshots" :key="s.id" class="snap-row">
              <div class="snap-info">
                <div class="snap-title">
                  <Icon icon="mdi:file-cabinet-outline" />
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
                  {{ $t('restore') }}
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
            <span v-if="resetMsg" class="meta" :class="resetMsgKind">{{ resetMsg }}</span>
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
const resetMsg = ref<string | null>(null)
const resetMsgKind = ref<'err' | 'ok'>('ok')

const canReset = computed(
  () => resetAck.value && resetInput.value === (current.value?.name ?? '') && !resetting.value,
)

async function loadSnapshots() {
  loadingSnaps.value = true
  try {
    snapshots.value = await ops.loadSnapshots(selectedId.value)
  } catch (e) {
    console.error('load snapshots failed:', e)
    snapshots.value = []
  } finally {
    loadingSnaps.value = false
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
    resetMsg.value = t('restoreNeedStop')
    resetMsgKind.value = 'err'
    return
  }
  if (!confirm(t('restoreConfirm'))) return
  restoring.value = true
  try {
    await ops.restoreSnapshot(id, s.id, false)
    resetMsg.value = t('restoreSuccess')
    resetMsgKind.value = 'ok'
  } catch (e: unknown) {
    const msg = String(e)
    // 跨大版本/跨来源不一致：提示可强制恢复（决策 8）
    if (msg.includes('强制恢复') || msg.includes('force')) {
      if (confirm(t('restoreForceConfirm'))) {
        try {
          await ops.restoreSnapshot(id, s.id, true)
          resetMsg.value = t('restoreSuccess')
          resetMsgKind.value = 'ok'
        } catch (e2: unknown) {
          resetMsg.value = String(e2)
          resetMsgKind.value = 'err'
        }
      }
    } else {
      resetMsg.value = msg
      resetMsgKind.value = 'err'
    }
  } finally {
    restoring.value = false
  }
}

async function onDelete(s: SnapshotMeta) {
  if (!confirm(t('deleteConfirm'))) return
  deleting.value = true
  try {
    await ops.deleteSnapshot(selectedId.value, s.id)
  } catch (e) {
    console.error('delete snapshot failed:', e)
  } finally {
    deleting.value = false
  }
}

async function onReset() {
  if (!canReset.value) return
  if (!confirm(t('resetDanger'))) return
  resetting.value = true
  resetMsg.value = null
  try {
    await ops.resetInstance(selectedId.value)
    resetMsg.value = t('resetSuccess')
    resetMsgKind.value = 'ok'
    resetInput.value = ''
    resetAck.value = false
  } catch (e: unknown) {
    resetMsg.value = String(e)
    resetMsgKind.value = 'err'
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
  resetMsg.value = null
  if (tab.value === 'snapshots') loadSnapshots()
})

onMounted(() => {
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
  overflow-y: auto;
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
.tab-bar {
  display: flex;
  gap: 4px;
  padding: 4px;
  background: var(--color-muted);
  border-radius: 6px;
  margin-bottom: 16px;
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
  min-height: 180px;
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
.meta {
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  margin-right: auto;
}
.meta.err {
  color: var(--color-destructive);
}
.meta.ok {
  color: var(--color-success);
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
