# R5 进程级资源监控 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 系统监控页新增「进程资源监控」：列出所有运行中进程（已装软件 + SpringBoot 应用）的实时 CPU%/内存，点某进程行内展开该进程的 CPU/内存趋势图；CPU/内存超固定阈值（90%）时弹 toast 告警。

**Architecture:** 后端新增 `process_monitor.rs` 采样模块（静态缓存 `System` 跨调用算 CPU%），暴露命令 `sample_process_resources(pids)`，前端传 pid 数组、后端返回 CPU%/内存字节。前端复用系统监控页既有 1s 轮询节奏，在 `system store` 维护每 pid 的 120 点环形历史，DashboardPage 渲染进程资源区块（表格 + 行内展开 TrendChart + 阈值 toast 告警）。

**Tech Stack:** Rust (sysinfo 0.31 + Tauri command)、Vue 3 `<script setup>` + Pinia store + vue-i18n、`TrendChart` 组件、`toast` 组合式函数。

## Global Constraints

- 分支：开发在 `feat/roadmap2-r5`（当前分支）上进行，禁止直接改 dev/master。
- 提交规范：不伪造 Co-Authored-By trailers；用本机 git 用户名邮箱。
- 内存趋势图归一化为「占整机内存百分比」（与整机 memory_usage 同量纲，TrendChart y 轴 0-100% 自洽）；表格中内存仍显示原始字节（formatBytes）。
- 告警为前端判定，固定阈值 CPU ≥ 90% 或内存 ≥ 90%，用全局 toast（`@/composables/useToast`），同一 pid+指标连续超限只提示一次。
- 复用 sysinfo 0.31 项目现有 API：`refresh_processes(ProcessesToUpdate::All)`（单参数，项目 0.31.0 签名）、`process(Pid::from_u32(pid))`、`process.cpu_usage() -> f32`、`process.memory() -> u64`。不引入新依赖。
- 只读采样，不启停/改进程。
- 新增 i18n key 中/英双语。
- 验证命令：`cargo test --lib`（Rust）+ `npx vue-tsc --noEmit`（前端类型）。

---

### Task 1: 后端 —— `process_monitor` 采样模块 + 单测

**Files:**
- Create: `src-tauri/src/services/software_manager/process_monitor.rs`
- Modify: `src-tauri/src/services/software_manager/mod.rs`（`pub mod process_monitor;`）

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  pub struct ProcessSample { pub pid: u32, pub cpu_usage: f64, pub mem_bytes: u64 }
  pub fn sample_processes(pids: &[u32]) -> Vec<ProcessSample>
  ```

- [ ] **Step 1: 写失败测试**

创建文件 `.superpowers/` 不涉及；直接按 TDD——先加测试（临时放文件底部 `#[cfg(test)]`），再实现。但为清晰，本步先建立文件骨架 + 测试，运行确认失败。

在 `src-tauri/src/services/software_manager/process_monitor.rs` 写入：

```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;
use sysinfo::{Pid, Process, ProcessesToUpdate, System};

/// 单进程采样结果：OS 级 CPU%（0-100，f64）与物理内存字节数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub cpu_usage: f64,
    pub mem_bytes: u64,
}

/// 跨调用缓存的 System：进程 CPU% 依赖两次 refresh 的时间差，
/// 保持同一实例才能算出准确的增量 CPU%。
static PROCESS_SYS: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new();
    s.refresh_processes(ProcessesToUpdate::All);
    Mutex::new(s)
});

/// 对给定 pid 列表逐进程采样。进程已退出/不存在则跳过（不返回该 pid）。
pub fn sample_processes(pids: &[u32]) -> Vec<ProcessSample> {
    let mut system = PROCESS_SYS.lock().unwrap();
    system.refresh_processes(ProcessesToUpdate::All);
    pids.iter()
        .filter_map(|&pid| {
            let p = system.process(Pid::from_u32(pid))?;
            Some(ProcessSample {
                pid,
                cpu_usage: p.cpu_usage() as f64,
                mem_bytes: p.memory(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_missing_pids_returns_empty() {
        // 不存在的 pid 直接返回空集，不崩溃
        let out = sample_processes(&[999_999_99u32]);
        assert!(out.is_empty());
    }

    #[test]
    fn sample_current_process_returns_non_negative() {
        // 采样当前进程自身：能取到数据且不为负
        let my_pid = std::process::id();
        let out = sample_processes(&[my_pid]);
        assert!(!out.is_empty());
        let me = &out[0];
        assert_eq!(me.pid, my_pid);
        assert!(me.cpu_usage >= 0.0);
        assert!(me.mem_bytes > 0);
    }
}
```

