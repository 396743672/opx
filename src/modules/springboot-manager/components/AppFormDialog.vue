<template>
  <Teleport to="body">
    <div class="overlay">
      <div class="dialog">
        <div class="dialog-head">
          <div class="dialog-title">
            <Icon :icon="app ? 'mdi:pencil' : 'mdi:plus'" />
            {{ app ? $t('editApp') : $t('registerApp') }}
          </div>
          <button class="dialog-close" @click="$emit('cancel')">
            <Icon icon="mdi:close" />
          </button>
        </div>

        <div class="dialog-body">
          <!-- JAR file -->
          <div class="field">
            <div class="field-label">{{ $t('jarFile') }}</div>
            <div class="file-row">
              <input class="input input-mono flex-1" :value="form.jar_path" readonly placeholder="..." />
              <button class="btn" @click="selectJar">{{ $t('browse') }}</button>
            </div>
          </div>

          <!-- App name -->
          <div class="field">
            <div class="field-label">{{ $t('appName') }}</div>
            <input class="input" v-model="form.name" />
          </div>

          <!-- JDK select -->
          <div class="field">
            <div class="field-label">{{ $t('jdk') }}</div>
            <select class="input" v-model="form.jdk_installed_id" @change="onJdkChange">
              <option value="" disabled>{{ $t('selectJdk') }}</option>
              <option v-for="jdk in jdkList" :key="jdk.id" :value="jdk.id">
                {{ jdk.name }} ({{ jdk.version }})
              </option>
            </select>
          </div>

          <!-- JVM parameters -->
          <div class="section-title">{{ $t('jvmParameters') }}</div>
          <div class="grid-3">
            <div class="field">
              <div class="field-label">-Xms (MB)</div>
              <input class="input" type="number" v-model.number="jvm.xms_mb" min="64" />
            </div>
            <div class="field">
              <div class="field-label">-Xmx (MB)</div>
              <input class="input" type="number" v-model.number="jvm.xmx_mb" min="64" />
            </div>
            <div class="field">
              <div class="field-label">Metaspace (MB)</div>
              <input class="input" type="number" v-model.number="jvm.metaspace_mb" min="16" />
            </div>
          </div>
          <div class="grid-2">
            <div class="field">
              <div class="field-label">{{ $t('gcType') }}</div>
              <select class="input" v-model="jvm.gc_type">
                <option value="G1GC">G1GC</option>
                <option value="ParallelGC">ParallelGC</option>
                <option value="ZGC">ZGC</option>
                <option value="ShenandoahGC">ShenandoahGC</option>
                <option value="SerialGC">SerialGC</option>
              </select>
            </div>
            <div class="field">
              <div class="field-label">{{ $t('extraFlags') }}</div>
              <input class="input input-mono" v-model="extraFlagsText" placeholder="-XX:+HeapDumpOnOOMError ..." />
            </div>
          </div>

          <!-- Port + Profile -->
          <div class="grid-2">
            <div class="field">
              <div class="field-label">{{ $t('port') }}</div>
              <input class="input" type="number" v-model.number="form.port" min="1" max="65535" />
            </div>
            <div class="field">
              <div class="field-label">Profile</div>
              <input class="input" v-model="form.profile" placeholder="prod" />
            </div>
          </div>

          <!-- Program args -->
          <div class="field">
            <div class="field-label">{{ $t('programArgs') }}</div>
            <textarea class="textarea input-mono" v-model="programArgsText" rows="2" placeholder="--server.port=8080" />
          </div>

          <!-- Environment variables -->
          <div class="field">
            <div class="field-label">{{ $t('environmentVariables') }}</div>
            <div v-for="(env, i) in form.env_vars" :key="i" class="env-row">
              <input class="input input-mono env-key" v-model="env[0]" placeholder="KEY" />
              <input class="input input-mono env-val" v-model="env[1]" placeholder="VALUE" />
              <button class="btn icon-btn" @click="removeEnv(i)">
                <Icon icon="mdi:close" />
              </button>
            </div>
            <button class="btn btn-add" @click="addEnv">
              <Icon icon="mdi:plus" /> {{ $t('add') }}
            </button>
          </div>

          <!-- Log path -->
          <div class="field">
            <div class="field-label">{{ $t('logPath') }}</div>
            <input class="input input-mono" v-model="form.log_path" placeholder="logs/app.log" />
          </div>

          <!-- Dependencies -->
          <div class="field">
            <div class="field-label">{{ $t('dependencies') }}</div>
            <div v-if="store.dependencyCandidates.length === 0" class="text-muted text-sm">
              {{ $t('noDependencyCandidates') }}
            </div>
            <div v-else class="checkbox-grid">
              <label v-for="dep in store.dependencyCandidates" :key="dep.id" class="checkbox-label">
                <input type="checkbox" :value="dep.id" v-model="form.dependencies" class="checkbox" />
                {{ dep.name }}
              </label>
            </div>
          </div>

          <!-- Advanced settings -->
          <details class="advanced">
            <summary class="advanced-summary">{{ $t('advancedSettings') }}</summary>
            <div class="advanced-body">
              <label class="checkbox-label">
                <input type="checkbox" v-model="form.auto_start" class="checkbox" />
                {{ $t('autoStart') }}
              </label>
              <div v-if="form.auto_start" class="field">
                <div class="field-label">{{ $t('startupOrder') }}</div>
                <input class="input" type="number" v-model.number="form.startup_order" min="0" />
              </div>
              <label class="checkbox-label">
                <input type="checkbox" v-model="form.auto_restart" class="checkbox" />
                {{ $t('autoRestart') }}
              </label>
              <div class="field">
                <div class="field-label">{{ $t('group') }}</div>
                <select class="input" v-model="form.group">
                  <option :value="null">{{ $t('noGroup') }}</option>
                  <option v-for="g in store.groups" :key="g.id" :value="g.name">
                    {{ g.name }}
                  </option>
                </select>
              </div>
            </div>
          </details>
        </div>

        <div v-if="saveError" class="error-banner">{{ saveError }}</div>
        <div class="dialog-footer">
          <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
          <button class="btn primary" @click="save" :disabled="!valid || saving">
            {{ saving ? $t('saving') : $t('save') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { Icon } from '@iconify/vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import { useSpringBootStore } from '../stores/springboot'
import type { SpringBootApp, JvmOptsTemplate } from '@/models/springboot'

const { t } = useI18n()

const props = withDefaults(defineProps<{
  app?: SpringBootApp | null
}>(), {
  app: null,
})

const emit = defineEmits<{
  saved: [app: SpringBootApp]
  cancel: []
}>()

const store = useSpringBootStore()
const saving = ref(false)
const saveError = ref('')

// 仅展示 JDK/JRE，过滤掉 MySQL/Redis 等其他已安装软件
const jdkList = computed(() =>
  store.jdkList.filter(j => j.key === 'jre')
)

// JVM structured state
const jvm = reactive<JvmOptsTemplate>({
  xms_mb: 512,
  xmx_mb: 512,
  metaspace_mb: 128,
  gc_type: 'G1GC',
  extra_flags: ['-Dfile.encoding=UTF-8'],
})

const extraFlagsText = ref('')

function parseJvmOpts(opts: string[]): JvmOptsTemplate {
  const result: JvmOptsTemplate = { xms_mb: 512, xmx_mb: 512, metaspace_mb: 128, gc_type: 'G1GC', extra_flags: ['-Dfile.encoding=UTF-8'] }
  const knownGc = ['G1GC', 'ZGC', 'ParallelGC', 'ShenandoahGC', 'SerialGC']
  for (const opt of opts) {
    if (opt.startsWith('-Xms')) {
      result.xms_mb = parseInt(opt.slice(4).replace(/[gm]/g, '')) || 256
    } else if (opt.startsWith('-Xmx')) {
      result.xmx_mb = parseInt(opt.slice(4).replace(/[gm]/g, '')) || 1024
    } else if (opt.startsWith('-XX:MetaspaceSize=')) {
      result.metaspace_mb = parseInt(opt.slice(18).replace('m', '')) || 128
    } else if (knownGc.some(gc => opt === `-XX:+Use${gc}`)) {
      result.gc_type = opt.slice(7)
    } else {
      result.extra_flags.push(opt)
    }
  }
  return result
}

function buildJvmOpts(): string[] {
  const extraFlags = extraFlagsText.value
    ? extraFlagsText.value.split(/\s+/).filter(Boolean)
    : [...jvm.extra_flags]
  return [
    `-Xms${jvm.xms_mb}m`,
    `-Xmx${jvm.xmx_mb}m`,
    `-XX:MetaspaceSize=${jvm.metaspace_mb}m`,
    `-XX:+Use${jvm.gc_type}`,
    ...extraFlags,
  ]
}

// Form state
const form = reactive({
  jar_path: '',
  name: '',
  jdk_installed_id: '',
  port: null as number | null,
  profile: '',
  env_vars: [] as [string, string][],
  log_path: '',
  dependencies: [] as string[],
  auto_start: false,
  startup_order: 0,
  auto_restart: false,
  group: null as string | null,
})

const programArgsText = ref('')

const valid = computed(() => {
  return form.jar_path.trim() !== '' && form.name.trim() !== '' && form.jdk_installed_id !== ''
})

onMounted(() => {
  store.fetchJdkList()
  store.fetchDependencyCandidates()
  store.fetchGroups()

  if (props.app) {
    form.jar_path = props.app.jar_path
    form.name = props.app.name
    form.jdk_installed_id = props.app.jdk_installed_id
    form.port = props.app.port
    form.profile = props.app.profile
    form.env_vars = props.app.env_vars.map(e => [...e] as [string, string])
    form.log_path = props.app.log_path
    form.dependencies = [...props.app.dependencies]
    form.auto_start = props.app.auto_start
    form.startup_order = props.app.startup_order
    form.auto_restart = props.app.auto_restart
    form.group = props.app.group
    programArgsText.value = props.app.program_args.join('\n')

    const parsed = parseJvmOpts(props.app.jvm_opts)
    jvm.xms_mb = parsed.xms_mb
    jvm.xmx_mb = parsed.xmx_mb
    jvm.metaspace_mb = parsed.metaspace_mb
    jvm.gc_type = parsed.gc_type
    extraFlagsText.value = parsed.extra_flags.join(' ')
  }
})

async function selectJar() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'JAR', extensions: ['jar'] }],
  })
  if (selected && typeof selected === 'string') {
    form.jar_path = selected
    if (!form.name) {
      const parts = selected.split(/[\\/]/)
      const filename = parts[parts.length - 1] || ''
      form.name = filename.replace(/\.jar$/i, '')
    }
  }
}

