# 指标历史持久化 + 告警规则 设计

> 状态：已确认设计
> 日期：2026-09-14

## 背景与现状（已勘察）

- **历史持久化是半成品**：`services/system_monitor/history.rs` 只有 `load_history()`，**全仓无任何写入方**（`system_history.json` 永远不会被创建）。命令 `commands::system::system_history` 读它 → 恒为空。
- 前端 `stores/system.ts`：每 **1s** 轮询 `system_info`，内存保留 `MAX_HISTORY = 120` 点（≈**2 分钟**）；重启即丢。进程级 `processSamples` 同样是内存 120 点环形。
- **告警硬编码在前端**：`DashboardPage` 的 `THRESHOLD_CPU/MEM = 90`，`checkAlerts()` 只 `toast`，去重集合 `alerted` 是内存 `Set`（重启即忘）；**阈值不可配、无告警历史**，且**只在监控页打开时才告警**（切页即失去告警能力）。
- 已有可复用件：`process_monitor::sample_processes`、`backup_scheduler::run_scheduler` 常驻循环范式、`oplog!` 审计（JSONL + 操作记录页）、`App.vue` 全局事件→toast 范式（`auto-restart-giveup` / `process-exited`）。

## 已确认决策

1. **告警范围**：整机（CPU/内存）+ 单实例（软件/SpringBoot 进程的 CPU/内存）。
2. **告警历史**：复用「操作记录」页（写 `oplog!` 审计），不新建存储与页面。
3. 阈值存 `AppSettings`，设置页可配，默认 90%。
4. 进程级历史**只采样当前运行的实例**；停止后不再采样。

## 设计

### 1. 后端采样器 `src-tauri/src/services/system_monitor/recorder.rs`（新）

- 入口：

  ```rust
  pub async fn run_recorder(
      app: tauri::AppHandle,
      software: Arc<SoftwareManager>,
      springboot: Arc<SpringBootManager>,
  )
  ```

  在 `lib.rs` setup 中 `tauri::async_runtime::spawn` 启动；`tokio::time::interval(30s)` 常驻循环（复用 `backup_scheduler::run_scheduler` 的写法）。

- 每轮动作：
  1. 取整机 CPU/内存（复用 `system_monitor::info` 的现有取值逻辑，产出 `HistoryPoint`）。
  2. 取运行中实例 pid：`software.get_installed()`（`status == Running` 且有 pid）+ `springboot.snapshot_apps()`（`AppStatus::Running` 且有 pid）。
  3. `process_monitor::sample_processes(&pids)` 采样这些 pid。
  4. 把整机与各进程样本**追加**写入 `data/metrics_history.json`，并做**滚动保留 7 天**裁剪。
  5. 对整机与各进程分别做阈值判定（见 §2）。

- 存储格式（沿用现有模型，不引入新格式）：

  ```rust
  // data/metrics_history.json
  #[derive(Serialize, Deserialize)]
  pub struct MetricsHistory {
      /// 整机样本（30s 粒度）
      pub system: Vec<HistoryPoint>,
      /// pid -> 样本（pid 用字符串作 JSON key）
      pub processes: std::collections::HashMap<String, Vec<HistoryPoint>>,
  }
  ```

- 落盘与裁剪放在 `history.rs`（存储职责），recorder 只调用：

  ```rust
  pub fn save_history(path: &Path, h: &MetricsHistory) -> Result<()>

  /// 按时间戳（**毫秒**，与前端 `Date.now()` 一致）裁剪掉早于 now_ms - retain_days 的点。
  pub fn prune_older_than(points: &mut Vec<HistoryPoint>, now_ms: i64, retain_days: i64)
  ```

- 常量：`SAMPLE_INTERVAL_SECS = 30`、`RETAIN_DAYS = 7`。

- 命令改造：`system_history` 返回 `MetricsHistory.system`（签名仍是 `Vec<HistoryPoint>`，前端调用不变）；新增 `process_metrics_history()` 返回 `MetricsHistory.processes`（`HashMap<String, Vec<HistoryPoint>>`，键为 pid 字符串）。

### 2. 告警判定（同一循环内）

- `AppSettings` 增 4 个阈值（`#[serde(default)]`，默认 90）：

  ```rust
  #[serde(default = "default_ninety")]
  pub alert_system_cpu: u32,
  #[serde(default = "default_ninety")]
  pub alert_system_mem: u32,
  #[serde(default = "default_ninety")]
  pub alert_process_cpu: u32,
  #[serde(default = "default_ninety")]
  pub alert_process_mem: u32,
  ```

