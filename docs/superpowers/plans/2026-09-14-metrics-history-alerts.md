# 指标历史持久化 + 告警规则 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把系统/进程指标以 30s 粒度持久化保留 7 天，并把告警（阈值可配、触发/恢复记审计）搬到后端常驻循环。

**Architecture:** 新增 `services/system_monitor/recorder.rs` 常驻采样器（复用 `backup_scheduler` 范式），落 `data/metrics_history.json` 并按天裁剪；阈值存 `AppSettings`；告警触发/恢复写 `oplog!` 审计 + emit `resource-alert`。前端 1s 轮询仅保留「当前值」，趋势曲线改读后端持久化序列（每 30s 刷新）。

**Tech Stack:** Rust（tauri v2, tokio, sysinfo, chrono, serde_json）、Vue 3 + TS + Chart.js（经 `TrendChart.vue`）。

## Global Constraints

- 分支：`feat/metrics-alerts`（已创建；spec 已提交 `203437d`）。
- 不引入新依赖。
- 采样间隔 `SAMPLE_INTERVAL_SECS = 30`；保留 `RETAIN_DAYS = 7`；告警迟滞 2 个百分点。
- 历史文件：`crate::utils::paths::data_dir().join("metrics_history.json")`。
- `HistoryPoint.timestamp` 为**毫秒**（与前端 `Date.now()` 一致）。
- 阈值默认 90，字段名：`alert_system_cpu` / `alert_system_mem` / `alert_process_cpu` / `alert_process_mem`（`#[serde(default = "default_ninety")]`）。
- 审计动作名：`alert_high` / `alert_recovered`；事件名 `resource-alert`，载荷 `{ kind: "system"|"process", name, metric: "cpu"|"mem", value, threshold }`。
- 后端验证 `cd src-tauri && cargo test --lib`；前端 `npx vue-tsc --noEmit`。
- i18n 键必须同时加 `src/locales/zh-CN.ts` 与 `src/locales/en-US.ts`。
- 提交信息中文；不加 Co-Authored-By。

---

### Task 1: 模型与纯函数（含单测）

**Files:**
- Modify: `src-tauri/src/models/system.rs`（`MetricsHistory`）
- Modify: `src-tauri/src/models/settings.rs`（4 个阈值 + `default_ninety`）
- Modify: `src-tauri/src/services/system_monitor/history.rs`（`save_history` / `load_metrics` / `prune_older_than`）
- Create: `src-tauri/src/services/system_monitor/alerts.rs`（`should_alert` / `is_recovered`）
- Modify: `src-tauri/src/services/system_monitor/mod.rs`（注册 `alerts`）
- Test: 上述文件内联 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `pub struct MetricsHistory { pub system: Vec<HistoryPoint>, pub processes: HashMap<String, Vec<HistoryPoint>> }`（`Default`）
  - `pub fn save_history(path: &Path, h: &MetricsHistory) -> anyhow::Result<()>`
  - `pub fn load_metrics(path: &Path) -> anyhow::Result<MetricsHistory>`
  - `pub fn prune_older_than(points: &mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64)`
  - `pub fn should_alert(value: f64, threshold: u32, already_alerting: bool) -> bool`
  - `pub fn is_recovered(value: f64, threshold: u32) -> bool`

- [ ] **Step 1: 写失败的测试**

