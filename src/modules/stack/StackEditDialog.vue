<template>
  <Teleport to="body">
    <div class="overlay" @click.self="onClose">
      <div class="dialog">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon :icon="isEdit ? 'mdi:pencil' : 'mdi:plus-box'" />
            {{ isEdit ? $t('editStack') : $t('createStack') }}
          </div>
          <button class="dialog-close" @click="onClose">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <div v-if="error" class="error-banner">{{ error }}</div>

        <div class="field">
          <label class="form-field-label">{{ $t('stackName') }}</label>
          <input
            v-model="name"
            class="text-input"
            :placeholder="$t('stackNamePlaceholder')"
          />
        </div>

        <div class="field">
          <label class="form-field-label">{{ $t('description') }}</label>
          <input v-model="description" class="text-input" />
        </div>

        <label class="auto-start-row">
          <input type="checkbox" v-model="autoStart" />
          {{ $t('stackAutoStart') }}
        </label>

        <div class="columns">
          <!-- 左：候选成员 -->
          <div class="col">
            <div class="col-title">
              <Icon icon="mdi:package-variant-closed" /> {{ $t('softwareType') }}
            </div>
            <div class="candidate-list">
              <label
                v-for="sw in candidateSoftware"
                :key="sw.id"
                class="candidate"
                :class="{ selected: isSelected('software', sw.id) }"
              >
                <input
                  type="checkbox"
                  :checked="isSelected('software', sw.id)"
                  @change="toggle('software', sw.id, sw.name)"
                />
                <span class="candidate-name">{{ sw.name }}</span>
                <span class="candidate-meta">{{ sw.version }}</span>
              </label>
              <div v-if="store.installedSoftware.length === 0" class="empty-hint">
                —
              </div>
            </div>

            <div class="col-title" style="margin-top: 10px">
              <Icon icon="mdi:leaf" /> {{ $t('springbootType') }}
            </div>
            <div class="candidate-list">
              <label
                v-for="app in store.springbootApps"
                :key="app.id"
                class="candidate"
                :class="{ selected: isSelected('springboot', app.id) }"
              >
                <input
                  type="checkbox"
                  :checked="isSelected('springboot', app.id)"
                  @change="toggle('springboot', app.id, app.name)"
                />
                <span class="candidate-name">{{ app.name }}</span>
                <span class="candidate-meta">{{ app.version ?? '' }}</span>
              </label>
              <div v-if="store.springbootApps.length === 0" class="empty-hint">—</div>
            </div>
          </div>

          <!-- 右：已选成员（可拖拽排序） -->
          <div class="col">
            <div class="col-title">
              <Icon icon="mdi:format-list-numbered" /> {{ $t('stackMembers') }}
              <span class="hint">{{ $t('dragToSort') }}</span>
            </div>
            <div class="member-list">
              <div
                v-for="(item, idx) in items"
                :key="item.ref_id"
                class="member"
              >
                <div class="member-head">
                  <span
                    class="type-badge"
                    :class="item.ref_type === 'software' ? 't-sw' : 't-sb'"
                  >
                    {{ item.ref_type === 'software' ? $t('softwareType') : $t('springbootType') }}
                  </span>
                  <span class="member-name">{{ resolveName(item) }}</span>
                  <div class="move-btns">
                    <button
                      class="mini-btn"
                      type="button"
                      :disabled="idx === 0"
                      :title="$t('moveUp')"
                      @click="moveUp(idx)"
                    >&#8593;</button>
                    <button
                      class="mini-btn"
                      type="button"
                      :disabled="idx === items.length - 1"
                      :title="$t('moveDown')"
                      @click="moveDown(idx)"
                    >&#8595;</button>
                    <button
                      class="mini-btn remove"
                      type="button"
                      :title="$t('removeMember')"
                      @click="removeMember(item.ref_id)"
                    >
                      <Icon icon="mdi:close" />
                    </button>
                  </div>
                </div>
                <label class="mini-toggle">
                  <input
                    type="checkbox"
                    v-model="item.enabled"
                  />
                  {{ $t('enabled') }}
                </label>
                <div class="member-fields">
                  <label class="mini-field">
                    {{ $t('memberOrder') }}
                    <input
                      type="number"
                      min="0"
                      v-model.number="item.order"
                      class="num"
                    />
                  </label>
                  <label class="mini-field">
                    {{ $t('stackRetry') }}
                    <input
                      type="number"
                      min="0"
                      v-model.number="item.retry"
                      class="num"
                    />
                  </label>
                  <label class="mini-field grow">
                    {{ $t('dependsOn') }}
                    <div class="dep-panel" :title="$t('dependsOnHint')">
                      <label
                        v-for="c in dependencyPool.filter((c) => c.id !== item.ref_id)"
                        :key="c.id"
                        class="dep-option"
                      >
                        <input
                          type="checkbox"
                          :checked="item.depends_on.includes(c.id)"
                          @change="toggleDep(item, c.id)"
                        />
                        <span :class="{ self: c.id === item.ref_id }">{{ c.name }}</span>
                      </label>
                      <div v-if="dependencyPool.filter((c) => c.id !== item.ref_id).length === 0" class="dep-empty">
                        {{ $t('noMembers') }}
                      </div>
                    </div>
                  </label>
                </div>
              </div>
              <div v-if="items.length === 0" class="empty-hint">
                {{ $t('noMembers') }}
              </div>
            </div>
          </div>
        </div>

        <div class="dialog-footer">
          <button class="btn" @click="onClose">{{ $t('cancel') }}</button>
          <button class="btn primary" :disabled="saving" @click="onSave">
            {{ $t('saveStack') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Icon } from '@iconify/vue'
import { useStackStore } from '@/stores/stack'
import type {
  Stack,
  StackItem,
  StackItemRefType,
  CreateStackPayload,
  UpdateStackPayload,
} from '@/models/stack'
import { SoftwareCategory } from '@/models/software'

const props = defineProps<{ stack: Stack | null; initialItems?: StackItem[] }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const store = useStackStore()
const isEdit = computed(() => props.stack !== null)

// 可运行的服务类软件（排除 JDK/JRE 等 Runtime 环境软件；自定义软件 category 为 null 也保留）
const candidateSoftware = computed(() =>
  store.installedSoftware.filter((sw) => sw.category !== SoftwareCategory.Runtime)
)

const name = ref('')
const description = ref('')
const items = ref<StackItem[]>([])
const saving = ref(false)
const error = ref<string | null>(null)
const autoStart = ref(false)

// 依赖候选：全部已装可运行软件（组内 + 组外），选中组外依赖启动服务组时自动先拉起
const dependencyPool = computed(() => [
  ...candidateSoftware.value.map((s) => ({ id: s.id, name: s.name })),
  ...store.springbootApps.map((a) => ({ id: a.id, name: a.name })),
])

// 进入时根据 props.stack 初始化表单
watch(
  () => [props.stack, props.initialItems] as [Stack | null, StackItem[] | undefined],
  ([s, initItems]) => {
    error.value = null
    if (s) {
      name.value = s.name
      description.value = s.description
      autoStart.value = s.auto_start
      items.value = s.items.map((i) => ({ ...i, depends_on: [...i.depends_on] }))
    } else {
      name.value = ''
      description.value = ''
      autoStart.value = false
      // 模板预填：优先用 initialItems 副本（深拷贝 depends_on）
      items.value = (initItems ?? []).map((i) => ({
        ...i,
        depends_on: [...i.depends_on],
      }))
    }
  },
  { immediate: true }
)

function isSelected(refType: StackItemRefType, refId: string): boolean {
  return items.value.some((i) => i.ref_type === refType && i.ref_id === refId)
}

function toggle(refType: StackItemRefType, refId: string, _label: string) {
  const idx = items.value.findIndex(
    (i) => i.ref_type === refType && i.ref_id === refId
  )
  if (idx >= 0) {
    items.value.splice(idx, 1)
  } else {
    items.value.push({
      ref_type: refType,
      ref_id: refId,
      order: items.value.length,
      depends_on: [],
      enabled: true,
      retry: 0,
    })
  }
}

function removeMember(refId: string) {
  items.value = items.value.filter((i) => i.ref_id !== refId)
  // 清理其它成员的依赖引用
  for (const it of items.value) {
    it.depends_on = it.depends_on.filter((d) => d !== refId)
  }
}

function resolveName(item: StackItem): string {
  return store.resolveName(item)
}

function reindexOrder() {
  items.value.forEach((it, i) => (it.order = i))
}
function moveUp(idx: number) {
  if (idx <= 0) return
  const arr = items.value
  ;[arr[idx - 1], arr[idx]] = [arr[idx], arr[idx - 1]]
  reindexOrder()
}
function moveDown(idx: number) {
  if (idx >= items.value.length - 1) return
  const arr = items.value
  ;[arr[idx + 1], arr[idx]] = [arr[idx], arr[idx + 1]]
  reindexOrder()
}

// 勾选/取消依赖，并触发按依赖拓扑重排（显示顺序=启动顺序）
function toggleDep(item: StackItem, depId: string) {
  const idx = item.depends_on.indexOf(depId)
  if (idx >= 0) item.depends_on.splice(idx, 1)
  else item.depends_on.push(depId)
  reorderByDependencies()
}

// 依赖变更后按依赖拓扑重排列表，让显示顺序=实际启动顺序（被依赖者在前）
function reorderByDependencies() {
  const byId = new Map(items.value.map((i) => [i.ref_id, i]))
  const visited = new Set<string>()
  const placed: StackItem[] = []
  const visit = (item: StackItem) => {
    if (visited.has(item.ref_id)) return
    visited.add(item.ref_id)
    for (const dep of item.depends_on) {
      const d = byId.get(dep)
      if (d) visit(d) // 先排被依赖者
    }
    placed.push(item)
  }
  for (const it of items.value) visit(it)
  if (placed.length === items.value.length) {
    items.value = placed
    reindexOrder()
  }
}

async function onSave() {
  error.value = null
  if (!name.value.trim()) {
    error.value = $tSafe('stackName')
    return
  }
  // 保存前按依赖拓扑重排，保证入库顺序与启动顺序一致
  reorderByDependencies()
  saving.value = true
  try {
    if (isEdit.value && props.stack) {
      const payload: UpdateStackPayload = {
        name: name.value.trim(),
        description: description.value,
        items: items.value,
        auto_start: autoStart.value,
      }
      await store.updateStack(props.stack.id, payload)
    } else {
      const payload: CreateStackPayload = {
        name: name.value.trim(),
        description: description.value,
        items: items.value,
        auto_start: autoStart.value,
      }
      await store.createStack(payload)
    }
    await store.loadStacks()
    emit('saved')
    onClose()
  } catch (e) {
    // 环检测等错误：提示但不关闭对话框，便于用户修正
    error.value = `${$tSafe('cycleDetected')} (${String(e)})`
  } finally {
    saving.value = false
  }
}

// i18n 在 setup 内无法直接用 $t（非模板作用域），用全局 i18n 取文案
import { i18n } from '@/utils/i18n'
function $tSafe(key: string): string {
  return (i18n.global.t as (k: string) => string)(key)
}

function onClose() {
  emit('close')
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
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 760px;
  max-width: 94vw;
  max-height: 90vh;
  overflow-y: auto;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
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
.error-banner {
  background: color-mix(in oklch, var(--color-danger, red) 15%, transparent);
  border: 1px solid var(--color-danger, red);
  color: var(--color-danger, red);
  padding: 8px 10px;
  border-radius: 6px;
  font-size: 12px;
  margin-bottom: 12px;
}
.field {
  margin-bottom: 12px;
}
.auto-start-row {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--color-foreground);
  margin-bottom: 12px;
  cursor: pointer;
}
.form-field-label {
  display: block;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.text-input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  outline: none;
}
.text-input:focus {
  border-color: var(--color-primary);
}
.columns {
  display: grid;
  grid-template-columns: 1fr 1.3fr;
  gap: 14px;
  margin: 6px 0 12px;
}
.col-title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-muted-foreground);
  margin-bottom: 8px;
}
.col-title .hint {
  font-weight: 400;
  font-size: 11px;
  opacity: 0.7;
}
.candidate-list,
.member-list {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-muted);
  padding: 6px;
  max-height: 320px;
  overflow-y: auto;
}
.candidate {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
}
.candidate:hover {
  background: var(--color-card);
}
.candidate.selected {
  background: var(--color-primary/15);
}
.candidate-name {
  flex: 1;
}
.candidate-meta {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.member {
  position: relative;
  display: block;
  padding: 8px;
  margin-bottom: 6px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-card);
}
.member-head {
  display: flex;
  align-items: center;
  gap: 6px;
}
.move-btns {
  margin-left: auto;
  display: flex;
  gap: 2px;
}
.mini-btn {
  width: 22px;
  height: 22px;
  padding: 0;
  border: 1px solid var(--color-border);
  background: var(--color-muted);
  color: var(--color-foreground);
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  line-height: 1;
}
.mini-btn:hover:not(:disabled) {
  background: var(--color-primary/15);
  color: var(--color-primary);
}
.mini-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.type-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  margin-right: 6px;
}
.type-badge.t-sw {
  background: var(--color-primary/15);
  color: var(--color-primary);
}
.type-badge.t-sb {
  background: color-mix(in oklch, green 15%, transparent);
  color: green;
}
.member-name {
  font-size: 13px;
  font-weight: 500;
}
.mini-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.member-fields {
  display: flex;
  gap: 10px;
  margin-top: 6px;
  align-items: flex-end;
  flex-wrap: wrap;
}
.mini-field {
  display: flex;
  flex-direction: column;
  font-size: 10px;
  color: var(--color-muted-foreground);
  gap: 2px;
}
.mini-field.grow {
  flex: 1;
}
.num {
  width: 60px;
  height: 26px;
  padding: 0 6px;
  background: var(--color-muted);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  color: var(--color-foreground);
  font-size: 12px;
}
.dep-panel {
  width: 100%;
  max-height: 120px;
  overflow-y: auto;
  background: var(--color-muted);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.dep-option {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-foreground);
  padding: 2px 4px;
  border-radius: 3px;
  cursor: pointer;
}
.dep-option:hover {
  background: var(--color-card);
}
.dep-empty {
  font-size: 11px;
  color: var(--color-muted-foreground);
  padding: 6px 4px;
}
.mini-btn.remove:hover:not(:disabled) {
  background: color-mix(in oklch, var(--color-danger, red) 15%, transparent);
  color: var(--color-danger, red);
}
.empty-hint {
  text-align: center;
  color: var(--color-muted-foreground);
  font-size: 12px;
  padding: 14px 0;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
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
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  background: var(--color-muted);
  border-color: var(--color-border);
  color: var(--color-muted-foreground);
}
</style>
