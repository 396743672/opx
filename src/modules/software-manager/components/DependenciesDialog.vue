<template>
  <Teleport to="body">
  <div class="overlay" @click.self="$emit('close')">
    <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:graph-outline" /> {{ $t('depsEdit') }}
        </div>
        <button class="dialog-close" @click="$emit('close')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="software-name">
        <b>{{ software.name }} {{ software.version }}</b>
        <div class="path">{{ software.install_path }}</div>
      </div>

      <div class="desc line">{{ $t('depsDesc') }}</div>

      <!-- 已选依赖 -->
      <div v-if="selected.length" class="selected-wrap">
        <span v-for="id in selected" :key="id" class="sel-chip">
          {{ idName(id) }}
          <button class="sel-x" @click="toggle(id)"><Icon icon="mdi:close" /></button>
        </span>
      </div>

      <!-- 依赖选择列表 -->
      <div class="dep-list">
        <label v-for="opt in candidates" :key="opt.id" class="dep-opt">
          <input
            type="checkbox"
            :checked="selected.includes(opt.id)"
            @change="toggle(opt.id)"
          />
          <span class="opt-name">{{ opt.name }}</span>
          <span class="opt-ver">{{ opt.version }}</span>
        </label>
        <div v-if="!candidates.length" class="empty">{{ $t('noCandidates') }}</div>
      </div>

      <!-- 拓扑校验反馈 -->
      <div v-if="topology" class="topo">
        <div v-if="topology.cycle?.length" class="topo-err">
          <Icon icon="mdi:alert-circle" /> {{ $t('depsCycle', { cycle: topology.cycle.join(' -> ') }) }}
        </div>
        <div v-if="topology.missing?.length" class="topo-err">
          <Icon icon="mdi:alert-circle" /> {{ $t('depsMissing', { missing: topology.missing.join(', ') }) }}
        </div>
        <div v-else-if="topology.layers?.length" class="topo-ok">
          <Icon icon="mdi:check-circle" /> {{ $t('depsTopology', { layers: topology.layers.length }) }}
        </div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('close')">{{ $t('cancel') }}</button>
        <button class="btn primary" :disabled="saving" @click="onSave">
          {{ $t('save') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { SoftwareCategory, type InstalledSoftware } from '@/models/software'

const props = defineProps<{
  software: InstalledSoftware
  allSoftware: InstalledSoftware[]
}>()
const emit = defineEmits<{ close: [] }>()

const selected = ref<string[]>([])
const saving = ref(false)
const topology = ref<{ layers: string[][]; cycle: string[] | null; missing: string[] } | null>(null)

// 可选作依赖的软件：排除自身 与 Runtime 类（运行时由 SpringBoot 拉起，不参与软件依赖）
const candidates = computed(() =>
  props.allSoftware.filter(
    (s) => s.id !== props.software.id && s.category !== SoftwareCategory.Runtime,
  ),
)

function idName(id: string) {
  return props.allSoftware.find((s) => s.id === id)?.name || id
}

function toggle(id: string) {
  const i = selected.value.indexOf(id)
  if (i >= 0) selected.value.splice(i, 1)
  else selected.value.push(id)
}

async function refreshTopology() {
  try {
    const res = await invoke<any>('resolve_software_deps', {
      installedId: props.software.id,
    })
    if (res?.cycle) {
      topology.value = { layers: [], cycle: res.cycle, missing: res.missing ?? [] }
    } else {
      topology.value = { layers: res.layers ?? [], cycle: null, missing: res.missing ?? [] }
    }
  } catch {
    topology.value = null
  }
}

onMounted(() => {
  selected.value = [...(props.software.depends_on ?? [])]
  refreshTopology()
})

async function onSave() {
  saving.value = true
  try {
    await invoke('update_software_deps', {
      installedId: props.software.id,
      dependsOn: selected.value,
    })
    emit('close')
  } catch (e) {
    console.error('save dependencies failed:', e)
  } finally {
    saving.value = false
  }
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
  width: 460px;
  max-height: 84vh;
  display: flex;
  flex-direction: column;
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
  margin-bottom: 16px;
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
  background: var(--color-destructive);
  color: var(--color-destructive-foreground);
}
.software-name {
  font-size: 13px;
  margin-bottom: 10px;
}
.path {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 2px;
  font-family: ui-monospace, monospace;
  word-break: break-all;
}
.desc {
  font-size: 12px;
  color: var(--color-muted-foreground);
}
.desc.line {
  margin-bottom: 12px;
}
.selected-wrap {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 10px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-muted);
  margin-bottom: 12px;
}
.sel-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  padding: 3px 8px;
  border-radius: 999px;
  background: color-mix(in oklch, var(--color-primary) 14%, transparent);
  color: var(--color-primary);
  font-weight: 500;
}
.sel-x {
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  display: inline-flex;
  padding: 0 2px;
}
.sel-x svg {
  width: 12px;
  height: 12px;
}
.sel-x:hover {
  opacity: 0.7;
}
.dep-list {
  overflow-y: auto;
  flex: 1;
  min-height: 120px;
  max-height: 240px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 4px;
}
.dep-opt {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
}
.dep-opt:hover {
  background: var(--color-muted);
}
.opt-name {
  flex: 1;
}
.opt-ver {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.empty {
  padding: 16px;
  text-align: center;
  color: var(--color-muted-foreground);
  font-size: 12px;
}
.topo {
  margin-top: 12px;
  font-size: 12px;
}
.topo-err {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--color-destructive);
  margin-bottom: 4px;
}
.topo-ok {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--color-success);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
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
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