在 `history.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::system::HistoryPoint;

    fn pt(ms: u64) -> HistoryPoint {
        HistoryPoint { timestamp: ms, cpu_usage: 1.0, memory_usage: 2.0 }
    }

    #[test]
    fn prune_removes_older_than_retain_days() {
        let now = 10_000_000_000i64; // ms
        let day = 86_400_000i64;
        let mut pts = vec![pt((now - 8 * day) as u64), pt((now - 6 * day) as u64), pt(now as u64)];
        prune_older_than(&mut pts, now, 7);
        assert_eq!(pts.len(), 2, "8 天前的点应被删除，6 天前与当前保留");
        assert_eq!(pts[0].timestamp, (now - 6 * day) as u64);
    }

    #[test]
    fn prune_keeps_exact_boundary_and_handles_empty() {
        let now = 10_000_000_000i64;
        let day = 86_400_000i64;
        // 恰好等于 cutoff 的点保留（>= 判定）
        let mut pts = vec![pt((now - 7 * day) as u64)];
        prune_older_than(&mut pts, now, 7);
        assert_eq!(pts.len(), 1);
        let mut empty: Vec<HistoryPoint> = Vec::new();
        prune_older_than(&mut empty, now, 7);
        assert!(empty.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("__qa_metrics_{}", chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("metrics_history.json");
        let mut h = MetricsHistory::default();
        h.system.push(pt(123));
        h.processes.insert("42".to_string(), vec![pt(456)]);
        save_history(&path, &h).unwrap();
        let back = load_metrics(&path).unwrap();
        assert_eq!(back.system.len(), 1);
        assert_eq!(back.processes.get("42").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

在 `alerts.rs` 内写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_alert_only_when_crossing_and_not_already_alerting() {
        assert!(!should_alert(89.9, 90, false));
        assert!(should_alert(90.0, 90, false));
        assert!(should_alert(99.0, 90, false));
        assert!(!should_alert(99.0, 90, true), "已在告警态不重复触发");
    }

    #[test]
    fn is_recovered_requires_hysteresis() {
        assert!(!is_recovered(95.0, 90));
        assert!(!is_recovered(88.5, 90), "阈值下 1.5 个百分点仍在迟滞区，未恢复");
        assert!(is_recovered(87.9, 90), "低于 阈值的 2 个百分点才判恢复");
        assert!(is_recovered(10.0, 90));
    }
}
```

在 `models/settings.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_thresholds_default_to_90_on_legacy_json() {
        let json = r#"{
            "theme":"auto","language":"zh-CN","sidebar_collapsed":false,
            "software_root":"apps","config_root":"config","mirror_url":"https://mirrors.aliyun.com",
            "auto_check_update":true,"close_window_action":"CloseToTray","ask_on_close":true,
            "jre_default_id":null,"github_proxy_url":"","proxy_url":""
        }"#;
        let s: AppSettings = serde_json::from_str(json).expect("legacy settings must load");
        assert_eq!(s.alert_system_cpu, 90);
        assert_eq!(s.alert_system_mem, 90);
        assert_eq!(s.alert_process_cpu, 90);
        assert_eq!(s.alert_process_mem, 90);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib system_monitor:: alerts::`
Expected: 编译失败（类型/函数未定义）

- [ ] **Step 3: 实现**

`models/system.rs` 追加：

```rust
/// 持久化指标历史（30s 粒度，保留 7 天）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsHistory {
    #[serde(default)]
    pub system: Vec<HistoryPoint>,
    /// pid（字符串）-> 该进程的样本
    #[serde(default)]
    pub processes: std::collections::HashMap<String, Vec<HistoryPoint>>,
}
```

`models/settings.rs` 的 `AppSettings` 追加并在 `Default` 同步：

```rust
    /// 告警阈值（百分比）。默认 90。
    #[serde(default = "default_ninety")]
    pub alert_system_cpu: u32,
    #[serde(default = "default_ninety")]
    pub alert_system_mem: u32,
    #[serde(default = "default_ninety")]
    pub alert_process_cpu: u32,
    #[serde(default = "default_ninety")]
    pub alert_process_mem: u32,
```

```rust
fn default_ninety() -> u32 {
    90
}
```

`services/system_monitor/history.rs` 改为（**删除**原 `load_history`）：

```rust
use crate::models::system::{HistoryPoint, MetricsHistory};
use anyhow::Result;
use std::path::Path;

pub fn load_metrics(path: &Path) -> Result<MetricsHistory> {
    if !path.exists() {
        return Ok(MetricsHistory::default());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

pub fn save_history(path: &Path, h: &MetricsHistory) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string(h)?)?;
    Ok(())
}

/// 删除早于 `now_ms - retain_days` 的点（时间戳单位：毫秒，与前端 Date.now() 一致）。
pub fn prune_older_than(points: &mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64) {
    let cutoff = now_ms - retain_days * 86_400_000;
    points.retain(|p| (p.timestamp as i64) >= cutoff);
}
```

