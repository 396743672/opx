# 首页瘦身 + 监控中心 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 首页回归纯整机监控，进程资源监控搬进新菜单「监控中心」。

**Architecture:** 抽出 `useRunningSoftware()` composable 供两页共用运行中软件/应用数据（含事件监听与生命周期清理）；监控中心页承载进程列表与 KPI；首页末尾加一个默认收起的「运行中服务与应用」折叠块。后端命令与事件零改动。

**Tech Stack:** Vue 3 Composition API + TypeScript + vue-i18n + Tailwind 工具类（项目现有 token）。

## Global Constraints

- 工作分支 `feat/dashboard-slim`（已存在）。
- **不改后端**：`sample_process_resources` / `process_metrics_history` / `system_history` / `list_installed_software` / `springboot-status-changed` 全部原样复用。
- 设计 token 与组件沿用项目现有：`StatCard` / `CardHeader` / `TrendChart` / `ProgressBar` / `--color-destructive` / `--color-success` / `--color-chart-N`。**不引入新配色、新字体、新依赖**。
- i18n 键必须同时加进 `src/locales/zh-CN.ts` 与 `src/locales/en-US.ts`，同键同义。
- 每个任务结束必须跑 `npx vue-tsc --noEmit`（仓库根目录）且零错误。
- 提交信息用中文 conventional commits，不加 `Co-Authored-By`。
- 不运行项目级格式化工具（eslint/prettier 全仓会污染 diff）。
- 项目无前端测试框架（无 vitest/jest），**测试门禁 = `vue-tsc` + 计划末尾的实机清单**。

---

### Task 1: 共用基础设施（composable + i18n 键）

**Files:**
- Create: `src/composables/useRunningSoftware.ts`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`

**Interfaces:**
- Consumes: `list_installed_software` 命令、`springboot-status-changed` 事件、`useSpringBootStore`、`useLifecycleStore`
- Produces（Task 2/3 依赖）:
  ```ts
  export function useRunningSoftware(): {
    installedSoftware: Ref<InstalledSoftware[]>
    runningSoftware: ComputedRef<InstalledSoftware[]>
    runningApps: Ref<SpringBootApp[]>
  }
  ```
  i18n 键：`monitorCenter`、`runningServicesAndApps`、`searchProcessPlaceholder`、`noMatchingProcess`、`kpiRunningProcesses`、`kpiCpuAlerting`、`kpiMemAlerting`、`noRunningServicesOrApps`

- [ ] **Step 1: 建 composable**

创建 `src/composables/useRunningSoftware.ts`：

```ts
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSpringBootStore } from '@/modules/springboot-manager/stores/springboot'
import { useLifecycleStore } from '@/modules/software-manager/stores/lifecycle'
import type { InstalledSoftware } from '@/models/software'
import { SoftwareStatus } from '@/models/software'
import type { SpringBootApp } from '@/models/springboot'
import { AppStatus } from '@/models/springboot'

/// 运行中的软件与应用：系统监控页与监控中心页共用。
/// 内部自管取数与事件监听的生命周期（onMounted 注册、onUnmounted 清理）。
/// ponytail: installedSoftware 只在挂载时拉一次（与首页原行为一致），
/// 启停变化由 lifecycleStore.getStatus 覆盖状态触发 runningSoftware 重算。
export function useRunningSoftware() {
  const sbStore = useSpringBootStore()
  const lifecycleStore = useLifecycleStore()

  const installedSoftware = ref<InstalledSoftware[]>([])
  const runningApps = ref<SpringBootApp[]>([])
  let unlistenSb: UnlistenFn | null = null

  const runningSoftware = computed(() =>
    installedSoftware.value.filter((s) => {
      const st = lifecycleStore.getStatus(s.id)
      const status = st !== SoftwareStatus.Unknown ? st : s.status
      return status === SoftwareStatus.Running
    })
  )

  async function refreshApps() {
    await sbStore.fetchApps()
    runningApps.value = sbStore.apps.filter((a) => a.status === AppStatus.Running)
  }

  onMounted(async () => {
    installedSoftware.value = await invoke<InstalledSoftware[]>('list_installed_software')
    await refreshApps()
    unlistenSb = await listen('springboot-status-changed', refreshApps)
  })

  onUnmounted(() => {
    unlistenSb?.()
  })

  return { installedSoftware, runningSoftware, runningApps }
}
```

- [ ] **Step 2: 加 i18n 键**

`src/locales/zh-CN.ts` 中 `systemMonitor` 键所在区块（监控相关文案，与 `alertThresholds` 相邻处）追加：

```typescript
  monitorCenter: '监控中心',
  runningServicesAndApps: '运行中服务与应用',
  noRunningServicesOrApps: '暂无运行中的服务或应用',
  searchProcessPlaceholder: '搜索名称或 PID',
  noMatchingProcess: '无匹配进程',
  kpiRunningProcesses: '运行中进程',
  kpiCpuAlerting: 'CPU 告警中',
  kpiMemAlerting: '内存告警中',
