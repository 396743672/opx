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
          <!-- JAR file - 仅用于上传/替换包，始终为空 -->
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
            <input class="input" :value="form.name" :readonly="!!app" @input="onNameInput" :class="{ 'border-destructive': nameError }" />
            <div v-if="nameError" class="text-xs text-destructive mt-1">{{ nameError }}</div>
            <div v-else class="text-xs hint mt-1">{{ $t('siteNameHint') }}</div>
          </div>

          <!-- JDK select -->
          <div class="field">
            <div class="field-label">{{ $t('jdk') }}</div>
            <select class="input" v-model="form.jdk_installed_id" @change="onJdkChange">
              <option value="" disabled>{{ $t('selectJdk') }}</option>
              <option v-for="j in jdkList" :key="j.id" :value="j.id">
                {{ j.name }} ({{ j.version }}) {{ j.key === 'jdk' ? '[JDK]' : '[JRE]' }}
              </option>
            </select>
            <!-- Spring Boot 3.x/4.x 要求 Java 17，选错 JDK 会在启动时直接 UnsupportedClassVersionError -->
            <div v-if="jarInfo" class="text-xs mt-1" :class="jdkTooOld ? 'text-destructive' : 'hint'">
              <template v-if="jarInfo.spring_boot_version">
                Spring Boot {{ jarInfo.spring_boot_version }}<template v-if="jarInfo.min_jdk"> · {{ $t('jarNeedsJdk', { n: jarInfo.min_jdk }) }}</template>
              </template>
              <template v-else>{{ $t('jarNoSpringBootInfo') }}</template>
              <template v-if="jdkTooOld"> — {{ $t('jdkTooOldForJar') }}</template>
            </div>
          </div>

          <!-- Resource planning -->
          <div class="section-title">{{ $t('resourcePlanning') }}</div>
          <div class="grid-3">
            <div class="field">
              <div class="field-label">{{ $t('totalDeployServices') }}</div>
              <input class="input" type="number" v-model.number="tuning.totalServices" min="1" />
            </div>
            <div class="field">
              <div class="field-label">{{ $t('xms') }}</div>
              <input class="input" type="number" v-model.number="tuning.xmsMb" min="64" @input="jvm.xms_mb = tuning.xmsMb" />
            </div>
            <div class="field">
              <div class="field-label">{{ $t('xmx') }}</div>
              <input class="input" type="number" v-model.number="tuning.xmxMb" min="64" @input="jvm.xmx_mb = tuning.xmxMb" />
            </div>
          </div>
          <!-- 按钮移到额外 JVM 参数上方 -->

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
          <div class="field">
            <div class="field-label">{{ $t('gcType') }}</div>
            <select class="input" v-model="jvm.gc_type">
              <option value="" disabled>{{ $t('selectGcType') }}</option>
              <option value="G1GC">G1GC</option>
              <option value="ParallelGC">ParallelGC</option>
              <option value="ZGC">ZGC</option>
              <option value="ShenandoahGC">ShenandoahGC</option>
              <option value="SerialGC">SerialGC</option>
            </select>
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

          <!-- JVM extra flags -->
          <div class="field">
            <div class="field-label" style="display:flex;align-items:center;gap:8px;justify-content:space-between">
              <span>{{ $t('extraFlags') }}</span>
              <button class="btn primary" style="height:26px;font-size:11px;padding:0 8px" @click="generateOptimalParams">
                <Icon icon="mdi:auto-fix" style="font-size:13px" /> {{ $t('generateRecommendedParams') }}
              </button>
            </div>
            <textarea class="textarea input-mono" v-model="extraFlagsText" rows="4" placeholder="-XX:+HeapDumpOnOutOfMemoryError -XX:+ExitOnOutOfMemoryError -Dfile.encoding=UTF-8" />
            <label class="checkbox-label mt-2">
              <input type="checkbox" v-model="utf8Encoding" class="checkbox" />
              <span class="text-xs">{{ $t('utf8Charset') }}</span>
            </label>
          </div>

          <!-- Program args -->
          <div class="field">
            <div class="field-label">{{ $t('programArgs') }}</div>
            <textarea class="textarea input-mono" v-model="programArgsText" rows="4" placeholder="--server.port=8080" />
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

          <!-- 运行包路径（只读） -->
          <div class="field" v-if="app">
            <div class="field-label">{{ $t('runningJarPath') }}</div>
            <input class="input input-mono" :value="displayRelPath(app.jar_path)" readonly />
          </div>

          <!-- Log path（只读，编辑模式下不允许修改） -->
          <div class="field">
            <div class="field-label">{{ $t('logPath') }}</div>
            <input class="input input-mono" :value="displayRelPath(form.log_path)" readonly placeholder="logs/app.log" />
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
              <div class="field">
                <div class="field-label">{{ $t('stopTimeout') }}</div>
                <input
                  class="input"
                  type="number"
                  v-model.number="form.stop_timeout_secs"
                  min="1"
                  max="600"
                />
                <div class="text-muted text-sm">{{ $t('stopTimeoutHint') }}</div>
              </div>
              <div class="field">
                <div class="field-label">{{ $t('gracefulStopUrl') }}</div>
                <input
                  class="input"
                  type="text"
                  v-model="form.actuator_shutdown_url"
                  :placeholder="actuatorPlaceholder"
                />
                <div class="text-muted text-sm">{{ $t('gracefulStopUrlHint') }}</div>
              </div>
            </div>
          </details>
        </div>

        <div class="dialog-footer">
          <button class="btn" @click="$emit('cancel')">{{ $t('cancel') }}</button>
          <button class="btn primary" @click="save" :disabled="!valid || saving">
            {{ saving ? $t('saving') : $t('save') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
  <Teleport to="body">
    <div v-if="saveError" class="overlay" style="z-index:70" @click.self="saveError = ''">
      <div class="confirm-box">
        <div class="confirm-title"><Icon icon="mdi:alert-circle-outline" /></div>
        <p class="confirm-msg">{{ saveError }}</p>
        <div class="confirm-actions">
          <button class="btn primary" @click="saveError = ''">{{ $t('confirm') }}</button>
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
import type { SpringBootApp, JvmOptsTemplate, JarInfo } from '@/models/springboot'

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
const nameError = ref('')

/** 名称输入：禁止中文，编辑模式只读由 :readonly 控制 */
function onNameInput(e: Event) {
  const v = (e.target as HTMLInputElement).value
  if (/[\u4e00-\u9fff\u3400-\u4dbf]/.test(v)) {
    nameError.value = t('nameCannotContainChinese')
    return
  }
  nameError.value = ''
  form.name = v
}

// ponytail: 展示 JDK 和 JRE，过滤掉 MySQL/Redis 等其他软件，标注类型
const jdkList = computed(() =>
  store.jdkList.filter(j => j.key === 'jre' || j.key === 'jdk')
)

// JVM structured state
const jvm = reactive<JvmOptsTemplate>({
  xms_mb: 512,
  xmx_mb: 512,
  metaspace_mb: 128,
  gc_type: 'G1GC',
  extra_flags: ['-XX:+ExitOnOutOfMemoryError', '-XX:+HeapDumpOnOutOfMemoryError', '-Dfile.encoding=UTF-8'],
})

const extraFlagsText = ref('')
const utf8Encoding = ref(true)

/** apps.json 中存的是相对 data_dir 的路径（如 springboot/{name}/app.jar），
 *  jar_path 已解析为绝对路径，需要剥离 data_dir 前缀显示相对路径 */
function displayRelPath(p: string): string {
  if (!p) return p
  // 统一正斜杠便于匹配
  const normalized = p.replace(/\\/g, '/')
  // 找最后一个 /data/ 后的部分作为相对路径
  const idx = normalized.lastIndexOf('/data/')
  if (idx >= 0) return normalized.slice(idx + 1)
  // 已经是相对路径则直接返回（统一正斜杠）
  if (!/^[A-Za-z]:\//.test(normalized) && !normalized.startsWith('/')) return normalized
  return normalized
}

function parseJvmOpts(opts: string[]): JvmOptsTemplate {
  const result: JvmOptsTemplate = { xms_mb: 512, xmx_mb: 512, metaspace_mb: 128, gc_type: 'G1GC', extra_flags: [] }
  const knownGc = ['G1GC', 'ZGC', 'ParallelGC', 'ShenandoahGC', 'SerialGC']
  for (const opt of opts) {
    if (opt.startsWith('-Xms')) {
      result.xms_mb = parseInt(opt.slice(4).replace(/[gm]/g, '')) || 256
    } else if (opt.startsWith('-Xmx')) {
      result.xmx_mb = parseInt(opt.slice(4).replace(/[gm]/g, '')) || 1024
    } else if (opt.startsWith('-XX:MetaspaceSize=')) {
      result.metaspace_mb = parseInt(opt.slice(18).replace('m', '')) || 128
    } else if (knownGc.some(gc => opt === `-XX:+Use${gc}`)) {
      result.gc_type = opt.slice(8) // -XX:+Use 为 8 字符，slice(8) 取出 GC 名
    } else {
      result.extra_flags.push(opt)
    }
  }
  // ponytail: gc_type 为空时回退默认值
  if (!result.gc_type) result.gc_type = 'G1GC'
  return result
}

function buildJvmOpts(): string[] {
  const extraFlags = extraFlagsText.value
    ? extraFlagsText.value.split(/\s+/).filter(Boolean)
    : [...jvm.extra_flags]
  const opts = [
    `-Xms${jvm.xms_mb}m`,
    `-Xmx${jvm.xmx_mb}m`,
    `-XX:MetaspaceSize=${jvm.metaspace_mb}m`,
    `-XX:+Use${jvm.gc_type}`,
    ...extraFlags,
  ]
  if (utf8Encoding.value && !opts.includes('-Dfile.encoding=UTF-8')) {
    opts.push('-Dfile.encoding=UTF-8')
  }
  return opts
}

	// Resource planning state
const tuning = reactive({
  totalServices: 1,
  xmsMb: 512,
  xmxMb: 512,
})
const recommendedClicked = ref(false)

function computeRegionSize(xmxMb: number): number {
  // G1 默认分 ~2048 个 region，结果取 2 的幂，限制在 1~32m
  const ideal = xmxMb / 2048
  const pow2 = Math.round(Math.log2(ideal))
  return Math.max(1, Math.min(32, Math.pow(2, pow2)))
}

function generateOptimalParams() {
  const xmx = Math.max(64, tuning.xmxMb)
  tuning.xmsMb = Math.max(64, tuning.xmsMb)
  jvm.xms_mb = tuning.xmsMb
  jvm.xmx_mb = xmx

  const regionSize = computeRegionSize(xmx)
  // ParallelGCThreads: 按堆大小递增
  const gcThreads = xmx <= 1024 ? 2 : xmx <= 4096 ? 4 : 6
  const concThreads = Math.max(1, Math.round(gcThreads / 4))
  const directMem = Math.max(64, Math.round(xmx * 0.1))

  const flags = [
    '-XX:MaxGCPauseMillis=300',
    `-XX:G1HeapRegionSize=${regionSize}m`,
    `-XX:ParallelGCThreads=${gcThreads}`,
    `-XX:ConcGCThreads=${concThreads}`,
    '-XX:MetaspaceSize=128m',
    '-XX:MaxMetaspaceSize=256m',
    '-Xss256k',
    `-XX:MaxDirectMemorySize=${directMem}m`,
    '-XX:+HeapDumpOnOutOfMemoryError',
    '-XX:HeapDumpPath=logs/heapdump.hprof',
  ]
  extraFlagsText.value = flags.join('\n')
  recommendedClicked.value = true
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
  jdk_type: '',
  stop_timeout_secs: 30,
  actuator_shutdown_url: '',
})

const programArgsText = ref('')

/** 留空时的实际请求地址：占位符直接把默认值显示出来，用户不必猜 */
const actuatorPlaceholder = computed(() =>
  form.port ? `http://127.0.0.1:${form.port}/actuator/shutdown` : t('gracefulStopUrlNoPort')
)

/** 选 jar 时探测到的元信息（Spring Boot 版本 / 所需 JDK），仅用于提示，不参与提交 */
const jarInfo = ref<JarInfo | null>(null)

/** 从 JDK 版本串取主版本：`21.0.11` → 21；旧命名 `1.8.0_x` → 8。取不到返回 null。 */
function jdkMajor(version: string): number | null {
  const m = /^(\d+)(?:\.(\d+))?/.exec(version.trim())
  if (!m) return null
  const first = Number(m[1])
  // 1.x 是 JDK 8 及更早的旧命名（1.8.0 → 8）
  return first === 1 && m[2] !== undefined ? Number(m[2]) : first
}

/** 所选 JDK 是否低于该 jar 的要求（Spring Boot 3.x/4.x 需 JDK 17+） */
const jdkTooOld = computed(() => {
  const floor = jarInfo.value?.min_jdk
  if (!floor) return false
  const jdk = store.jdkList.find(j => j.id === form.jdk_installed_id)
  const major = jdk ? jdkMajor(jdk.version) : null
  return major !== null && major < floor
})

const valid = computed(() => {
  if (nameError.value) return false
  if (props.app) return form.name.trim() !== '' && form.jdk_installed_id !== ''
  return form.jar_path.trim() !== '' && form.name.trim() !== '' && form.jdk_installed_id !== '' && recommendedClicked.value
})

/** 检查名称是否已被其他应用使用 */
function isNameDuplicate(name: string, excludeId?: string): boolean {
  return store.apps.some(a => a.name === name.trim() && a.id !== excludeId)
}

onMounted(async () => {
  store.fetchJdkList()
  store.fetchDependencyCandidates()
  store.fetchGroups()

  if (props.app) {
    // ponytail: 编辑模式下 jar_path 留空（上传框干净），当前 jar 路径通过 app.jar_path 显示
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
    form.jdk_type = props.app.jdk_type
    form.stop_timeout_secs = props.app.stop_timeout_secs
    form.actuator_shutdown_url = props.app.actuator_shutdown_url || ''
    programArgsText.value = props.app.program_args.join('\n')

    const parsed = parseJvmOpts(props.app.jvm_opts)
    jvm.xms_mb = parsed.xms_mb
    jvm.xmx_mb = parsed.xmx_mb
    tuning.xmsMb = parsed.xms_mb
    tuning.xmxMb = parsed.xmx_mb
    jvm.metaspace_mb = parsed.metaspace_mb
    jvm.gc_type = parsed.gc_type || 'G1GC'
    jvm.extra_flags = parsed.extra_flags
    extraFlagsText.value = parsed.extra_flags.join(' ')
    utf8Encoding.value = props.app.jvm_opts.includes('-Dfile.encoding=UTF-8')
    recommendedClicked.value = true

    // 编辑模式下 jar 已在位：直接读它的元信息，显示框架版本与所需 JDK
    if (props.app.jar_path) {
      try {
        jarInfo.value = await store.readJarInfo(props.app.jar_path)
      } catch (e) {
        console.error('读取 JAR 信息失败:', e)
      }
    }
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
    // ponytail: 自动从 JAR 内 application.yml/properties 读取端口
    try {
      const port = await store.readJarPort(selected)
      if (port !== null && port > 0) form.port = port
    } catch (e) {
      console.error('读取 JAR 端口失败:', e)
    }
    // 识别构建该 JAR 的 Spring Boot 版本 → 提示所需 JDK
    // （3.x/4.x 需 17+，选错会在启动时直接 UnsupportedClassVersionError）
    try {
      jarInfo.value = await store.readJarInfo(selected)
    } catch (e) {
      console.error('读取 JAR 信息失败:', e)
      jarInfo.value = null
    }
  }
}

async function onJdkChange() {
  if (!form.jdk_installed_id) return
  try {
    const recommended = await store.getRecommendedOpts(form.jdk_installed_id)
    jvm.xms_mb = recommended.xms_mb
    jvm.xmx_mb = recommended.xmx_mb
    tuning.xmsMb = recommended.xms_mb
    tuning.xmxMb = recommended.xmx_mb
    jvm.metaspace_mb = recommended.metaspace_mb
    jvm.gc_type = recommended.gc_type
    extraFlagsText.value = recommended.extra_flags.join(' ')
    const selected = store.jdkList.find(j => j.id === form.jdk_installed_id)
    form.jdk_type = selected?.key === 'jdk' ? 'jdk' : 'jre'
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
  // 名称重复校验
  const dupId = props.app ? props.app.id : undefined
  if (isNameDuplicate(form.name, dupId)) {
    saveError.value = t('appNameAlreadyExists', { name: form.name.trim() })
    saving.value = false
    return
  }
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
        jdk_type: form.jdk_type,
        group: form.group,
        stop_timeout_secs: form.stop_timeout_secs,
        actuator_shutdown_url: form.actuator_shutdown_url,
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
        jdk_type: form.jdk_type,
        group: form.group,
        stop_timeout_secs: form.stop_timeout_secs,
        actuator_shutdown_url: form.actuator_shutdown_url,
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
  white-space: pre-wrap;
  overflow-wrap: break-word;
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
.mt-1 { margin-top: 4px; }
.text-xs { font-size: 12px; }
.text-destructive { color: #b91c1c; }
.border-destructive { border-color: #b91c1c !important; }
.hint { font-size: 11px; color: var(--color-muted-foreground); }
</style>