async function onJdkChange() {
  if (!form.jdk_installed_id) return
  try {
    const recommended = await store.getRecommendedOpts(form.jdk_installed_id)
    jvm.xms_mb = recommended.xms_mb
    jvm.xmx_mb = recommended.xmx_mb
    jvm.metaspace_mb = recommended.metaspace_mb
    jvm.gc_type = recommended.gc_type
    extraFlagsText.value = recommended.extra_flags.join(' ')
  } catch (e) {
    console.error('Failed to get recommended JVM opts:', e)
  }
}

function addEnv() {
  form.env_vars.push(['', ''])
}

function removeEnv(index: number) {
  form.env_vars.splice(index, 1)
}

async function save() {
  if (!valid.value || saving.value) return
  saving.value = true
  saveError.value = ''
  try {
    const jvmOpts = buildJvmOpts()
    const programArgs = programArgsText.value
      ? programArgsText.value.split('\n').map(s => s.trim()).filter(Boolean)
      : []

    // ponytail: port 可为 null，向后端传 None
    const safePort = typeof form.port === 'number' && form.port > 0 ? form.port : null

    if (props.app) {
      const updated = await store.updateApp(props.app.id, {
        name: form.name,
        jdk_installed_id: form.jdk_installed_id,
        jvm_opts: jvmOpts,
        program_args: programArgs,
        profile: form.profile,
        env_vars: form.env_vars,
        port: safePort,
        log_path: form.log_path,
        dependencies: form.dependencies,
        auto_start: form.auto_start,
        startup_order: form.startup_order,
        auto_restart: form.auto_restart,
        group: form.group,
      })
      emit('saved', updated)
    } else {
      const created = await store.createApp({
        jar_path: form.jar_path,
        name: form.name,
        jdk_installed_id: form.jdk_installed_id,
        jvm_opts: jvmOpts,
        program_args: programArgs,
        profile: form.profile,
        env_vars: form.env_vars,
        port: safePort,
        log_path: form.log_path,
        dependencies: form.dependencies,
        auto_start: form.auto_start,
        startup_order: form.startup_order,
        auto_restart: form.auto_restart,
        group: form.group,
      })
      emit('saved', created)
    }
  } catch (e: any) {
    saveError.value = typeof e === 'string' ? e : (e?.message || t('saveFailed'))
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
  animation: fade 0.15s ease-out;
}
@keyframes fade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.dialog {
  width: 520px;
  max-height: 85vh;
  border-radius: 10px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: var(--shadow-popover);
  display: flex;
  flex-direction: column;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 20px 20px 0;
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
.dialog-body {
  overflow-y: auto;
  padding: 16px 20px 0;
  flex: 1;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 14px 20px;
  border-top: 1px solid var(--color-border);
}
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-bottom: 6px;
}
.section-title {
  font-size: 13px;
  font-weight: 600;
  margin-bottom: 10px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--color-border);
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
  box-sizing: border-box;
}
.input:focus {
  border-color: var(--color-primary);
}
.input[readonly] {
  color: var(--color-muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.textarea {
  width: 100%;
  padding: 8px 10px;
  background: var(--color-muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-foreground);
  font-size: 13px;
  outline: none;
  resize: vertical;
  box-sizing: border-box;
  line-height: 1.4;
}
.textarea:focus {
  border-color: var(--color-primary);
}
.grid-2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.grid-3 {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 10px;
}
.file-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.flex-1 {
  flex: 1;
}
.env-row {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: 6px;
}
.env-key {
  flex: 2;
  min-width: 0;
}
.env-val {
  flex: 3;
  min-width: 0;
}
.checkbox-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.checkbox-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: background 0.15s;
}
.checkbox-label:hover {
  background: var(--color-muted);
}
.checkbox {
  accent-color: var(--color-primary);
}
.advanced {
  margin-bottom: 14px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  overflow: hidden;
}
.advanced-summary {
  font-size: 12px;
  font-weight: 600;
  padding: 8px 12px;
  cursor: pointer;
  background: var(--color-muted);
  color: var(--color-muted-foreground);
}
.advanced-summary:hover {
  color: var(--color-foreground);
}
.advanced-body {
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.text-muted {
  color: var(--color-muted-foreground);
}
.text-sm {
  font-size: 12px;
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
  white-space: nowrap;
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
.icon-btn {
  width: 32px;
  padding: 0;
  justify-content: center;
  flex-shrink: 0;
}
.btn-add {
  width: 100%;
  justify-content: center;
  border-style: dashed;
  color: var(--color-muted-foreground);
  font-size: 12px;
}
.btn-add:hover {
  color: var(--color-foreground);
  border-color: var(--color-primary);
}
select.input {
  appearance: auto;
}
.error-banner {
  margin: 0 20px;
  padding: 8px 12px;
  background: #fef2f2;
  color: #b91c1c;
  border-radius: 6px;
  font-size: 12px;
}
</style>