```

`src/locales/en-US.ts` **同一位置**追加：

```typescript
  monitorCenter: 'Monitor Center',
  runningServicesAndApps: 'Running services & apps',
  noRunningServicesOrApps: 'No running services or apps',
  searchProcessPlaceholder: 'Search name or PID',
  noMatchingProcess: 'No matching process',
  kpiRunningProcesses: 'Running processes',
  kpiCpuAlerting: 'CPU alerting',
  kpiMemAlerting: 'Memory alerting',
```

- [ ] **Step 3: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 零输出、退出码 0（composable 尚无调用方，仅验证语法与类型）。

- [ ] **Step 4: 提交**

```bash
git add src/composables/useRunningSoftware.ts src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(monitor): 抽出运行中软件与应用 composable，补监控中心 i18n 键"
```

---

### Task 2: 监控中心页 + 路由 + 侧边栏

**Files:**
- Create: `src/modules/monitor/pages/MonitorCenterPage.vue`
- Modify: `src/router/index.ts`、`src/layouts/Sidebar.vue`

**Interfaces:**
- Consumes: Task 1 的 `useRunningSoftware()` 与 8 个 i18n 键；`useSystemStore`（`memTotal`、`sampleProcesses`）、`useSettingsStore`（告警阈值）、`TrendChart` / `StatCard` / `CardHeader` / `PageHeader` 组件
- Produces: 路由 `/monitor`（name `monitor`）

- [ ] **Step 1: 建页面**

创建 `src/modules/monitor/pages/MonitorCenterPage.vue`：

```vue
<template>
  <div class="animate-fade-in">
    <PageHeader icon="mdi:chart-timeline-variant" :title="$t('monitorCenter')">
      <template #actions>
        <input
          v-model="searchKeyword"
          class="h-8 px-2 w-48 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          :placeholder="$t('searchProcessPlaceholder')"
        />
        <button class="btn" @click="loadMetricsHistory">
          <Icon icon="mdi:refresh" /> {{ $t('refresh') }}
        </button>
      </template>
    </PageHeader>

    <!-- KPI -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">
      <StatCard
        :label="$t('kpiRunningProcesses')"
        :value="processRows.length"
        icon="mdi:server"
        accent="primary"
      />
      <StatCard
        :label="$t('kpiCpuAlerting')"
        :value="alertingCpu"
        icon="mdi:alert-circle-outline"
        :accent="alertingCpu > 0 ? 'destructive' : 'chart-2'"
      />
      <StatCard
        :label="$t('kpiMemAlerting')"
        :value="alertingMem"
        icon="mdi:alert-circle-outline"
        :accent="alertingMem > 0 ? 'destructive' : 'chart-2'"
      />
    </div>

    <!-- 进程列表 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card">
      <CardHeader icon="mdi:chart-timeline-variant" :title="$t('processMonitor')" hide-refresh />
      <!-- 告警机制提示：说明采样/阈值/触发条件，避免「改了阈值却没记录」的困惑 -->
      <div class="text-xs text-muted-foreground mb-2" style="overflow-wrap: anywhere">
        {{ $t('alertHintLine', alertHintParams) }}
        <span v-if="processRows.length === 0"> · {{ $t('alertNoRunning') }}</span>
      </div>
      <div v-if="processRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noRunningProcess') }}
      </div>
      <div v-else-if="filteredRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noMatchingProcess') }}
      </div>
      <div v-else class="space-y-1">
        <div v-for="row in filteredRows" :key="row.pid" class="border border-border rounded-md">
          <div class="flex items-center justify-between px-3 py-2 text-sm">
            <span class="flex items-center gap-2 min-w-0">
              <Icon
                :icon="row.type === 'springboot' ? 'mdi:leaf' : 'mdi:server'"
                :class="row.type === 'springboot' ? 'text-green-500' : 'text-info'"
              />
              <span class="truncate">{{ row.name }}</span>
              <span class="text-xs text-muted-foreground tnum">({{ row.pid }})</span>
            </span>
            <span class="flex items-center gap-4 shrink-0">
              <span
                class="text-xs tnum flex items-center gap-1"
                :class="row.cpu >= thresholds.cpu ? 'text-destructive' : ''"
              >
                <Icon v-if="row.cpu >= thresholds.cpu" icon="mdi:alert" />
                CPU {{ row.cpu.toFixed(1) }}%
              </span>
              <span
                class="text-xs tnum flex items-center gap-1"
                :class="row.memPct >= thresholds.mem ? 'text-destructive' : ''"
              >
                <Icon v-if="row.memPct >= thresholds.mem" icon="mdi:alert" />
                {{ $t('memory') }} {{ formatBytes(row.memBytes) }}
              </span>
              <button class="btn btn-sm" @click="toggleProcess(row.pid)">
                {{ expandedPids.has(row.pid) ? $t('collapse') : $t('view') }}
              </button>
            </span>
          </div>
          <div
            v-if="expandedPids.has(row.pid)"
            class="border-t border-border p-3 grid grid-cols-1 lg:grid-cols-2 gap-4"
          >
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('cpuUsage') }}</div>
              <TrendChart
                metric="cpu"
                :points="procHistory[String(row.pid)] ?? []"
                color-var="--color-chart-1"
                :height="120"
              />
            </div>
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('memoryUsage') }}（占整机 %）</div>
              <TrendChart
                metric="memory"
                :points="procHistory[String(row.pid)] ?? []"
                color-var="--color-chart-2"
                :height="120"
                :max="100"
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '@/stores/settings'
import { useSystemStore } from '@/stores/system'
import { useRunningSoftware } from '@/composables/useRunningSoftware'
import type { ProcessSample } from '@/models/process'
import type { HistoryPoint } from '@/models/system'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import { Icon } from '@iconify/vue'
import { formatBytes } from '@/utils/format'

