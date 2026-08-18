<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t('stacks') }}</h1>
        <p class="page-desc">{{ t('stacksDesc') }}</p>
      </div>
      <div class="head-actions">
        <button class="btn" @click="onImport">
          <Icon icon="mdi:file-import" /> {{ t('importStack') }}
        </button>
        <button class="btn primary" @click="openCreate">
          <Icon icon="mdi:plus" /> {{ t('createStack') }}
        </button>
      </div>
    </div>

    <div v-if="store.error" class="global-error">{{ store.error }}</div>

    <div v-if="store.loading" class="loading">{{ t('loading') }}</div>

    <div v-else-if="store.stacks.length === 0" class="empty-state">
      <Icon icon="mdi:layers-outline" class="empty-icon" />
      <div>{{ t('noStacks') }}</div>
      <button class="btn primary" @click="openCreate">
        <Icon icon="mdi:plus" /> {{ t('createStack') }}
      </button>
    </div>

    <div v-else class="stack-grid">
      <div
        v-for="stack in store.stacks"
        :key="stack.id"
        class="stack-card"
        :class="{ active: selectedId === stack.id }"
        @click="select(stack)"
      >
        <div class="sc-head">
          <div>
            <div class="sc-name">{{ stack.name }}</div>
            <div v-if="stack.description" class="sc-desc">{{ stack.description }}</div>
          </div>
          <span class="sc-status" :class="cardStatusClass(stack)">
            {{ t(cardStatusLabel(stack)) }}
          </span>
        </div>

        <div class="sc-meta">
          <Icon icon="mdi:account-group" />
          {{ stack.items.length }} {{ t('stackMembers') }}
        </div>

        <div class="sc-actions" @click.stop>
          <button
            class="btn"
            :class="{ primary: canStart(stack) }"
            :disabled="!canStart(stack) || busyId === stack.id"
            @click="onStart(stack)"
          >
            <Icon v-if="busyId === stack.id && busyAction === 'start'" icon="mdi:loading" class="spinning" />
            <Icon v-else icon="mdi:play" />
            {{ t('startStack') }}
          </button>
          <button
            class="btn"
            :class="{ primary: canStop(stack) }"
            :disabled="!canStop(stack) || busyId === stack.id"
            @click="onStop(stack)"
          >
            <Icon v-if="busyId === stack.id && busyAction === 'stop'" icon="mdi:loading" class="spinning" />
            <Icon v-else icon="mdi:stop" />
            {{ t('stopStack') }}
          </button>
          <button
            class="btn"
            :disabled="!canRestart(stack) || busyId === stack.id"
            @click="onRestart(stack)"
          >
            <Icon v-if="busyId === stack.id && busyAction === 'restart'" icon="mdi:loading" class="spinning" />
            <Icon v-else icon="mdi:restart" />
            {{ t('restartStack') }}
          </button>
          <button class="btn ghost" :disabled="busyId === stack.id || !canStop(stack)" :title="t('exportStack')" @click="onExport(stack)">
            <Icon icon="mdi:export" />
          </button>
          <button class="btn ghost" :disabled="!canEdit(stack) || busyId === stack.id" @click="onEdit(stack)">
            <Icon icon="mdi:pencil" /> {{ t('editStack') }}
          </button>
          <button class="btn danger ghost" :disabled="!canEdit(stack) || busyId === stack.id" @click="onDelete(stack)">
            <Icon icon="mdi:delete" />
          </button>
        </div>
      </div>
    </div>

    <!-- 选中栈的运行面板 -->
    <div v-if="selectedStack" class="run-area">
      <StackRunPanel :stack="selectedStack" />
    </div>

    <!-- 编辑 / 新建对话框 -->
    <StackEditDialog
      v-if="showDialog"
      :stack="editingStack"
      @close="showDialog = false"
      @saved="onSaved"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useStackStore } from '@/stores/stack'
import type { Stack, StackMemberStatus } from '@/models/stack'
import StackEditDialog from './StackEditDialog.vue'
import StackRunPanel from './StackRunPanel.vue'

const { t } = useI18n()
const store = useStackStore()
const showDialog = ref(false)
const editingStack = ref<Stack | null>(null)
const selectedId = ref<string | null>(null)
// 正在编排的栈 id（启动/停止/重启期间阻挡重复点击并显示遮罩）
const busyId = ref<string | null>(null)

const selectedStack = computed<Stack | null>(
  () => store.stacks.find((s) => s.id === selectedId.value) ?? null
)

function select(stack: Stack) {
  selectedId.value = stack.id
}

function overallStatusOf(stack: Stack): StackMemberStatus {
  const rt = store.getRuntime(stack.id)
  if (rt.length === 0) return 'pending'
  if (rt.some((m) => m.status === 'failed')) return 'failed'
  if (rt.every((m) => m.status === 'running')) return 'running'
  if (rt.some((m) => m.status === 'starting')) return 'starting'
  if (rt.some((m) => m.status === 'stopping')) return 'stopping'
  if (rt.every((m) => m.status === 'stopped')) return 'stopped'
  return 'pending'
}

function cardStatusLabel(stack: Stack): string {
  switch (overallStatusOf(stack)) {
    case 'running':
      return 'stackRunning'
    case 'failed':
      return 'stackFailed'
    case 'starting':
      return 'stackStarting'
    case 'stopping':
      return 'stackStopping'
    case 'stopped':
      return 'stackStopped'
    default:
      return 'stackPending'
  }
}
function cardStatusClass(stack: Stack): string {
  return `st-${overallStatusOf(stack)}`
}

