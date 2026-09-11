# 崩溃自愈（看门狗）设计

> 状态：已确认设计
> 日期：2026-09-11

## 背景与现状

- 软件实例、SpringBoot 应用、Node 应用都是 OPX 直接 spawn 的长期进程。进程意外崩溃后 OPX 不会感知，也不会拉起：现有代码只在「被查询状态」时把 pid 已死的 Running 记为 Error（如 `software_manager::SoftwareManager::list`），没有后台监听。
- SpringBoot 模型 `SpringBootApp.auto_restart` 字段与前端 `AppFormDialog` 开关**均已存在**，但全仓无任何消费者——是个「存了没实现」的空壳。
- 可复用件：
  - `services/software_manager/lifecycle.rs`：`ProcessRegistry`、`spawn_process`、`stop_one`
  - `services/software_manager/health_check.rs::is_process_alive(pid)`
  - 后台常驻循环先例：`services/software_manager/backup_scheduler::run_scheduler`（`lib.rs` 中 `tauri::async_runtime::spawn` 启动）
  - 三者启动调用方式（见 `commands/software.rs::do_start_software`、`services/springboot_manager/lifecycle.rs::start_app`、`NodeAppManager::start`）

## 已确认决策

1. **覆盖范围**：软件实例 + SpringBoot 应用 + Node 应用（三类进程型实体）。
2. **重启策略**：意外退出后延迟 ~2s 重启；连续失败达上限（3 次）则放弃。
3. **参数**：看门狗轮询间隔 5s；稳定运行 60s 后重置失败计数。
4. **「意外退出」仅指进程消失**，不做健康检查级「假活」检测。

## 设计

### 1. 模型

- `src-tauri/src/models/software.rs` 的 `InstalledSoftware` 增：

  ```rust
  /// 进程意外退出后自动重启
  #[serde(default)]
  pub auto_restart: bool,
  ```

- `src-tauri/src/models/node_app.rs` 的 `NodeApp` 增同名字段（`#[serde(default)]`）。
- SpringBoot 复用现有 `SpringBootApp.auto_restart`（零模型改动）。
- 前端 `src/models/software.ts`、`src/models/node-app.ts` 同步加可选布尔字段。

### 2. 「意外退出」判定

- 判据：`auto_restart == true` 且状态为 Running 且 `pid` 存在 且 `!is_process_alive(pid)`。
- **用户主动停止不误判**：停止流程先置状态 Stopping/Stopped 并清 pid，故不再满足「Running 且有 pid」。
- **OPX 退出不误判**：`stop_all_on_exit` 同样先置状态；看门狗线程随 OPX 进程结束而终止。
- 抽为纯函数便于单测：

  ```rust
  /// 是否应视为意外退出（需重启）
  pub fn is_unexpected_exit(auto_restart: bool, running: bool, pid: Option<u32>, alive: bool) -> bool
  ```

### 3. 看门狗模块 `src-tauri/src/services/watchdog.rs`（新）

- 入口：

  ```rust
  pub async fn run_watchdog(
      software: Arc<SoftwareManager>,
      springboot: Arc<SpringBootManager>,
      node: Arc<NodeAppManager>,
      app: tauri::AppHandle,
      node_exe: Option<PathBuf>,
  )
  ```

  在 `lib.rs` setup 中 `tauri::async_runtime::spawn` 启动，无限循环：`sleep(5s)` → 扫描三类实体 → 对命中项处理。

- 重启调用（复用既有路径，不新写启动逻辑）：
  - 软件 → `crate::commands::software::do_start_software(&software, &app, id, None).await`
  - SpringBoot → `crate::services::springboot_manager::lifecycle::start_app(id, &springboot, &software, &app).await`
  - Node → `node.start(id, &exe)`（`node_exe` 为 None 则跳过并记一次失败）
  - 每次重启前 `sleep(2s)`。