const systemStore = useSystemStore()
const settingsStore = useSettingsStore()
const { runningSoftware, runningApps } = useRunningSoftware()

/** 最近一次采样结果（pid -> sample），用于表格实时值回填 */
const latestSamples = ref<Map<number, ProcessSample>>(new Map())
const expandedPids = ref<Set<number>>(new Set())
const searchKeyword = ref('')

interface ProcRow {
  name: string
  type: 'software' | 'springboot'
  pid: number
  cpu: number
  memBytes: number
  memPct: number
}

const processRows = computed<ProcRow[]>(() => {
  const rows: ProcRow[] = []
  for (const s of runningSoftware.value) {
    if (s.pid == null) continue
    const smp = latestSamples.value.get(s.pid)
    const memPct = systemStore.memTotal > 0 ? ((smp?.mem_bytes ?? 0) / systemStore.memTotal) * 100 : 0
    rows.push({ name: s.name, type: 'software', pid: s.pid, cpu: smp?.cpu_usage ?? 0, memBytes: smp?.mem_bytes ?? 0, memPct })
  }
  for (const a of runningApps.value) {
    if (a.pid == null) continue
    const smp = latestSamples.value.get(a.pid)
    const memPct = systemStore.memTotal > 0 ? ((smp?.mem_bytes ?? 0) / systemStore.memTotal) * 100 : 0
    rows.push({ name: a.name, type: 'springboot', pid: a.pid, cpu: smp?.cpu_usage ?? 0, memBytes: smp?.mem_bytes ?? 0, memPct })
  }
  return rows
})