- [ ] **Step 2: 导出模块到 `mod.rs`**

在 `src-tauri/src/services/software_manager/mod.rs` 的 `pub mod process_monitor;` 行（按字母序放在 `providers` 之前）：

```rust
pub mod process_monitor;
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test --lib process_monitor`（从 `src-tauri/`）
Expected: 两个新测试通过（`sample_missing_pids_returns_empty`、`sample_current_process_returns_non_negative`）。

> 若编译报 `refresh_processes` 参数数量不匹配：项目锁定 0.31.0，签名可能为双参数 `refresh_processes(ProcessesToUpdate, bool)`——若如此改为 `system.refresh_processes(ProcessesToUpdate::All, true)`（第二个 bool 是否 remove 已死进程）。以实际编译为准，二者择一即可。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/services/software_manager/process_monitor.rs src-tauri/src/services/software_manager/mod.rs
git commit -m "feat(monitor): 新增 process_monitor 单进程采样模块并添加单测"
```

---

### Task 2: 后端 —— `sample_process_resources` 命令 + 注册

**Files:**
- Modify: `src-tauri/src/commands/software.rs`（新增命令）
- Modify: `src-tauri/src/lib.rs`（注册命令）

**Interfaces:**
- Consumes: `crate::services::software_manager::process_monitor::sample_processes`（Task 1）、`ProcessSample`。
- Produces: `#[tauri::command] pub fn sample_process_resources(pids: Vec<u32>) -> Result<Vec<ProcessSample>, String>`。

- [ ] **Step 1: 新增命令**

在 `src-tauri/src/commands/software.rs` 末尾（`restore_config_backup` 之后）新增：

```rust
#[tauri::command]
pub fn sample_process_resources(pids: Vec<u32>) -> Result<Vec<crate::services::software_manager::process_monitor::ProcessSample>, String> {
    Ok(crate::services::software_manager::process_monitor::sample_processes(&pids))
}
```

> 命令用 `pub fn`（非 async）即可——纯同步采样，与 `system_info`/`system_history` 一致（它们用 `pub fn`）。无需 State。

- [ ] **Step 2: 注册到 `lib.rs`**

在 `src-tauri/src/lib.rs` 的 invoke_handler 里 `commands::software::get_backup_schedule,`（R1 命令附近）之后加入：

```rust
commands::software::sample_process_resources,
```

- [ ] **Step 3: 编译验证**

Run: `cargo build`（从 `src-tauri/`）
Expected: 编译通过。

- [ ] **Step 4: 运行全部测试**