`services/system_monitor/alerts.rs`（新）：

```rust
//! 资源告警判定（纯函数，便于单测）。

/// 是否需要触发告警：达到阈值且此前不在告警态。
pub fn should_alert(value: f64, threshold: u32, already_alerting: bool) -> bool {
    value >= threshold as f64 && !already_alerting
}

/// 是否已恢复：回落到「阈值 - 2 个百分点」以下（迟滞，避免临界抖动反复告警）。
pub fn is_recovered(value: f64, threshold: u32) -> bool {
    value < threshold as f64 - 2.0
}
```

`services/system_monitor/mod.rs` 加 `pub mod alerts;`。

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib system_monitor:: alerts:: models::settings::`
Expected: PASS（5 个新测试）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/models/system.rs src-tauri/src/models/settings.rs src-tauri/src/services/system_monitor/
git commit -m "feat(metrics): 指标历史模型与告警纯函数"
```

---

### Task 2: 系统采样入口集中到 system_monitor

**Files:**
- Modify: `src-tauri/src/services/system_monitor/info.rs`（新增 `sample_system()` + 全局 `System` 静态）
- Modify: `src-tauri/src/commands/system.rs`（`system_info` 改用它；删除本地静态）

**Interfaces:**
- Produces: `pub fn sample_system() -> SystemInfo`

- [ ] **Step 1: 加采样入口**

`services/system_monitor/info.rs` 顶部加：

```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use sysinfo::System;

/// 全局 System 实例：CPU% 依赖两次 refresh 的时间差，recorder 与 system_info 命令
/// 必须共用同一实例，否则各自算出的增量都不准。
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new();
    s.refresh_all();
    thread::sleep(Duration::from_millis(200));
    s.refresh_all();
    Mutex::new(s)
});

/// 采样一次整机信息（复用全局 System 基线）。
pub fn sample_system() -> SystemInfo {
    let mut system = SYSTEM.lock().unwrap();
    get_system_info(&mut system)
}
```

- [ ] **Step 2: 命令改用入口**

`commands/system.rs` 的 `system_info` 改为：

```rust
#[tauri::command]
pub fn system_info() -> SystemInfo {
    crate::services::system_monitor::info::sample_system()
}
```

并删除该文件中的 `static SYSTEM`、`Lazy`/`Mutex`/`thread`/`Duration`/`sysinfo::System` 相关 import（若不再使用）。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo check --lib`
Expected: Finished（无未使用 import 警告）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/services/system_monitor/info.rs src-tauri/src/commands/system.rs
git commit -m "refactor(metrics): 系统采样入口集中到 system_monitor（共用 CPU 基线）"
```

---

### Task 3: 采样器循环（落盘 + 裁剪 + 告警）

**Files:**
- Create: `src-tauri/src/services/system_monitor/recorder.rs`
- Modify: `src-tauri/src/services/system_monitor/mod.rs`（注册 `recorder`）
- Modify: `src-tauri/src/lib.rs`（spawn 循环）

**Interfaces:**
- Consumes: `info::sample_system()`、`history::{load_metrics, save_history, prune_older_than}`、`alerts::{should_alert, is_recovered}`、`process_monitor::sample_processes`、`SoftwareManager::get_installed()`、`SpringBootManager::snapshot_apps()`、`config::read_settings()`、`oplog!`
- Produces: `pub async fn run_recorder(app: AppHandle, software: Arc<SoftwareManager>, springboot: Arc<SpringBootManager>)`；`pub const SAMPLE_INTERVAL_SECS: u64 = 30;`、`pub const RETAIN_DAYS: i64 = 7;`

- [ ] **Step 1: 实现**

