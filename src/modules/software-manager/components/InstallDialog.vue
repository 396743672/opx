<template>
  <Teleport to="body">
    <div class="overlay" @click.self="$emit('cancel')">
      <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:package-variant-closed" />
          {{ $t('install') }} {{ entry.name }}
        </div>
        <button class="dialog-close" @click="$emit('cancel')">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label" style="display:flex;align-items:center;gap:8px">
          <span>{{ $t('selectVersion') }}</span>
          <button class="refresh-btn" :disabled="fetchingVersions" @click="refreshVersions" :title="$t('refresh')">
            <Icon :icon="fetchingVersions ? 'mdi:loading' : 'mdi:refresh'" :class="{ spinning: fetchingVersions }" style="font-size:14px" />
          </button>
        </div>
        <div class="select" @click="showVersionDropdown = !showVersionDropdown">
          <span>{{ selectedVersion?.version }}</span>
          <Icon icon="mdi:chevron-down" class="caret" />
        </div>
        <div v-if="showVersionDropdown" class="dropdown version-dropdown">
          <div
            v-for="(v, idx) in mergedVersions"
            :key="v.version"
            class="dropdown-item"
            :class="{ selected: selectedVersionIdx === idx }"
            @click="selectVersion(idx)"
          >
            <span>{{ v.version }}</span>
            <span v-if="isBuiltinVersion(v)" class="builtin-tag">{{ $t('offline') }}</span>
            <span v-else-if="v.version === 'latest'" class="v-badge">{{ $t('latestVersion') }}</span>
          </div>
        </div>
        <div v-if="fetchingVersions" class="fetching-hint">
          <Icon icon="mdi:loading" class="spinning" />
          <span>{{ $t('fetchingVersions') }}</span>
        </div>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('selectMirror') }}</div>
        <div class="select" @click="showMirrorDropdown = !showMirrorDropdown">
          <Icon v-if="selectedMirror?.builtin" icon="mdi:package-variant-closed" class="builtin-icon" />
          <span>{{ mirrorName(selectedMirror?.name || '') }}</span>
          <span v-if="selectedMirror?.builtin" class="builtin-tag">{{ $t('offline') }}</span>
          <Icon icon="mdi:chevron-down" class="caret" />
        </div>
        <div v-if="showMirrorDropdown" class="dropdown">
          <div
            v-for="(m, idx) in selectedVersion?.mirrors"
            :key="idx"
            class="dropdown-item"
            :class="{ selected: selectedMirrorIdx === idx }"
            @click="selectMirror(idx)"
          >
            <Icon v-if="m.builtin" icon="mdi:package-variant-closed" class="builtin-icon" />
            <span>{{ mirrorName(m.name) }}</span>
            <span v-if="m.builtin" class="builtin-tag">{{ $t('offline') }}</span>
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">{{ $t('installPath') }}</div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
        <button class="btn primary" @click="install" :disabled="installing">
          {{ $t('install') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
  <Teleport to="body">
    <div v-if="fetchError" class="overlay" style="z-index:70" @click.self="fetchError = ''">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ fetchError }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="fetchError = ''">{{ $t('confirm') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useInstallStore } from '../stores/install'
import { useCatalogStore } from '../stores/catalog'
import type { CatalogEntry, CatalogVersion } from '@/models/software'

const props = defineProps<{
  entry: CatalogEntry
}>()

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const { t } = useI18n()

// 镜像名 i18n 转译：后端返回 "i18n:key" 格式时调 t() 转译
function mirrorName(name: string): string {
  if (name.startsWith('i18n:')) {
    return t(name.slice(5))
  }
  return name
}

const selectedVersionIdx = ref(0)
const selectedMirrorIdx = ref(0)
const showMirrorDropdown = ref(false)
const installing = ref(false)
const showVersionDropdown = ref(false)
const fetchingVersions = ref(false)
const fetchError = ref<string | null>(null)
const remoteVersions = ref<CatalogVersion[]>([])

// 版本号比较：支持如 "17.0.16"、"1.8"、"8.4.10"、"RELEASE.2025-04-22" 等
function compareVersion(a: string, b: string): number {
  // 提取数字部分
  const numA = (a.match(/\d+/g) || []).map(Number)
  const numB = (b.match(/\d+/g) || []).map(Number)
  const len = Math.max(numA.length, numB.length)
  for (let i = 0; i < len; i++) {
    const va = numA[i] ?? 0
    const vb = numB[i] ?? 0
    if (va !== vb) return vb - va  // 降序
  }
  return 0
}

// 合并版本列表：内置（entry.versions）+ 远程拉取的版本（去重 + 降序）
const mergedVersions = computed(() => {
  const existing = new Set(props.entry.versions.map(v => v.version))
  // 内置版本在前（保持原顺序）
  const builtin = [...props.entry.versions]
  // 网络版本去重 + 降序排序
  const remote = remoteVersions.value
    .filter(v => !existing.has(v.version))
    .sort((a, b) => compareVersion(a.version, b.version))
  return [...builtin, ...remote]
})

function isBuiltinVersion(v: CatalogVersion): boolean {
  return v.mirrors.some(m => m.builtin)
}

function selectVersion(idx: number) {
  selectedVersionIdx.value = idx
  selectedMirrorIdx.value = 0
  showVersionDropdown.value = false
}

const selectedVersion = computed(() => mergedVersions.value[selectedVersionIdx.value])
const selectedMirror = computed(() => selectedVersion.value?.mirrors[selectedMirrorIdx.value])
const installPath = computed(
  () => `apps/${props.entry.key}/${selectedVersion.value?.version}`,
)

function selectMirror(idx: number) {
  selectedMirrorIdx.value = idx
  showMirrorDropdown.value = false
}

async function fetchRemoteVersions() {
  fetchingVersions.value = true
  fetchError.value = null
  try {
    const result = await invoke('fetch_remote_versions_for', { key: props.entry.key }) as CatalogEntry[]
    if (result.length > 0) {
      remoteVersions.value = result[0].versions
    }
    // ponytail: 回写 catalog store，关闭再打开版本不丢失
    useCatalogStore().loadCatalog()
  } catch (e) {
    fetchError.value = String(e)
    console.error('Failed to fetch remote versions:', e)
  } finally {
    fetchingVersions.value = false
  }
}

onMounted(() => {
  // ponytail: 缓存优先，不自动拉取，用户点刷新按钮时拉取
})

async function refreshVersions() {
  await fetchRemoteVersions()
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_software', {
      params: {
        key: props.entry.key,
        version: selectedVersion.value.version,
        mirror_index: selectedMirrorIdx.value,
        set_as_default_jre: false,
      },
    }) as string
    useInstallStore().createTask(
      installId,
      props.entry.key,
      `${props.entry.name} ${selectedVersion.value.version}`,
    )
    emit('installed', installId)
  } catch (e) {
    console.error('Failed to install:', e)
  } finally {
    installing.value = false
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
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
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
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.select {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.select span:first-of-type {
  flex: 1;
}
.select:hover {
  border-color: var(--color-primary);
}
.caret {
  color: var(--color-muted-foreground);
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
}
.input.readonly {
  color: var(--color-muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.version-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.version-row:hover {
  background: var(--color-muted);
}
.version-row.selected {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.v-name {
  flex: 1;
  font-size: 13px;
}
.v-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--color-info) 14%, transparent);
  color: var(--color-info);
}
.v-check {
  width: 16px;
  height: 16px;
}
.dropdown {
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-top: 4px;
  box-shadow: var(--shadow-popover);
}
.dropdown-item {
  padding: 8px 12px;
  cursor: pointer;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.dropdown-item:hover {
  background: var(--color-muted);
}
.dropdown-item.selected {
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.version-dropdown {
  max-height: 240px;
  overflow-y: auto;
}
.fetching-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 6px;
}
.fetch-error {
  font-size: 12px;
  color: var(--color-destructive);
  margin-top: 6px;
}
.spinning {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
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
.btn.primary:hover {
  background: color-mix(in oklch, var(--color-primary) 88%, var(--color-background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.builtin-icon {
  width: 16px;
  height: 16px;
  color: var(--color-primary);
  flex-shrink: 0;
}
.builtin-tag {
  margin-left: auto;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  background: color-mix(in oklch, var(--color-primary) 12%, transparent);
  color: var(--color-primary);
}
.refresh-btn {
  border: none;
  background: transparent;
  color: var(--color-muted-foreground);
  cursor: pointer;
  padding: 2px;
  border-radius: 4px;
  display: flex;
  align-items: center;
}
.refresh-btn:hover { color: var(--color-primary); background: var(--color-muted); }
</style>