Run: `cargo test --lib`
Expected: Task 1 测试 + 既有测试全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/software.rs src-tauri/src/lib.rs
git commit -m "feat(monitor): 新增 sample_process_resources 命令"
```

---

### Task 3: 前端 —— 进程采样 + 进程资源监控区块 + 告警 + i18n

**Files:**
- Create: `src/models/process.ts`
- Modify: `src/stores/system.ts`（进程采样与历史）
- Modify: `src/modules/system-monitor/pages/DashboardPage.vue`（进程资源区块 + 展开 + 告警）
- Modify: `src/locales/zh-CN.ts` / `src/locales/en-US.ts`（i18n）

**Interfaces:**
- Consumes: `invoke('sample_process_resources', { pids })`、`TrendChart`、`toast`、已有 `installedSoftware`/`runningApps` 列表、`systemStore` 的整机内存总量。
- Produces: `ProcessSample { pid, cpu_usage, mem_bytes }`；`system store` 新增 `processSamples: Record<pid, HistoryPoint[]>` 与采样方法；DashboardPage 渲染区块。

- [ ] **Step 1: 新建 `src/models/process.ts`**

```ts
export interface ProcessSample {
  pid: number
  cpu_usage: number
  mem_bytes: number
}
```

- [ ] **Step 2: `system store` 增加进程采样**

在 `src/stores/system.ts` 顶部 import `invoke`（已有），新增进程相关状态与方法。修改后的相关片段：

```ts
import type { SystemInfo, HistoryPoint } from '@/models/system'
import type { ProcessSample } from '@/models/process'

// …（POLL_INTERVAL、MAX_HISTORY 沿用）

  /** pid -> 该进程的历史采样点（环形 MAX_HISTORY） */
  const processSamples = ref<Record<number, HistoryPoint[]>>({})
  /** 最近一次整机内存总量，用于内存趋势归一化 */
  const memTotal = ref(0)

  // 进程采样：一次 invoke 批量采样所有 pid，写入各 pid 历史
  async function sampleProcesses(pids: number[]) {
    if (pids.length === 0) return
    try {
      const samples = await invoke<ProcessSample[]>('sample_process_resources', { pids })
      for (const s of samples) {
        const point: HistoryPoint = {
          timestamp: Date.now(),
          cpu_usage: s.cpu_usage,
          memory_usage: memTotal.value > 0 ? (s.mem_bytes / memTotal.value) * 100 : 0,
        }
        const list = processSamples.value[s.pid] ?? []
        list.push(point)
        if (list.length > MAX_HISTORY) list.splice(0, list.length - MAX_HISTORY)
        processSamples.value[s.pid] = list
      }
    } catch (e) {
      console.error('process sample failed:', e)
    }
  }
```

并在 `fetchAll` 内（`history.value.push` 之后、维护整机内存）更新 `memTotal`：

```ts
      if (info.memory_total) memTotal.value = info.memory_total
```

`fetchAll` 末尾追加整机内存总量记录（放在 `systemInfo.value = info` 之后即可）。

返回对象追加：

```ts
  return {
    // …既有
    processSamples, memTotal, sampleProcesses,
  }
```

- [ ] **Step 3: DashboardPage 新增「进程资源监控」区块**

1. Template：在「服务与应用概览」`div`（`.grid.grid-cols-1.lg:grid-cols-3.mb-4`，含三个概览卡片结构以 `<!-- ⚡ 服务与应用概览（示例） -->` 注释块结束于 line 116-117）之后、信息行（注释 `<!-- 信息行：系统信息 / 磁盘 / 网络 -->`）之前，插入新卡片。表格列出运行中进程，点「查看」行内展开趋势图：

```vue
    <!-- 进程资源监控 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader icon="mdi:chart-timeline-variant" :title="$t('processMonitor')" />
      <div v-if="processRows.length === 0" class="py-3 text-sm text-muted-foreground">
        {{ $t('noRunningProcess') }}
      </div>
      <div v-else class="space-y-1">
        <div v-for="row in processRows" :key="row.pid" class="border border-border rounded-md">
          <!-- 表格行 -->
          <div class="flex items-center justify-between px-3 py-2 text-sm">
            <span class="flex items-center gap-2 min-w-0">
              <Icon :icon="row.type === 'springboot' ? 'mdi:leaf' : 'mdi:server'" :class="row.type === 'springboot' ? 'text-green-500' : 'text-info'" />
              <span class="truncate">{{ row.name }}</span>
              <span class="text-xs text-muted-foreground tnum">({{ row.pid }})</span>
            </span>
            <span class="flex items-center gap-4 shrink-0">
              <span class="text-xs tnum" :class="row.cpu >= 90 ? 'text-destructive' : ''">
                CPU {{ row.cpu.toFixed(1) }}%
              </span>
              <span class="text-xs tnum" :class="row.memPct >= 90 ? 'text-destructive' : ''">
                内存 {{ formatBytes(row.memBytes) }}
              </span>
              <button class="btn btn-sm" @click="toggleProcess(row.pid)">
                {{ expandedPids.has(row.pid) ? $t('collapse') : $t('view') }}
              </button>
            </span>
          </div>
          <!-- 行内展开趋势图 -->
          <div v-if="expandedPids.has(row.pid)" class="border-t border-border p-3 grid grid-cols-1 lg:grid-cols-2 gap-4">
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('cpuUsage') }}</div>
              <TrendChart metric="cpu" :points="processPoints(row.pid)" color-var="--color-chart-1" height="120" />
            </div>
            <div>
              <div class="text-xs text-muted-foreground mb-1">{{ $t('memoryUsage') }}（占整机 %）</div>
              <TrendChart metric="memory" :points="processPoints(row.pid)" color-var="--color-chart-2" height="120" max="100" />
            </div>
          </div>
        </div>
      </div>
    </div>