```rust
//! 指标采样器：每 30s 采样整机与运行中实例，落盘保留 7 天，并做阈值告警。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::system::HistoryPoint;
use crate::services::software_manager::{process_monitor, SoftwareManager};
use crate::services::springboot_manager::SpringBootManager;
use crate::services::system_monitor::{alerts, history, info};
use crate::utils::paths;

pub const SAMPLE_INTERVAL_SECS: u64 = 30;
pub const RETAIN_DAYS: i64 = 7;

fn metrics_path() -> PathBuf {
    paths::data_dir().join("metrics_history.json")
}

pub async fn run_recorder(
    app: AppHandle,
    software: Arc<SoftwareManager>,
    springboot: Arc<SpringBootManager>,
) {
    let mut alerting: HashSet<String> = HashSet::new();
    let mut tick = tokio::time::interval(Duration::from_secs(SAMPLE_INTERVAL_SECS));
    tick.tick().await; // 消耗初始化 tick
    loop {
        tick.tick().await;
        sample_once(&app, &software, &springboot, &mut alerting).await;
    }
}

async fn sample_once(
    app: &AppHandle,
    software: &Arc<SoftwareManager>,
    springboot: &Arc<SpringBootManager>,
    alerting: &mut HashSet<String>,
) {
    let thresholds = crate::commands::config::read_settings().unwrap_or_default();
    let sys = info::sample_system();
    let now_ms = chrono::Local::now().timestamp_millis();

    // 运行中实例：(显示名, pid)
    let mut targets: Vec<(String, u32)> = Vec::new();
    for sw in software.get_installed() {
        if sw.status == crate::models::software::SoftwareStatus::Running {
            if let Some(pid) = sw.pid {
                targets.push((sw.name.clone(), pid));
            }
        }
    }
    for sb in springboot.snapshot_apps() {
        if sb.status == crate::models::springboot::AppStatus::Running {
            if let Some(pid) = sb.pid {
                targets.push((sb.name.clone(), pid));
            }
        }
    }

    let pids: Vec<u32> = targets.iter().map(|(_, p)| *p).collect();
    let samples = process_monitor::sample_processes(&pids);

    let mut h = history::load_metrics(&metrics_path()).unwrap_or_default();
    h.system.push(HistoryPoint {
        timestamp: now_ms as u64,
        cpu_usage: sys.cpu_usage,
        memory_usage: sys.memory_usage,
    });
    for s in &samples {
        let mem_pct = if sys.memory_total > 0 {
            s.mem_bytes as f64 / sys.memory_total as f64 * 100.0
        } else {
            0.0
        };
        h.processes
            .entry(s.pid.to_string())
            .or_default()
            .push(HistoryPoint {
                timestamp: now_ms as u64,
                cpu_usage: s.cpu_usage,
                memory_usage: mem_pct,
            });
        if let Some(pts) = h.processes.get_mut(&s.pid.to_string()) {
            history::prune_older_than(pts, now_ms, RETAIN_DAYS);
        }
    }
    history::prune_older_than(&mut h.system, now_ms, RETAIN_DAYS);
    // 丢弃已无样本的进程键，避免文件无限膨胀
    h.processes.retain(|_, pts| !pts.is_empty());
    if let Err(e) = history::save_history(&metrics_path(), &h) {
        tracing::warn!(error = %e, "写入指标历史失败");
    }

    // ---- 告警判定 ----
    let name_of = |pid: u32| {
        targets
            .iter()
            .find(|(_, p)| *p == pid)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| format!("PID {pid}"))
    };
    eval(app, alerting, "system:cpu", "整机", "cpu", sys.cpu_usage, thresholds.alert_system_cpu);
    eval(app, alerting, "system:mem", "整机", "mem", sys.memory_usage, thresholds.alert_system_mem);
    for s in &samples {
        let mem_pct = if sys.memory_total > 0 {
            s.mem_bytes as f64 / sys.memory_total as f64 * 100.0
        } else {
            0.0
        };
        let name = name_of(s.pid);
        eval(app, alerting, &format!("proc:{}:cpu", s.pid), &name, "cpu", s.cpu_usage, thresholds.alert_process_cpu);
        eval(app, alerting, &format!("proc:{}:mem", s.pid), &name, "mem", mem_pct, thresholds.alert_process_mem);
    }
}

/// 触发/恢复单条告警：触发写审计 + emit 事件；恢复只写审计（不打扰）。
fn eval(
    app: &AppHandle,
    alerting: &mut HashSet<String>,
    key: &str,
    name: &str,
    metric: &str,
    value: f64,
    threshold: u32,
) {
    if alerts::should_alert(value, threshold, alerting.contains(key)) {
        alerting.insert(key.to_string());
        crate::oplog!("alert_high", &format!("{} {} {}%（阈值 {}%）", name, metric, value.round(), threshold));
        let _ = app.emit(
            "resource-alert",
            serde_json::json!({ "kind": if key.starts_with("proc:") { "process" } else { "system" }, "name": name, "metric": metric, "value": value.round(), "threshold": threshold }),
        );
    } else if alerting.contains(key) && alerts::is_recovered(value, threshold) {
        alerting.remove(key);
        crate::oplog!("alert_recovered", &format!("{} {} {}%", name, metric, value.round()));
    }
}
```

