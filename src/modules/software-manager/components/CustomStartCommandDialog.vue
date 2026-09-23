<template>
  <Teleport to="body">
  <div class="overlay" @click.self="onClose">
    <div class="dialog wide">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:console" /> {{ $t('customStartCommand') }}
        </div>
        <button class="dialog-close" @click="onClose">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="dialog-body">
      <div class="hint">{{ $t('customStartCommandHint') }}</div>

      <div class="field">
        <label class="form-field-label">{{ $t('templateLabel') }}</label>
        <select v-model="selectedTemplate" class="input" @change="applyTemplate">
          <option value="">{{ $t('noTemplate') }}</option>
          <option v-for="t in templates" :key="t.id" :value="t.id">
            {{ $t(t.name_i18n) }}
          </option>
        </select>
      </div>

      <div class="field">
        <label class="form-field-label">
          {{ $t('executable') }} <span class="required">*</span>
        </label>
        <div class="input-wrap">
          <input
            v-model="cmd.executable"
            class="input input-mono"
            placeholder="bin/app.exe"
          />
          <button class="browse-btn" @click="browseExecutable">
            <Icon icon="mdi:folder-open" />{{ $t('browse') }}
          </button>
        </div>
      </div>

      <div class="form-grid">
        <div class="field">
          <label class="form-field-label">{{ $t('startArgs') }}</label>
          <input v-model="argsStr" class="input input-mono" placeholder="--port=8080" />
        </div>
        <div class="field">
          <label class="form-field-label">{{ $t('port') }}</label>
          <input v-model.number="healthPort" type="number" class="input tnum" />
        </div>
      </div>

      <div class="field">
        <label class="form-field-label">{{ $t('workingDir') }}</label>
        <div class="input-wrap">
          <input
            v-model="cmd.working_dir"
            class="input input-mono"
            :placeholder="$t('workingDirDefault')"
          />
          <button class="browse-btn" @click="browseWorkingDir">
            <Icon icon="mdi:folder-open" />{{ $t('browse') }}
          </button>
        </div>
      </div>

      <div class="field">
        <label class="form-field-label">{{ $t('envVars') }}</label>
        <div class="env-list">
          <div v-for="(env, i) in envList" :key="i" class="env-row">
            <input v-model="env.key" class="env-input" placeholder="KEY" />
            <input v-model="env.value" class="env-input" placeholder="VALUE" />
            <button class="env-remove" @click="envList.splice(i, 1)">
              <Icon icon="mdi:close" />
            </button>
          </div>
        </div>
        <button class="env-add" @click="envList.push({ key: '', value: '' })">
          <Icon icon="mdi:plus" />{{ $t('add') }}
        </button>
      </div>

      <div class="field">
        <label class="form-field-label">{{ $t('healthCheck') }}</label>
        <div class="radio-group">
          <label>
            <input v-model="healthKind" type="radio" value="none" />
            {{ $t('noHealthCheck') }}
          </label>
          <label>
            <input v-model="healthKind" type="radio" value="tcp" />
            {{ $t('tcpPort') }}
          </label>
          <label>
            <input v-model="healthKind" type="radio" value="http" />
            {{ $t('httpUrl') }}
          </label>
        </div>
        <input
          v-if="healthKind === 'http'"
          v-model="healthUrl"
          class="input input-mono"
          placeholder="http://127.0.0.1:8080/health"
        />
        <input
          v-if="healthKind === 'http'"
          v-model.number="expectedStatus"
          type="number"
          class="input tnum"
          placeholder="200"
        />
      </div>

      <div class="field">
        <label class="form-field-label">{{ $t('customConfigFile') }}</label>
        <div class="input-wrap">
          <input
            v-model="cmd.config_file_relative"
            class="input input-mono"
            placeholder="conf/app.conf"
          />
          <button class="browse-btn" @click="browseConfigFile">
            <Icon icon="mdi:folder-open" />{{ $t('browse') }}
          </button>
        </div>
      </div>

      </div>
      <div class="dialog-footer">
        <button class="btn" @click="onClose">{{ $t('cancel') }}</button>
        <button
          class="btn primary"
          :disabled="!cmd.executable || saving"
          @click="onSave"
        >
          {{ $t('saveAndStart') }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  CustomHealthSpec,
  CustomStartCommand,
  CustomTemplate,
  InstalledSoftware,
} from '@/models/software'

const props = defineProps<{ software: InstalledSoftware }>()
const emit = defineEmits<{ close: []; saved: [] }>()

const cmd = ref<CustomStartCommand>({
  executable: '',
  args: [],
  working_dir: null,
  env_vars: {},
  health_check: { kind: 'None', spec: null },
  config_file_relative: null,
})
const argsStr = ref('')
const envList = ref<Array<{ key: string; value: string }>>([])
const templates = ref<CustomTemplate[]>([])
const selectedTemplate = ref('')
const healthKind = ref<'none' | 'tcp' | 'http'>('none')
const healthPort = ref(8080)
const healthUrl = ref('')
const expectedStatus = ref(200)
const saving = ref(false)