```

2. Script：新增状态与逻辑（在既有 `runningApps` 声明附近）：

```ts
import type { ProcessSample } from '@/models/process'
import { toast } from '@/composables/useToast'

const processSamples = systemStore.processSamples
const expandedPids = ref<Set<number>>(new Set())
/** 告警去重：pid:metric 已告警标记 */
const alerted = ref<Set<string>>(new Set())

interface ProcRow { name: string; type: 'software' | 'springboot'; pid: number; cpu: number; memBytes: number; memPct: number }

const processRows = computed<ProcRow[]>(() => {
  const rows: ProcRow[] = []
  for (const s of runningSoftware.value) {
    if (s.pid == null) continue
    rows.push({ name: s.name, type: 'software', pid: s.pid, cpu: 0, memBytes: 0, memPct: 0 })
  }
  for (const a of runningApps.value) {
    if (a.pid == null) continue
    rows.push({ name: a.name, type: 'springboot', pid: a.pid, cpu: 0, memBytes: 0, memPct: 0 })
  }
  return rows
})

function processPoints(pid: number): HistoryPoint[] {
  return systemStore.processSamples[pid] ?? []
}

function toggleProcess(pid: number) {
  const next = new Set(expandedPids.value)
  next.has(pid) ? next.delete(pid) : next.add(pid)
  expandedPids.value = next
}
```

> 注意：`processRows` 里的 `cpu/memBytes/memPct` 初始为 0，需用实时采样回填。设计为每 1s 采样后更新——因 `TrendChart` 用 `processPoints` 取自 store 历史，表格行内实时值可由一个 `latestSamples` ref 提供：

```ts
const latestSamples = ref<Map<number, ProcessSample>>(new Map())
```

并在采样回调里 `latestSamples.value.set(s.pid, s)`。`processRows` 从 `latestSamples` 取实时值（有则用，无则 0）：

```ts
const processRows = computed<ProcRow[]>(() => {
  // … 构造行时：
  const smp = latestSamples.value.get(pid)
  const memPct = systemStore.memTotal > 0 ? ((smp?.mem_bytes ?? 0) / systemStore.memTotal) * 100 : 0
  rows.push({ name, type, pid, cpu: smp?.cpu_usage ?? 0, memBytes: smp?.mem_bytes ?? 0, memPct })
})
```

3. 采样 + 告警循环：在 `onMounted` 里既有 `tickTimer` 1s 间隔中，追加采样与告警（与整机轮询共用节奏）。新增一个 `setInterval` 或并入现有定时：

```ts
  // 进程采样 + 告警：与整机轮询同频（1s）
  const procTimer = window.setInterval(async () => {
    const pids = processRows.value.map((r) => r.pid).filter((p) => p != null)
    if (pids.length === 0) return
    await systemStore.sampleProcesses(pids)
    for (const s of latestSamples.value.values()) {
      maybeAlert(s.name ?? '', s)   // simplified call, see below
    }
  }, 1000)