`services/system_monitor/mod.rs` 加 `pub mod recorder;`。

- [ ] **Step 2: 接线 lib.rs**

在既有 spawn 区（看门狗/续期调度附近）追加：

```rust
            // 指标采样器：30s 采样整机与运行中实例，落盘 7 天，并做阈值告警
            let rec_app = app.handle().clone();
            let rec_sw = app
                .state::<std::sync::Arc<crate::services::software_manager::SoftwareManager>>()
                .inner()
                .clone();
            let rec_sb = app
                .state::<std::sync::Arc<crate::services::springboot_manager::SpringBootManager>>()
                .inner()
                .clone();
            tauri::async_runtime::spawn(async move {
                crate::services::system_monitor::recorder::run_recorder(rec_app, rec_sw, rec_sb).await;
            });
```

- [ ] **Step 3: 编译 + 测试**

Run: `cd src-tauri && cargo check --lib && cargo test --lib`
Expected: Finished；测试全绿（Task 1 的 5 个新测试在内）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/services/system_monitor/recorder.rs src-tauri/src/services/system_monitor/mod.rs src-tauri/src/lib.rs
git commit -m "feat(metrics): 30s 采样器（落盘 7 天 + 阈值告警与审计）"
```

---

### Task 4: 命令改造

**Files:**
- Modify: `src-tauri/src/commands/system.rs`
- Modify: `src-tauri/src/lib.rs`（注册 `process_metrics_history`）

**Interfaces:**
- Produces: `system_history() -> Result<Vec<HistoryPoint>, String>`（改读新文件）；`process_metrics_history() -> Result<HashMap<String, Vec<HistoryPoint>>, String>`

- [ ] **Step 1: 改命令**

```rust
#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    let path = paths::data_dir().join("metrics_history.json");
    let h = system_monitor::history::load_metrics(&path).map_err(|e| e.to_string())?;
    Ok(h.system)
}