/** 搜索只过滤列表展示，不影响 KPI 计数 */
const filteredRows = computed(() => {
  const k = searchKeyword.value.trim().toLowerCase()
  if (!k) return processRows.value
  return processRows.value.filter(
    (r) => r.name.toLowerCase().includes(k) || String(r.pid).includes(k)
  )
})

const thresholds = computed(() => ({
  cpu: settingsStore.settings?.alert_process_cpu ?? 90,
  mem: settingsStore.settings?.alert_process_mem ?? 90,
}))

// 告警机制提示：阈值取自设置（后端每 30s 采样时读取）
const alertHintParams = computed(() => {
  const s = settingsStore.settings
  return {
    sc: s?.alert_system_cpu ?? 90,
    sm: s?.alert_system_mem ?? 90,
    pc: s?.alert_process_cpu ?? 90,
    pm: s?.alert_process_mem ?? 90,
  }
})

const alertingCpu = computed(() => processRows.value.filter((r) => r.cpu >= thresholds.value.cpu).length)
const alertingMem = computed(() => processRows.value.filter((r) => r.memPct >= thresholds.value.mem).length)

function toggleProcess(pid: number) {
  const next = new Set(expandedPids.value)
  next.has(pid) ? next.delete(pid) : next.add(pid)
  expandedPids.value = next
}

// 趋势曲线读后端持久化序列（30s 粒度），前端不做累积
const procHistory = ref<Record<string, HistoryPoint[]>>({})

async function loadMetricsHistory() {
  try {
    procHistory.value = await invoke<Record<string, HistoryPoint[]>>('process_metrics_history')
  } catch {
    // 首次尚无历史文件：保持空对象即可
  }
}

let procTimer: number | null = null
let metricsTimer: number | null = null

onMounted(() => {
  // 进程采样：与整机轮询同频（1s），只刷新实时数值（告警由后端常驻循环负责）
  procTimer = window.setInterval(async () => {
    const pids = processRows.value.map((r) => r.pid).filter((p) => p != null)
    if (pids.length === 0) return
    const samples = await systemStore.sampleProcesses(pids)
    for (const s of samples) {
      latestSamples.value.set(s.pid, s)
    }
  }, 1000)
  loadMetricsHistory()
  metricsTimer = window.setInterval(loadMetricsHistory, 30_000)
})

onUnmounted(() => {
  if (procTimer) clearInterval(procTimer)
  if (metricsTimer) clearInterval(metricsTimer)
})
</script>
```

**i18n 键已核实**：`memory`、`runningServices`、`runningApps`、`processMonitor`、`alertHintLine`、`alertNoRunning`、`noRunningProcess`、`view`、`collapse`、`cpuUsage`、`memoryUsage`、`refresh` 均已存在，直接复用；本计划只新增 Task 1 列出的 8 个键。`（占整机 %）` 为原实现的硬编码中文，按原样搬运，不新增键也不改文案。

- [ ] **Step 2: 注册路由**

`src/router/index.ts` 在 `/dashboard` 条目之后加：

```typescript
  {
    path: '/monitor',
    name: 'monitor',
    component: () => import('@/modules/monitor/pages/MonitorCenterPage.vue'),
    meta: { title: 'monitorCenter' },
  },
```

- [ ] **Step 3: 加侧边栏项**

`src/layouts/Sidebar.vue` 的 `groups` 中 `monitor` 组改为两项：

```typescript
  {
    label: 'monitor',
    items: [
      { path: '/dashboard', titleKey: 'systemMonitor', icon: 'mdi:gauge' },
      { path: '/monitor', titleKey: 'monitorCenter', icon: 'mdi:chart-timeline-variant' },
    ],
  },
