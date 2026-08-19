<template>
  <div class="page">
    <div class="page-head">
      <div>
        <h1 class="page-title">{{ t('stacks') }}</h1>
        <p class="page-desc">{{ t('stacksDesc') }}</p>
      </div>
      <div class="head-actions">
        <div class="tmpl-wrap">
          <button class="btn" @click="showTemplates = !showTemplates">
            <Icon icon="mdi:apps" /> {{ t('stackTemplates') }}
          </button>
          <div v-if="showTemplates" class="tmpl-menu">
            <button
              v-for="tmpl in templateItems"
              :key="tmpl.key"
              class="tmpl-item"
              @click="openWithTemplate(tmpl); showTemplates = false"
            >
              <Icon icon="mdi:view-grid-plus" /> {{ t(tmpl.labelKey) }}
            </button>
          </div>
        </div>
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
          <button class="btn" :disabled="busyId === stack.id || !canStop(stack)" @click="onExport(stack)">
            <Icon icon="mdi:export" /> {{ t('exportStack') }}
          </button>
          <button class="btn" :disabled="!canEdit(stack) || busyId === stack.id" @click="onEdit(stack)">
            <Icon icon="mdi:pencil" /> {{ t('editStack') }}
          </button>
          <button class="btn danger" :disabled="!canEdit(stack) || busyId === stack.id" @click="onDelete(stack)">
            <Icon icon="mdi:delete" /> {{ t('deleteStack') }}
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
      :initial-items="dialogInitialItems"
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
import type { Stack, StackMemberStatus, StackItem } from '@/models/stack'
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
  dialogInitialItems.value = undefined
  showDialog.value = true
}
function onEdit(stack: Stack) {
  editingStack.value = stack
  dialogInitialItems.value = undefined
  showDialog.value = true
}
function onSaved() {
  showDialog.value = false
}

// ---- 模板 ----
// 内置预设模板：软件 key → 已装 id，预填新建对话框。软件未装则跳过该项（模板仍可用）。
const STACK_TEMPLATES: { key: string; labelKey: string; requires: { key: string; name: string }[] }[] = [
  {
    key: 'dev-env',
    labelKey: 'templateDevEnv',
    requires: [
      { key: 'mysql', name: 'MySQL' },
      { key: 'redis', name: 'Redis' },
      { key: 'nginx', name: 'Nginx' },
    ],
  },
  {
    key: 'cache-web',
    labelKey: 'templateCacheWeb',
    requires: [
      { key: 'redis', name: 'Redis' },
      { key: 'nginx', name: 'Nginx' },
    ],
  },
  {
    key: 'object-storage',
    labelKey: 'templateObjectStorage',
    requires: [
      { key: 'minio', name: 'MinIO' },
      { key: 'rustfs', name: 'RustFS' },
    ],
  },
]

const dialogInitialItems = ref<StackItem[] | undefined>(undefined)
const showTemplates = ref(false)
const templateItems = computed(() =>
  STACK_TEMPLATES.map((tmpl) => ({
    ...tmpl,
    // 解析为已装软件成员（按 key 匹配已装软件 id；未装则留空项）
    items: tmpl.requires
      .map((r) => {
        const sw = store.installedSoftware.find((s) => s.key === r.key)
        return sw
          ? {
              ref_type: 'software' as const,
              ref_id: sw.id,
              order: 0,
              depends_on: [] as string[],
              enabled: true,
              retry: 0,
            }
          : null
      })
      .filter(Boolean) as StackItem[],
  }))
)

function openWithTemplate(tmpl: (typeof templateItems.value)[number]) {
  editingStack.value = null
  // 模板成员若全部未装则 items 空，回退为空新建
  dialogInitialItems.value = tmpl.items
  showDialog.value = true
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
.tmpl-wrap {
  position: relative;
}
.tmpl-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 30;
  min-width: 180px;
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  box-shadow: 0 6px 18px oklch(0 0 0 / 0.18);
  padding: 4px;
  display: flex;
  flex-direction: column;
}
.tmpl-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  cursor: pointer;
  text-align: left;
}
.tmpl-item:hover {
  background: var(--color-muted);
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
  grid-template-columns: 1fr;
  gap: 12px;
}
@media (min-width: 768px) {
  .stack-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
.stack-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid var(--color-border);
  border-radius: 10px;
  background: var(--color-card);
  padding: 16px;
  box-shadow: var(--shadow-card);
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
  opacity: 0.5;
  cursor: not-allowed;
  background: var(--color-muted);
  border-color: var(--color-border);
  color: var(--color-muted-foreground);
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