/// 各进程（pid 字符串键）的持久化样本，供 Dashboard 行内趋势图。
#[tauri::command]
pub fn process_metrics_history() -> Result<std::collections::HashMap<String, Vec<HistoryPoint>>, String> {
    let path = paths::data_dir().join("metrics_history.json");
    let h = system_monitor::history::load_metrics(&path).map_err(|e| e.to_string())?;
    Ok(h.processes)
}
```

`lib.rs` 的 `generate_handler!` 注册 `commands::system::process_metrics_history`。

- [ ] **Step 2: 编译 + 提交**

Run: `cd src-tauri && cargo check --lib` → Finished

```bash
git add src-tauri/src/commands/system.rs src-tauri/src/lib.rs
git commit -m "feat(metrics): system_history 读新存储，新增 process_metrics_history"
```

---

### Task 5: 前端 —— 曲线改读后端持久化序列

**Files:**
- Modify: `src/stores/system.ts`（删除前端历史累积与 `MAX_HISTORY`）
- Modify: `src/modules/system-monitor/pages/DashboardPage.vue`（删硬编码阈值与 `checkAlerts`；曲线读后端，每 30s 刷新）

**Interfaces:**
- Consumes: 命令 `system_history`、`process_metrics_history`、`sample_process_resources`（实时值）

- [ ] **Step 1: 精简 store**

`src/stores/system.ts`：
- 删除 `history`、`processSamples`、`MAX_HISTORY`（及其 push/裁剪逻辑与 `return` 里的导出）。
- `fetchAll()` 只更新当前值（`systemInfo`、`cpuUsage`、`memoryUsage` 等）与网络增量基线，不再累积历史。
- `sampleProcesses(pids)` 保留（Dashboard 用于取实时值），但只返回样本、不写历史。

- [ ] **Step 2: Dashboard 改造**

`DashboardPage.vue`：
- 删除 `THRESHOLD_CPU` / `THRESHOLD_MEM` / `alerted` / `checkAlerts()` 及其调用（告警已由后端负责）。
- 新增：

```ts
const sysHistory = ref<HistoryPoint[]>([])
const procHistory = ref<Record<string, HistoryPoint[]>>({})

async function loadMetricsHistory() {
  try {
    sysHistory.value = await invoke<HistoryPoint[]>('system_history')
    procHistory.value = await invoke<Record<string, HistoryPoint[]>>('process_metrics_history')
  } catch {
    // 首次尚无历史文件：保持空数组即可
  }
}
```

- `onMounted` 调 `loadMetricsHistory()`，并加 30s 定时器再次调用（与后端采样间隔一致）；`onUnmounted` 清理。
- 整机两张 `TrendChart` 的 `:points` 由 `systemStore.history` 改为 `sysHistory`。
- 进程行内 `TrendChart` 的 `points` 由 `processPoints(pid)`（内存）改为 `procHistory.value[String(pid)] ?? []`。
- 进程表的实时 CPU/内存数字仍来自 `sampleProcesses`（1s）——不变。

- [ ] **Step 3: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 4: 提交**

```bash
git add src/stores/system.ts src/modules/system-monitor/pages/DashboardPage.vue
git commit -m "feat(metrics): 趋势曲线改读后端持久化序列，移除前端历史累积"
```

---

### Task 6: 前端 —— 告警 toast 与阈值配置

**Files:**
- Modify: `src/App.vue`（`resource-alert` → toast）
- Modify: `src/models/settings.ts`（4 个阈值字段）
- Modify: `src/modules/settings/pages/SettingsPage.vue`（告警阈值区）
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`

- [ ] **Step 1: App.vue 监听**

在既有 `auto-restart-giveup` / `process-exited` 监听旁追加：

```ts
  unlistenResourceAlert = await listen<{ name: string; metric: string; value: number; threshold: number }>(
    'resource-alert',
    (e) => {
      const p = e.payload
      if (!p) return
      toast(t('resourceAlert', { name: p.name, metric: p.metric === 'cpu' ? 'CPU' : t('memory'), value: p.value, threshold: p.threshold }), 'err')
    },
  )
```

声明 `let unlistenResourceAlert: UnlistenFn | null = null`，并在 `onUnmounted` 加 `unlistenResourceAlert?.()`。

- [ ] **Step 2: 设置页阈值区**

`src/models/settings.ts` 的 `AppSettings` 加 `alert_system_cpu: number` 等 4 个字段。

`SettingsPage.vue`：在「DNS 服务商」区后新增「告警阈值」区，4 行数值输入（整机 CPU / 整机内存 / 单实例 CPU / 单实例内存），绑定 4 个新 ref；把 refs 加入既有防抖 watcher 数组与 `save()`。

- [ ] **Step 3: i18n（中英各加 6 键）**