```

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: vue-tsc 零错误；build 成功（末尾显示 `✓ built in ...`）。

- [ ] **Step 5: 提交**

```bash
git add src/modules/monitor/pages/MonitorCenterPage.vue src/router/index.ts src/layouts/Sidebar.vue
git commit -m "feat(monitor): 新增监控中心页（进程资源监控 + KPI + 搜索）"
```

---

### Task 3: 首页瘦身

**Files:**
- Modify: `src/modules/system-monitor/pages/DashboardPage.vue`

**Interfaces:**
- Consumes: Task 1 的 `useRunningSoftware()`、键 `runningServicesAndApps` / `noRunningServicesOrApps`
- Produces: 无（终端页面）

- [ ] **Step 1: 删掉三个区块的模板**

从 `DashboardPage.vue` 的 `<template>` 中删除以下整段（连同注释）：

1. `<!-- 服务组概览 -->` 开头的整个 `<div>`（含 `stackOverview` 循环）
2. `<!-- 最近一次启动报告 -->` 开头的整个 `<div>`
3. `<!-- ⚡ 服务与应用概览（示例） -->` 开头的三列 `<div>`（已装软件 / 运行中服务 / 运行中应用）

保留：PageHeader、骨架屏、4 张 StatCard、趋势图 ×2、**进程资源监控区块也要删**（`<!-- 进程资源监控 -->` 开头的整个 `<div>`，它已搬到监控中心）、系统信息/磁盘/网络三列。

- [ ] **Step 2: 换成折叠块**

在「系统信息 / 磁盘 / 网络」三列之后、根 `</div>` 之前插入：

```html
    <!-- 运行中服务与应用：默认收起，避免首页被软件列表占满 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card">
      <button
        class="w-full flex items-center justify-between cursor-pointer text-left"
        :aria-expanded="showRunningList"
        @click="showRunningList = !showRunningList"
      >
        <span class="flex items-center gap-2 text-sm font-semibold tracking-tight">
          <Icon icon="mdi:server-network" class="text-muted-foreground" />
          {{ $t('runningServicesAndApps') }} ({{ runningTotal }})
        </span>
        <Icon
          :icon="showRunningList ? 'mdi:chevron-up' : 'mdi:chevron-down'"
          class="text-muted-foreground"
        />
      </button>
      <div v-if="showRunningList" class="mt-4">
        <div
          v-if="runningTotal === 0"
          class="py-3 text-sm text-muted-foreground"
        >
          {{ $t('noRunningServicesOrApps') }}
        </div>
        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <div class="text-xs text-muted-foreground mb-2">{{ $t('runningServices') }}</div>
            <div class="space-y-1">
              <div
                v-for="s in runningSoftware"
                :key="s.id"
                class="flex items-center justify-between text-sm border border-border rounded-md px-3 py-2"
              >
                <span class="flex items-center gap-2 min-w-0">
                  <span class="w-1.5 h-1.5 rounded-full bg-success shrink-0"></span>
                  <span class="truncate">{{ s.name }}</span>
                </span>
                <span class="text-xs text-muted-foreground shrink-0">{{ s.version }}</span>
              </div>
            </div>
          </div>
          <div>
            <div class="text-xs text-muted-foreground mb-2">{{ $t('runningApps') }}</div>
            <div class="space-y-1">
              <div
                v-for="app in runningApps"
                :key="app.id"
                class="flex items-center justify-between text-sm border border-border rounded-md px-3 py-2"
              >
                <span class="flex items-center gap-2 min-w-0">
                  <span class="w-1.5 h-1.5 rounded-full bg-info shrink-0"></span>
                  <span class="truncate">{{ app.name }}</span>
                </span>
                <span class="text-xs text-muted-foreground tnum shrink-0">:{{ app.port }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
```

- [ ] **Step 3: 清理 script**

`<script setup>` 改为（保留页面仍在用的部分）：

```typescript
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useSystemStore } from '@/stores/system'
import { useSettingsStore } from '@/stores/settings'
import { useRunningSoftware } from '@/composables/useRunningSoftware'
import type { HistoryPoint } from '@/models/system'
import PageHeader from '@/components/PageHeader.vue'
import StatCard from '@/components/StatCard.vue'
import CardHeader from '@/components/CardHeader.vue'
import TrendChart from '@/components/TrendChart.vue'
import ProgressBar from '@/components/ProgressBar.vue'
import { Icon } from '@iconify/vue'
import { formatBytes, formatRate, formatUptime, formatBootTime } from '@/utils/format'

const systemStore = useSystemStore()
const settingsStore = useSettingsStore()
const { runningSoftware, runningApps } = useRunningSoftware()

const showRunningList = ref(false)
const runningTotal = computed(() => runningSoftware.value.length + runningApps.value.length)