- 判定纯函数：

  ```rust
  /// 返回 true 表示本条告警应被触发（此前未处于告警态）。
  pub fn should_alert(value: f64, threshold: u32, already_alerting: bool) -> bool
  /// 返回 true 表示已恢复：值回落到阈值以下并带 2 个百分点的迟滞（`value < threshold - 2.0`），避免临界抖动反复告警/恢复。
  pub fn is_recovered(value: f64, threshold: u32) -> bool
  ```

- 去重状态：内存 `HashSet<String>`，key 形如 `system:cpu` / `proc:<pid>:mem`；触发入集合，恢复出集合（可再次触发）。
- 触发时：
  - `crate::oplog!("alert_high", &format!("{} CPU {}%（阈值 {}%）", name, value, threshold))`
  - `app.emit("resource-alert", json!({ "kind": "system"|"process", "name": name, "metric": "cpu"|"mem", "value": value, "threshold": threshold }))`
- 恢复时：`crate::oplog!("alert_recovered", ...)`（不发 toast，避免打扰）。
- 审计条目自动出现在「操作记录」页（可搜索/导出），无需新增 UI。

### 3. 前端

- `App.vue`：监听 `resource-alert` → `toast(t('resourceAlert', { name, metric, value, threshold }), 'err')`；`onUnmounted` 注销（与既有 `auto-restart-giveup` / `process-exited` 同法）。
- `DashboardPage`：
  - 删除 `THRESHOLD_CPU` / `THRESHOLD_MEM` / `checkAlerts()` / `alerted`（告警统一由后端负责，切页也能告警）。
  - 趋势图**只读后端持久化序列**（避免 30s 历史与 1s 实时点混在一个数组里被 120 点上限切光）：整机图用 `system_history`，进程行内图用 `process_metrics_history`，**每 30s 刷新一次**（与后端采样间隔一致）。
- `stores/system.ts`：删除前端历史累积与 `MAX_HISTORY`（连带 `history` / `processSamples` 的 push 逻辑）——1s 轮询仅保留用于「当前值」（统计卡的数值、进程表每行的 CPU/内存），历史一律来自后端。进程表仍可通过 `sampleProcesses` 拿实时值，但不再累积进历史数组。
- 设置页新增「告警阈值」区：4 个数值输入（整机 CPU/内存、单实例 CPU/内存），随既有 400ms 防抖保存流程提交。
- i18n（中英）：`alertThresholds`、`alertSystemCpu`、`alertSystemMem`、`alertProcessCpu`、`alertProcessMem`、`resourceAlert`。

### 4. 测试

- `cargo test --lib`：
  - `should_alert`：未超限 false / 超限且未告警 true / 已告警 false
  - `is_recovered`：高于阈值 false / 低于阈值（含 2% 迟滞边界）true
  - `prune_older_than`：删除早于 `now_ms - retain_days*86400_000` 的点、保留边界点、空输入（时间戳单位与前端 `Date.now()` 一致，均为毫秒）
  - `MetricsHistory` / `AppSettings` 旧 JSON（无新字段）反序列化取默认值（90）
- 前端 `npx vue-tsc --noEmit`
- 实机走查：把阈值临时调到很低 → 触发 toast + 操作记录出现 `alert_high`；调回后出现 `alert_recovered`

## 边界（不做）

- 不引入时序数据库（JSON 文件足够，7 天 30s 粒度 ≈ 20k 点）。
- 不做告警通知渠道（邮件 / Webhook / 系统通知）。
- 不做告警静默窗口 / 抑制规则（仅 2% 迟滞防抖）。
- 不持久化未运行实例的指标（只采当前运行中的软件/SpringBoot）。
- 前端保留 1s 轮询，但**只用于当前值显示**（统计卡与进程表数字）；**趋势曲线**统一改为后端 30s 持久化序列（不再有 1s 粒度曲线，换来重启后可查 7 天）。

## 改动文件清单

- `src-tauri/src/services/system_monitor/recorder.rs`（新）
- `src-tauri/src/services/system_monitor/mod.rs`（注册模块）
- `src-tauri/src/services/system_monitor/history.rs`（增 `save_history` / 裁剪）
- `src-tauri/src/models/system.rs`（`MetricsHistory`）
- `src-tauri/src/models/settings.rs`（4 个阈值 + `default_ninety`）
- `src-tauri/src/commands/system.rs`（`system_history` 改造 + `process_metrics_history`）
- `src-tauri/src/lib.rs`（spawn recorder + 注册命令）
- `src/stores/system.ts`、`src/modules/system-monitor/pages/DashboardPage.vue`
- `src/App.vue`（`resource-alert` toast）
- `src/modules/settings/pages/SettingsPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