zh：`alertThresholds: '告警阈值'`、`alertSystemCpu: '整机 CPU 超过'`、`alertSystemMem: '整机内存超过'`、`alertProcessCpu: '单实例 CPU 超过'`、`alertProcessMem: '单实例内存超过'`、`resourceAlert: '「{name}」{metric} {value}%（阈值 {threshold}%）'`。
en：`alertThresholds: 'Alert thresholds'`、`alertSystemCpu: 'System CPU above'`、`alertSystemMem: 'System memory above'`、`alertProcessCpu: 'Instance CPU above'`、`alertProcessMem: 'Instance memory above'`、`resourceAlert: '"{name}" {metric} {value}% (threshold {threshold}%)'`。

- [ ] **Step 4: 类型检查 + 提交**

Run: `npx vue-tsc --noEmit` → 无输出

```bash
git add src/App.vue src/models/settings.ts src/modules/settings/pages/SettingsPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(metrics): 告警 toast 与阈值配置入口"
```

---

### Task 7: 全量验证

- [ ] **Step 1: 后端**

Run: `cd src-tauri && cargo test --lib`
Expected: 全绿（139 + 新增 5）

- [ ] **Step 2: 前端**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 3: 实机走查**

1. `npm run tauri:dev` 启动，等 ≥1 个采样周期（30s），确认 `data/metrics_history.json` 生成且有内容
2. 系统监控页趋势图应显示来自持久化的曲线；**重启应用后曲线仍在**（此前会清空）
3. 设置页把「整机 CPU / 单实例 CPU」阈值临时调到 1%（必然触发）→ 30s 内应弹 toast，且「操作记录」页出现 `alert_high`
4. 阈值调回 90 → 下一次采样应出现 `alert_recovered`
5. 切到其它页面（如软件管理）杀掉一个高占用进程 → 告警仍能触发（验证「切页也能告警」）

- [ ] **Step 4: 提交（若走查有微调）**

```bash
git add -A && git commit -m "chore(metrics): 实机走查微调"
```

---

## Self-Review

**Spec coverage：**
- 30s 采样整机 + 运行中实例、落 `metrics_history.json`、保留 7 天 → Task 1（模型/裁剪）+ Task 3（循环）
- 4 个阈值入 `AppSettings`（默认 90，旧配置兼容）→ Task 1
- 告警触发/恢复写审计、emit `resource-alert`、2% 迟滞、内存去重 → Task 1（纯函数）+ Task 3（eval）
- 顺手修「只在监控页才告警」→ Task 3（后端常驻）
- `system_history` 改造 + `process_metrics_history` → Task 4
- 曲线只读后端序列、删前端历史累积与硬编码阈值 → Task 5
- toast + 设置页阈值 → Task 6
- `HistoryPoint.timestamp` 毫秒一致性 → Task 1 注释与测试
- 边界（不做时序库/通知渠道/静默窗口/未运行实例采样）→ 未安排任务，符合 spec

**Placeholder scan：** 无 TBD/TODO；后端步骤均含完整代码。Task 5/6 的前端步骤给出精确的 ref/函数名与绑定变化，属结构性改动说明而非占位。

**Type consistency：**
- `MetricsHistory{system, processes: HashMap<String, Vec<HistoryPoint>>}` 在 Task 1 定义，Task 3（写入）、Task 4（读出）一致。
- `prune_older_than(&mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64)` 在 Task 1 定义、Task 3 调用一致（毫秒）。
- `should_alert(value, threshold, already_alerting)` / `is_recovered(value, threshold)` 在 Task 1 定义、Task 3 调用一致（迟滞 2.0）。
- `sample_system()` 在 Task 2 定义、Task 3 调用一致。
- 命令名 `system_history` / `process_metrics_history` 在 Task 4 定义、Task 5 调用一致；事件名 `resource-alert` 与审计动作名 `alert_high`/`alert_recovered` 在 Task 3 定义、Task 6 消费一致。
- 阈值字段名 `alert_system_cpu`/`alert_system_mem`/`alert_process_cpu`/`alert_process_mem` 在 Rust（Task 1）与 TS（Task 6）一致，JS 侧经既有 Tauri 驼峰映射使用。