// 趋势曲线读后端持久化序列（30s 粒度），前端不累积历史
const sysHistory = ref<HistoryPoint[]>([])

async function loadMetricsHistory() {
  try {
    sysHistory.value = await invoke<HistoryPoint[]>('system_history')
  } catch {
    // 首次尚无历史文件：保持空数组即可
  }
}
```

**需删除**（随区块一并移除，且不再被引用）：`invoke`/`listen`/`UnlistenFn` 之外的旧导入（`useSpringBootStore`、`useLifecycleStore`、`useStackStore`、`SpringBootApp`/`AppStatus`/`InstalledSoftware`/`SoftwareStatus`/`ProcessSample`/`StartupReport`/`StartupItemReport` 类型）、`installedSoftware`、`runningApps` 本地声明、`runningSoftware` computed、`startupReport`/`failedCount`/`KIND_ICON`/`kindIcon`/`formatStartTime`/`upsertStartupItem`、`expandedPids`/`latestSamples`/`ProcRow`/`processRows`/`toggleProcess`、`procHistory`、`stackOverview`、`unlistenSb`/`unlistenStartupProgress`/`unlistenStartupDone`、`procTimer`/`metricsTimer` 及其在 `onMounted`/`onUnmounted` 中的相应语句。

`onMounted` 只保留：`nowTick` 秒级 tick（运行时长用）+ `loadMetricsHistory()` + `metricsTimer = window.setInterval(loadMetricsHistory, 30_000)`。
`onUnmounted` 只保留：`tickTimer` 与 `metricsTimer` 的清理。

（`loadMetricsHistory` 里的 `invoke` 仍需 `import { invoke } from '@tauri-apps/api/core'`。）

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: vue-tsc 零错误（无 unused import 报错——`vue-tsc` 对未使用的导入不报错，但请人工核对 Step 3 的删除清单）；build 成功。

- [ ] **Step 5: 提交**

```bash
git add src/modules/system-monitor/pages/DashboardPage.vue
git commit -m "refactor(monitor): 首页回归纯系统监控，运行中服务与应用改为折叠块"
```

---

### Task 4: 全量验证与实机清单

**Files:** 无改动（仅验证；发现问题回到对应 Task 修）

- [ ] **Step 1: 门禁命令**

Run: `npx vue-tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: 零错误 + build 成功。

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -2`
Expected: `167 passed`（本次纯前端改动，后端基线不变）。

- [ ] **Step 2: 启动实机**

Run: `npm run tauri dev`

- [ ] **Step 3: 实机验证清单**

逐项确认并记录实际观察：

1. 首页只剩：指标卡 ×4 → 趋势图 ×2 → 系统信息/磁盘/网络 → 「运行中服务与应用 (N)」，**服务组概览/启动报告/已装软件列表/进程监控均不再出现**
2. 折叠块默认收起；点击展开显示两个列表，数量与标题一致；再点收起
3. 启动/停止一个软件 → 折叠块数量与列表内容实时变化（事件监听仍生效）
4. 侧边栏「监控」组显示两项；点「监控中心」进入新页且高亮正确
5. 监控中心 3 张 KPI 数字与实际运行进程一致；停掉一个软件后「运行中进程」下降
6. 设置页把 `alert_process_cpu` 压到 1 → 监控中心列表出现红字 + `mdi:alert` 图标，「CPU 告警中」计数上升
7. 搜索框：输入名称片段命中；输入 pid 片段命中；输入乱码显示「无匹配进程」；清空恢复；**KPI 数字不随搜索变化**
8. 进程行「查看」展开 CPU/内存双曲线正常；「收起」恢复
9. 从监控中心切到其他页再切回 → 无报错，数值正常刷新（定时器不叠加）
10. 中英文切换：两页所有新增文案齐全、无 key 裸奔

- [ ] **Step 4: 收尾提交（若第 3 步有修复）**

```bash
git add -A
git commit -m "fix(monitor): 实机验证发现的问题修复"
```

---

## 附：明确不做

- 不改任何后端命令/事件/模型。
- 不删后端启动报告与栈相关命令（`get_last_startup_report` 等仍被其他入口使用）。
- 不做监控中心的其他监控类型、不做列表排序切换、不做首页自定义布局。