- 失败计数（内存态，不落盘）：`Mutex<HashMap<String, Attempt>>`，`Attempt { failures: u32, last_restart_at: Instant }`。
  - 重启成功 → `failures = 0`，记 `last_restart_at`。
  - 重启失败 → `failures += 1`。
  - `failures >= 3` → 放弃：不再尝试，状态置 Error：
    - 软件 → 写回状态并 emit 既有 `software-status-changed`
    - SpringBoot → 写回状态并 emit 既有 `springboot-status-changed`
    - Node → 写回状态（Node 无状态事件，前端靠 `auto-restart-giveup` 触发列表刷新）
  - 同时 emit `"auto-restart-giveup"`，载荷 `{ kind: "software"|"springboot"|"node", id, name }`；前端 `App.vue` 监听后调用 `toast(...)` 提示（与 `App.vue` 既有 `listen('close-requested')` / `listen('stop-complete')` 同法）。Node 页面另行监听该事件以刷新列表。
  - 稳定 60s 后重置：下一轮扫描时若 `last_restart_at.elapsed() >= 60s` 则把 `failures` 归零。

- 决策与重置抽为纯函数：

  ```rust
  pub enum Action { Restart, GiveUp }
  pub fn next_action(failures: u32, limit: u32) -> Action
  /// 距上次重启已稳定运行 reset_after 则以「0 次失败」重新计数
  pub fn effective_failures(failures: u32, elapsed_secs: u64, reset_after_secs: u64) -> u32
  ```

### 4. 配置入口

- **软件**：`StartupSettingsDialog.vue` 增「崩溃后自动重启」开关；`commands/software.rs::save_startup_settings` 增加 `auto_restart: bool` 参数，`SoftwareManager::update_startup_settings` 同步写入。
- **Node**：`NodeAppsPage.vue` 表单增同名开关，保存/回显经其既有保存路径。
- **SpringBoot**：复用 `AppFormDialog` 现有开关（零 UI 改动）。
- i18n：`autoRestart` 键已存在（`zh-CN.ts:295`），软件/Node 表单直接复用；如描述文案缺失再补。

### 5. 可观测性

- 每次自动重启成功：`oplog!("auto_restart", name, &format!("第 {} 次", failures_before + 1))`（`failures_before` 为本次重启前的计数；明细复用已落地的审计 JSONL）。
- 放弃时：`oplog!("auto_restart_giveup", name, &format!("连续 {} 次失败", failures))` + 前端 toast 通知。

### 6. 测试

- `watchdog.rs` 纯函数单测（`cargo test --lib`）：
  - `is_unexpected_exit`：pid 死→true；pid 活→false；无 pid→false；非 Running→false；`auto_restart=false`→false。
  - `next_action`：`failures < limit` → Restart；`failures >= limit` → GiveUp。
  - `effective_failures`：未到稳定阈值→原值；达到→0。
- 真实重启行为靠实机走查（杀进程观察是否拉起、连续失败是否放弃）。
- 前端 `npx vue-tsc --noEmit`。

## 边界（不做）

- 不做健康检查/端口级「假活」检测（仅进程退出）。
- 不做服务组级联重启（服务组成员是软件/SpringBoot 实体，由本机制各自覆盖）。
- 失败计数为内存态，不持久化；OPX 重启后重新计数。
- 仅 OPX 运行期间生效；OPX 退出不拉起被停服务。
- 不做指数退避（固定 2s 延迟）。

## 改动文件清单

- `src-tauri/src/models/software.rs`（`InstalledSoftware.auto_restart`）
- `src-tauri/src/models/node_app.rs`（`NodeApp.auto_restart`）
- `src-tauri/src/services/watchdog.rs`（新）
- `src-tauri/src/services/mod.rs`（注册模块）
- `src-tauri/src/lib.rs`（spawn 看门狗）
- `src-tauri/src/services/software_manager/mod.rs`（`update_startup_settings` 增字段）
- `src-tauri/src/commands/software.rs`（`save_startup_settings` 增参数）
- `src-tauri/src/commands/node_app.rs`（保存/回显 `auto_restart`）
- `src/models/software.ts`、`src/models/node-app.ts`
- `src/modules/software-manager/components/StartupSettingsDialog.vue`
- `src/modules/node-apps/NodeAppsPage.vue`
- `src/App.vue`（监听 `auto-restart-giveup` → toast）
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`（如需补充描述/提示文案）