```

为简化，告警检测写入一个独立辅助（在 script 内），接收各实时行：

```ts
function checkAlerts() {
  const THRESHOLD_CPU = 90
  const THRESHOLD_MEM = 90
  for (const row of processRows.value) {
    if (row.cpu >= THRESHOLD_CPU) {
      const key = `${row.pid}:cpu`
      if (!alerted.value.has(key)) {
        alerted.value.add(key)
        toast(t('processAlertCpu', { name: row.name, value: row.cpu.toFixed(0) }), 'err')
      }
    } else {
      alerted.value.delete(`${row.pid}:cpu`)
    }
    if (row.memPct >= THRESHOLD_MEM) {
      const key = `${row.pid}:mem`
      if (!alerted.value.has(key)) {
        alerted.value.add(key)
        toast(t('processAlertMem', { name: row.name, value: row.memPct.toFixed(0) }), 'err')
      }
    } else {
      alerted.value.delete(`${row.pid}:mem`)
    }
  }
}
```

`onUnmounted` 清理 `procTimer`。

- [ ] **Step 4: i18n 新增 key**

`src/locales/zh-CN.ts`（在「仪表盘卡片」或 system 相关区，任意位置追加）：

```ts
  processMonitor: '进程资源监控',
  noRunningProcess: '暂无运行中的进程',
  view: '查看',
  collapse: '收起',
  processAlertCpu: '进程「{name}」CPU {value}% 已超过阈值 90%',
  processAlertMem: '进程「{name}」内存占用 {value}% 已超过阈值 90%',
```

`src/locales/en-US.ts`（对应）：

```ts
  processMonitor: 'Process Monitor',
  noRunningProcess: 'No running processes',
  view: 'View',
  collapse: 'Collapse',
  processAlertCpu: 'Process "{name}" CPU {value}% exceeded 90% threshold',
  processAlertMem: 'Process "{name}" memory usage {value}% exceeded 90% threshold',
```

> 若 `view`/`collapse` key 已存在于其他模块（如已定义），改为复用现有 key 或加前缀 `processView`/`processCollapse` 避免冲突。以 Grep 定位后决定。

- [ ] **Step 5: 前端类型检查 + 构建**

Run: `npx vue-tsc --noEmit`
Expected: 无类型错误。

- [ ] **Step 6: 全量测试确认（跨线回归）**

Run: `cargo test --lib`（从 `src-tauri/`）
Expected: 全绿。

- [ ] **Step 7: Commit**

```bash
git add src/models/process.ts src/stores/system.ts src/modules/system-monitor/pages/DashboardPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(monitor): 系统监控页新增进程资源监控与阈值告警"
```

---

## Self-Review 记录

- **Spec 覆盖:** 后端采样模块(T1)、命令+注册(T2)、前端 model/store/区块/告警/i18n(T3)，内存归一化为占整机%(T3 store 计算)、阈值告警去重(T3)、进程退出跳过(T1 filter_map)——全覆盖。
- **占位符扫描:** 所有步骤含具体代码与命令，无 TBD。
- **类型一致性:** `ProcessSample { pid, cpu_usage, mem_bytes }` 后端 struct 与前端 interface 字段一致；`sample_process_resources(pids: Vec<u32>)` 后端 ↔ 前端 `invoke('sample_process_resources', { pids })`；`sample_processes(&[u32]) -> Vec<ProcessSample>` 定义与 T2 调用一致。
- **边界:** 内存趋势归一化（T3 store 用 memTotal 算 pct）、进程退出跳过（T1 filter_map）、告警去重（T3 alerted）、固定阈值 90%（T3 常量）——均与小 spec「边界」节一致。
- **已知待验证:** sysinfo 0.31.0 `refresh_processes` 签名单/双参数可能在编译时需调整（T1 Step 3 已标注两方案）。