onMounted(async () => {
  try {
    templates.value = await invoke<CustomTemplate[]>('list_custom_templates')
  } catch (e) {
    console.error('load templates failed:', e)
  }

  if (props.software.custom_start_command) {
    cmd.value = { ...props.software.custom_start_command }
    argsStr.value = cmd.value.args.join(' ')
    envList.value = Object.entries(cmd.value.env_vars).map(([k, v]) => ({
      key: k,
      value: v,
    }))
    // 反向解析 health_check（serde tag=kind, content=spec 格式）
    const hc: CustomHealthSpec = cmd.value.health_check
    if (hc.kind === 'Tcp') {
      healthKind.value = 'tcp'
      healthPort.value = hc.spec.port
    } else if (hc.kind === 'Http') {
      healthKind.value = 'http'
      healthUrl.value = hc.spec.url
      expectedStatus.value = hc.spec.expected_status
    } else {
      healthKind.value = 'none'
    }
  }
})

function applyTemplate() {
  const t = templates.value.find((t) => t.id === selectedTemplate.value)
  if (!t) return
  cmd.value.executable = t.executable
  argsStr.value = t.args.join(' ')
  cmd.value.config_file_relative = t.config_file_relative
}

// 把绝对路径转为相对 install_path 的路径（Windows 兼容 / 和 \）
function toRelative(absPath: string): string {
  const base = props.software.install_path
  let rel = absPath
  if (rel.startsWith(base + '/')) rel = rel.slice(base.length + 1)
  else if (rel.startsWith(base + '\\')) rel = rel.slice(base.length + 1)
  return rel
}

async function browseExecutable() {
  const path = await open({ directory: false, multiple: false })
  if (typeof path === 'string') {
    cmd.value.executable = toRelative(path)
  }
}

async function browseWorkingDir() {
  const path = await open({ directory: true, multiple: false })
  if (typeof path === 'string') {
    cmd.value.working_dir = toRelative(path)
  }
}

async function browseConfigFile() {
  const path = await open({ directory: false, multiple: false })
  if (typeof path === 'string') {
    cmd.value.config_file_relative = toRelative(path)
  }
}

async function onSave() {
  saving.value = true
  cmd.value.args = argsStr.value.split(/\s+/).filter(Boolean)
  cmd.value.env_vars = Object.fromEntries(
    envList.value.filter((e) => e.key).map((e) => [e.key, e.value]),
  )
  // 构造 health_check（serde tag=kind, content=spec 格式）
  if (healthKind.value === 'none') {
    cmd.value.health_check = { kind: 'None', spec: null }
  } else if (healthKind.value === 'tcp') {
    cmd.value.health_check = { kind: 'Tcp', spec: { port: healthPort.value } }
  } else {
    cmd.value.health_check = {
      kind: 'Http',
      spec: {
        url: healthUrl.value,
        expected_status: expectedStatus.value,
      },
    }
  }

  try {
    await invoke('save_custom_start_command', {
      installedId: props.software.id,
      cmd: cmd.value,
    })
    emit('saved')
    emit('close')
  } catch (e) {
    console.error('save failed:', e)
  } finally {
    saving.value = false
  }
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
  width: 680px;
  max-height: 90vh;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 8px 24px oklch(0 0 0 / 0.45);
  padding: 20px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
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
.hint {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 14px;
}
.field {
  margin-bottom: 14px;
}
.form-field-label {
  display: block;
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.form-field-label .required {
  color: var(--color-destructive);
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
  outline: none;
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', monospace;
  font-size: 12px;
}
.tnum {
  font-variant-numeric: tabular-nums;
}
.input-wrap {
  position: relative;
}
.browse-btn {
  position: absolute;
  right: 4px;
  top: 50%;
  transform: translateY(-50%);
  height: 26px;
  padding: 0 8px;
  font-size: 11px;
  background: var(--color-card);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  cursor: pointer;
  color: var(--color-muted-foreground);
  display: flex;
  align-items: center;
  gap: 4px;
}
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
.env-list {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-muted);
  margin-top: 6px;
}
.env-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--color-border);
}
.env-row:last-child {
  border-bottom: none;
}
.env-input {
  flex: 1;
  height: 26px;
  padding: 0 8px;
  background: var(--color-card);
  border: 1px solid transparent;
  border-radius: 4px;
  color: var(--color-foreground);
  font-size: 12px;
  font-family: ui-monospace, monospace;
}
.env-remove {
  width: 24px;
  height: 24px;
  background: transparent;
  border: none;
  color: var(--color-muted-foreground);
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.env-add {
  margin-top: 6px;
  padding: 4px 10px;
  height: 26px;
  font-size: 12px;
  background: transparent;
  border: 1px dashed var(--color-border);
  border-radius: 4px;
  cursor: pointer;
  color: var(--color-muted-foreground);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.radio-group {
  display: flex;
  gap: 16px;
  padding: 4px 0;
}
.radio-group label {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 13px;
  cursor: pointer;
}
.dialog-body {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
.dialog-footer {
  flex: none;
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