// 状态驱动的按钮门控（与软件管理一致）：未运行不能停，已运行不能启
function isRunning(stack: Stack): boolean {
  return ['running', 'starting', 'stopping'].includes(overallStatusOf(stack))
}
function canStart(stack: Stack): boolean {
  return !isRunning(stack)
}
function canStop(stack: Stack): boolean {
  return isRunning(stack)
}
function canRestart(stack: Stack): boolean {
  return overallStatusOf(stack) === 'running'
}
function canEdit(stack: Stack): boolean {
  return !isRunning(stack)
}

function openCreate() {
  editingStack.value = null
  showDialog.value = true
}
function onEdit(stack: Stack) {
  editingStack.value = stack
  showDialog.value = true
}
function onSaved() {
  showDialog.value = false
}

const busyAction = ref<'start' | 'stop' | 'restart'>('start')

async function runBusy(id: string, action: 'start' | 'stop' | 'restart', fn: () => Promise<void>) {
  if (busyId.value) return
  busyId.value = id
  busyAction.value = action
  try {
    await fn()
  } catch (e) {
    store.error = String(e)
  } finally {
    busyId.value = null
  }
}

async function onStart(stack: Stack) {
  await runBusy(stack.id, 'start', async () => {
    await store.startStack(stack.id)
  })
}
async function onStop(stack: Stack) {
  await runBusy(stack.id, 'stop', async () => {
    await store.stopStack(stack.id)
  })
}
async function onRestart(stack: Stack) {
  await runBusy(stack.id, 'restart', async () => {
    await store.restartStack(stack.id)
  })
}

async function onExport(stack: Stack) {
  const path = await save({
    defaultPath: `${stack.name}.json`,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof path === 'string' && path) {
    try {
      await store.exportStack(stack.id, path)
    } catch (e) {
      store.error = String(e)
    }
  }
}

async function onImport() {
  const picked = await open({
    multiple: false,
    filters: [{ name: 'JSON', extensions: ['json'] }],
  })
  if (typeof picked === 'string' && picked) {
    const stack = await store.importStack(picked)
    if (stack) selectedId.value = stack.id
  }
}

async function onDelete(stack: Stack) {
  if (!confirm(t('confirmDeleteStack'))) return
  await store.deleteStack(stack.id)
  if (selectedId.value === stack.id) selectedId.value = null
}

onMounted(async () => {
  await store.loadStacks()
  await store.loadCandidates()
  await store.subscribe()
})
onUnmounted(() => {
  store.unsubscribe()
})
</script>

<style scoped>
.page {
  padding: 20px;
}
.page-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  margin-bottom: 16px;
}
.page-title {
  font-size: 20px;
  font-weight: 700;
}
.page-desc {
  font-size: 13px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
}
.head-actions {
  display: flex;
  gap: 8px;
}
.global-error {
  background: color-mix(in oklch, var(--color-danger, red) 15%, transparent);
  border: 1px solid var(--color-danger, red);
  color: var(--color-danger, red);
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 12px;
  margin-bottom: 12px;
}
.loading,
.empty-state {
  padding: 40px;
  text-align: center;
  color: var(--color-muted-foreground);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}
.empty-icon {
  font-size: 40px;
  opacity: 0.5;
}
.stack-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 14px;
}
.stack-card {
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-card);
  padding: 14px;
  cursor: pointer;
  transition: border-color 0.15s, box-shadow 0.15s;
}
.stack-card:hover {
  border-color: var(--color-primary);
}
.stack-card.active {
  border-color: var(--color-primary);
  box-shadow: 0 0 0 1px var(--color-primary);
}
.sc-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 10px;
}
.sc-name {
  font-size: 15px;
  font-weight: 600;
}
.sc-desc {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  word-break: break-all;
}
.sc-status {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  white-space: nowrap;
  background: var(--color-muted);
  color: var(--color-muted-foreground);
}
.sc-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin: 10px 0;
}
.sc-actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  position: relative;
}
.spinning {
  animation: opx-spin 1s linear infinite;
}
.run-area {
  margin-top: 18px;
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
  transition: background 0.15s, color 0.15s, border-color 0.15s, opacity 0.15s;
}
.btn:hover:not(:disabled) {
  background: var(--color-muted);
}
.btn.primary {
  background: var(--color-primary);
  color: var(--color-primary-foreground);
  border-color: var(--color-primary);
}
.btn.primary:hover:not(:disabled) {
  background: color-mix(in oklch, var(--color-primary) 88%, black);
}
.btn.danger {
  background: color-mix(in oklch, var(--color-danger, red) 12%, transparent);
  color: var(--color-danger, red);
  border-color: color-mix(in oklch, var(--color-danger, red) 30%, transparent);
}
.btn.danger:hover:not(:disabled) {
  background: color-mix(in oklch, var(--color-danger, red) 20%, transparent);
}
.btn.ghost {
  background: transparent;
  border-color: transparent;
}
.btn.ghost:hover:not(:disabled) {
  background: var(--color-muted);
}
.btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* 状态着色（与运行面板一致） */
.st-running {
  background: color-mix(in oklch, green 15%, transparent);
  color: green;
}
.st-failed {
  background: color-mix(in oklch, var(--color-danger, red) 15%, transparent);
  color: var(--color-danger, red);
}
.st-starting {
  background: color-mix(in oklch, var(--color-primary) 15%, transparent);
  color: var(--color-primary);
}
.st-stopping {
  background: color-mix(in oklch, orange 15%, transparent);
  color: orange;
}
.st-stopped,
.st-pending {
  background: var(--color-muted);
  color: var(--color-muted-foreground);
}
</style>
