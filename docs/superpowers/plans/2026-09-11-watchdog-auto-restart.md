# 崩溃自愈（看门狗）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 软件实例 / SpringBoot 应用 / Node 应用进程意外退出后按「延迟 + 上限」策略自动拉起；连续失败达上限则放弃并告警。

**Architecture:** 新增 `services/watchdog.rs` 后台常驻循环（5s 轮询），复用 `health_check::is_process_alive` 判定意外退出，复用现有三个启动路径拉起；失败计数为内存态 `HashMap`；纯逻辑（判定/上限/重置）抽为可测函数。模型为软件与 Node 增加 `auto_restart` 字段，SpringBoot 复用既有字段。

**Tech Stack:** Rust（tauri v2, tokio, serde, chrono）、Vue 3 + TypeScript。

## Global Constraints

- 分支：`feat/watchdog-auto-restart`（已创建；spec 已提交 `8d2bb7f`）。
- 不引入新依赖（tokio / serde / chrono / once_cell 已在用）。
- 参数固定：轮询 `POLL_INTERVAL=5s`、重启延迟 `RESTART_DELAY=2s`、上限 `MAX_FAILURES=3`、稳定重置 `RESET_AFTER_SECS=60`。
- 「意外退出」仅指进程消失（`!is_process_alive(pid)`），不做健康检查级假活检测。
- 后端验证：`cd src-tauri && cargo test --lib`；前端：`npx vue-tsc --noEmit`。
- i18n 键必须同时加 `src/locales/zh-CN.ts` 与 `src/locales/en-US.ts`。
- 提交信息中文；不加 Co-Authored-By。
- 复用既有标识：启动软件用 `crate::commands::software::do_start_software`；SpringBoot 用 `crate::services::springboot_manager::lifecycle::start_app`；Node 用 `NodeAppManager::start`。

---

### Task 1: 模型字段 `auto_restart`

**Files:**
- Modify: `src-tauri/src/models/software.rs`（`InstalledSoftware`）
- Modify: `src-tauri/src/models/node_app.rs`（`NodeApp`、`CreateNodeAppParams`、`UpdateNodeAppParams`）
- Modify: `src/models/software.ts`、`src/models/node-app.ts`
- Test: `src-tauri/src/models/software.rs`（内联 `#[cfg(test)] mod tests`）

**Interfaces:**
- Produces: `InstalledSoftware.auto_restart: bool`；`NodeApp.auto_restart: bool`；`CreateNodeAppParams.auto_restart: bool`、`UpdateNodeAppParams.auto_restart: Option<bool>`（均 `#[serde(default)]`）。

- [ ] **Step 1: 写失败的测试**

在 `src-tauri/src/models/software.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 旧 installed.json 无 auto_restart 字段时必须能反序列化（默认 false），
    /// 否则升级后加载旧数据会整体失败。
    #[test]
    fn installed_software_without_auto_restart_defaults_false() {
        let json = r#"{
            "id":"x","key":"k","version":"1.0","name":"n","install_path":"p",
            "install_time":"2024-01-01T00:00:00","status":"Stopped","port":0,
            "config":{},"is_custom":false,"auto_start_on_app_start":false,
            "startup_order":0,"source":{"Builtin":{"version":"1.0"}}
        }"#;
        let sw: InstalledSoftware = serde_json::from_str(json).expect("old json loads");
        assert!(!sw.auto_restart);
    }
}
```

在 `src-tauri/src/models/node_app.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_app_params_default_auto_restart_false() {
        let p: CreateNodeAppParams =
            serde_json::from_str(r#"{"name":"a","entry_path":"b"}"#).expect("params parse");
        assert!(!p.auto_restart);
        let u: UpdateNodeAppParams = serde_json::from_str(r#"{}"#).expect("update parse");
        assert!(u.auto_restart.is_none());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib models::`
Expected: 编译失败（`auto_restart` 字段不存在）

- [ ] **Step 3: 加字段**

`src-tauri/src/models/software.rs` 的 `InstalledSoftware`，在 `depends_on` 声明之后追加：

```rust
    /// 进程意外退出后自动重启
    #[serde(default)]
    pub auto_restart: bool,
```

`src-tauri/src/models/node_app.rs` 的 `NodeApp`，在 `startup_order` 之后（`// ===== 运行时字段 =====` 之前）追加：

```rust
    /// 进程意外退出后自动重启
    #[serde(default)]
    pub auto_restart: bool,
```

同文件 `CreateNodeAppParams` 的 `startup_order` 之后追加：

```rust
    #[serde(default)]
    pub auto_restart: bool,
```

`UpdateNodeAppParams` 的 `startup_order` 之后追加：

```rust
    #[serde(default)]
    pub auto_restart: Option<bool>,
```

前端 `src/models/software.ts` 的 `InstalledSoftware` 接口加 `auto_restart: boolean`；`src/models/node-app.ts` 的 `NodeApp` 与创建/更新载荷接口加 `auto_restart: boolean` / `auto_restart?: boolean`。

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib models::`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/models/software.rs src-tauri/src/models/node_app.rs src/models/software.ts src/models/node-app.ts
git commit -m "feat(watchdog): 软件/Node 模型增加 auto_restart 字段"
```

---

### Task 2: 看门狗纯逻辑 + 单测

**Files:**
- Create: `src-tauri/src/services/watchdog.rs`
- Modify: `src-tauri/src/services/mod.rs`（新增 `pub mod watchdog;`）
- Test: `src-tauri/src/services/watchdog.rs`（内联）

**Interfaces:**
- Produces:
  - `pub enum Action { Restart, GiveUp }`
  - `pub fn next_action(failures: u32, limit: u32) -> Action`
  - `pub fn effective_failures(failures: u32, elapsed_secs: u64, reset_after_secs: u64) -> u32`
  - `pub fn is_unexpected_exit(auto_restart: bool, running: bool, pid: Option<u32>, alive: bool) -> bool`
  - `struct WatchdogState { attempts: HashMap<String, Attempt> }` 及 `failures_now/record_success/record_failure/record_healthy`

- [ ] **Step 1: 写失败的测试**

创建 `src-tauri/src/services/watchdog.rs`，先只写测试与最少的 `use`：

```rust
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_action_stops_at_limit() {
        assert_eq!(next_action(0, 3), Action::Restart);
        assert_eq!(next_action(2, 3), Action::Restart);
        assert_eq!(next_action(3, 3), Action::GiveUp);
        assert_eq!(next_action(4, 3), Action::GiveUp);
    }

    #[test]
    fn is_unexpected_exit_requires_all_conditions() {
        // 开启 + 运行中 + 有 pid + 进程已死 → true
        assert!(is_unexpected_exit(true, true, Some(1), false));
        // 进程活着 → false
        assert!(!is_unexpected_exit(true, true, Some(1), true));
        // 无 pid → false
        assert!(!is_unexpected_exit(true, true, None, false));
        // 非运行态 → false
        assert!(!is_unexpected_exit(true, false, Some(1), false));
        // 未开启自动重启 → false
        assert!(!is_unexpected_exit(false, true, Some(1), false));
    }

    #[test]
    fn failures_accumulate_without_time_reset() {
        let mut s = WatchdogState::new();
        assert_eq!(s.failures("software:a"), 0);
        s.record_failure("software:a");
        s.record_failure("software:a");
        assert_eq!(s.failures("software:a"), 2);
    }

    #[test]
    fn successful_restart_clears_failures_and_given_up() {
        let mut s = WatchdogState::new();
        s.record_failure("node:x");
        s.record_success("node:x");
        assert_eq!(s.failures("node:x"), 0);
        assert!(!s.is_given_up("node:x"));
    }

    #[test]
    fn given_up_blocks_until_healthy_observed() {
        let mut s = WatchdogState::new();
        s.record_failure("springboot:y");
        s.mark_given_up("springboot:y");
        assert!(s.is_given_up("springboot:y"));
        // 观察到健康（用户手动拉起成功）→ 解除并清零
        s.record_healthy("springboot:y");
        assert!(!s.is_given_up("springboot:y"));
        assert_eq!(s.failures("springboot:y"), 0);
    }

    #[test]
    fn record_healthy_does_not_create_missing_entry() {
        let mut s = WatchdogState::new();
        s.record_healthy("ghost");
        assert_eq!(s.failures("ghost"), 0);
        assert!(!s.is_given_up("ghost"));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib watchdog::`
Expected: 编译失败（类型/函数未定义）；同时在 `src-tauri/src/services/mod.rs` 加入 `pub mod watchdog;`

- [ ] **Step 3: 写最小实现**

在 `watchdog.rs` 的测试模块之前写入：

```rust
//! 崩溃自愈看门狗：周期性检测三类实体（软件 / SpringBoot / Node）进程是否意外退出，
//! 对开启 auto_restart 的项按「延迟 + 上限」策略自动拉起；连续失败达上限则放弃。

use std::collections::HashMap;

pub const POLL_INTERVAL_SECS: u64 = 5;
pub const RESTART_DELAY_SECS: u64 = 2;
pub const MAX_FAILURES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Restart,
    GiveUp,
}

/// 连续失败未达上限 → 继续重启；达到 → 放弃。
pub fn next_action(failures: u32, limit: u32) -> Action {
    if failures >= limit {
        Action::GiveUp
    } else {
        Action::Restart
    }
}

/// 是否应视为「意外退出」：开启自动重启 + 运行中 + 有 pid + 进程已不存在。
pub fn is_unexpected_exit(auto_restart: bool, running: bool, pid: Option<u32>, alive: bool) -> bool {
    auto_restart && running && pid.is_some() && !alive
}

#[derive(Debug, Clone, Copy, Default)]
struct Attempt {
    failures: u32,
    /// 已达上限并放弃自动重启；仅当再次观察到健康才解除。
    given_up: bool,
}

/// 各实体重启尝试状态（内存态，不落盘）
pub struct WatchdogState {
    attempts: HashMap<String, Attempt>,
}

impl WatchdogState {
    pub fn new() -> Self {
        Self {
            attempts: HashMap::new(),
        }
    }

    /// 当前失败次数（不随时间自动归零）。
    pub fn failures(&self, key: &str) -> u32 {
        self.attempts.get(key).map(|a| a.failures).unwrap_or(0)
    }

    pub fn is_given_up(&self, key: &str) -> bool {
        self.attempts.get(key).map(|a| a.given_up).unwrap_or(false)
    }

    pub fn record_success(&mut self, key: &str) {
        let a = self.attempts.entry(key.to_string()).or_default();
        a.failures = 0;
        a.given_up = false;
    }

    pub fn record_failure(&mut self, key: &str) {
        self.attempts.entry(key.to_string()).or_default().failures += 1;
    }

    /// 观察到「运行中且存活」→ 健康：清零失败计数并解除「已放弃」。
    pub fn record_healthy(&mut self, key: &str) {
        if let Some(a) = self.attempts.get_mut(key) {
            a.failures = 0;
            a.given_up = false;
        }
    }

    /// 达到上限时置「已放弃」。
    pub fn mark_given_up(&mut self, key: &str) {
        self.attempts.entry(key.to_string()).or_default().given_up = true;
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --lib watchdog::`
Expected: PASS（5 个测试）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/watchdog.rs src-tauri/src/services/mod.rs
git commit -m "feat(watchdog): 意外退出判定/上限/重置纯逻辑 + 单测"
```

---

### Task 3: 看门狗循环 + 接线

**Files:**
- Modify: `src-tauri/src/services/watchdog.rs`
- Modify: `src-tauri/src/services/node_app_manager.rs`（新增 `snapshot` / `set_status`）
- Modify: `src-tauri/src/services/springboot_manager/mod.rs`（新增 `snapshot_apps`）
- Modify: `src-tauri/src/lib.rs`（spawn 看门狗）

**Interfaces:**
- Consumes（Task 2 与既有代码）：
  - `watchdog::{WatchdogState, is_unexpected_exit, next_action, Action, MAX_FAILURES, POLL_INTERVAL_SECS, RESTART_DELAY_SECS}`
  - `software_manager::health_check::is_process_alive(u32) -> bool`
  - `SoftwareManager::get_installed() -> Vec<InstalledSoftware>`、`update_runtime_fields(&str, SoftwareStatus, Option<u32>, Option<NaiveDateTime>, Option<NaiveDateTime>, Option<String>) -> Result<()>`
  - `lifecycle::emit_status_changed(&AppHandle, &str, SoftwareStatus, Option<u32>, Option<String>)`
  - `commands::software::do_start_software(&Arc<SoftwareManager>, &AppHandle, &str, Option<String>) -> anyhow::Result<()>`
  - `SpringBootManager::list_apps() -> Vec<SpringBootApp>`、`update_status(&str, AppStatus, Option<u32>, Option<String>) -> Result<()>`
  - `springboot_manager::lifecycle::start_app(&str, &SpringBootManager, &SoftwareManager, &AppHandle) -> Result<(), String>`
  - `NodeAppManager::list() -> Vec<NodeApp>`、`start(&str, &Path) -> Result<(), String>`
- Produces: `pub async fn run_watchdog(software: Arc<SoftwareManager>, springboot: Arc<SpringBootManager>, node: Arc<NodeAppManager>, app: tauri::AppHandle, node_exe: Option<PathBuf>)`；`NodeAppManager::snapshot(&self) -> Vec<NodeApp>`、`NodeAppManager::set_status(&self, app_id: &str, status: NodeAppStatus, pid: Option<u32>, error: Option<String>) -> Result<(), String>`；`SpringBootManager::snapshot_apps(&self) -> Vec<SpringBootApp>`

> **为什么必须用快照**（预检发现的关键点）：`NodeAppManager::list()` 会把死进程的 Running 改判为 Error 并落盘；`SpringBootManager::list_apps()` 会改判为 Stopped **并清空 pid** 并落盘。若看门狗用它们，就读不到「Running + 有 pid」这一意外退出前提，永远不会触发重启。因此必须用**不产生副作用的快照**读取。
>
> 另外：重启前必须把陈旧的 `Running` 复位为 `Stopped`——`lifecycle::validate_start_transition` 与 `springboot start_app` 都会拒绝 `Running`。Node 的 `start` 只校验 pid 存活性，无需复位。

- [ ] **Step 1: 管理器加无损快照与状态写回**

在 `src-tauri/src/services/node_app_manager.rs` 的 `impl NodeAppManager` 内（`stop` 之后）加：

```rust
    /// 只读快照：不改状态、不落盘（供看门狗判定意外退出）
    pub fn snapshot(&self) -> Vec<NodeApp> {
        self.inner.lock().unwrap().apps.clone()
    }

    /// 写回状态/pid/错误并落盘（看门狗复位或放弃时使用）
    pub fn set_status(
        &self,
        app_id: &str,
        status: NodeAppStatus,
        pid: Option<u32>,
        error: Option<String>,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let a = inner
            .apps
            .iter_mut()
            .find(|a| a.id == app_id)
            .ok_or_else(|| format!("未找到 Node 应用: {}", app_id))?;
        a.status = status;
        a.pid = pid;
        a.last_error = error;
        let apps = inner.apps.clone();
        if let Some(parent) = inner.data_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(&apps) {
            let _ = std::fs::write(&inner.data_path, content);
        }
        Ok(())
    }
```

在 `src-tauri/src/services/springboot_manager/mod.rs` 的 `impl SpringBootManager` 内（`list_apps` 之后）加：

```rust
    /// 只读快照：不做死进程纠正、不落盘（供看门狗判定意外退出）
    pub fn snapshot_apps(&self) -> Vec<SpringBootApp> {
        self.store.read().unwrap().applications.clone()
    }
```

- [ ] **Step 2: 实现循环**

在 `watchdog.rs` 顶部补 `use`，并在文件末尾（测试模块之前）加入：

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::{health_check, SoftwareManager};
use crate::services::springboot_manager::SpringBootManager;

/// 看门狗主循环：永不返回，随 OPX 进程结束而终止。
pub async fn run_watchdog(
    software: Arc<SoftwareManager>,
    springboot: Arc<SpringBootManager>,
    node: Arc<NodeAppManager>,
    app: tauri::AppHandle,
    node_exe: Option<PathBuf>,
) {
    let mut state = WatchdogState::new();
    loop {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        watch_software(&software, &app, &mut state).await;
        watch_springboot(&software, &springboot, &app, &mut state).await;
        watch_node(&node, &app, node_exe.as_deref(), &mut state).await;
    }
}

async fn watch_software(
    software: &Arc<SoftwareManager>,
    app: &tauri::AppHandle,
    state: &mut WatchdogState,
) {
    for sw in software.get_installed() {
        let key = format!("software:{}", sw.id);
        let running = sw.status == SoftwareStatus::Running;
        let alive = running && sw.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(sw.auto_restart, running, sw.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = software.update_runtime_fields(
                    &sw.id,
                    SoftwareStatus::Error,
                    None,
                    None,
                    None,
                    Some(msg.clone()),
                );
                crate::services::software_manager::lifecycle::emit_status_changed(
                    app,
                    &sw.id,
                    SoftwareStatus::Error,
                    None,
                    Some(msg),
                );
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "software", "id": sw.id, "name": sw.name }),
                );
                crate::oplog!("auto_restart_giveup", &sw.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        // 陈旧 Running 会被 validate_start_transition 拒绝，先复位为 Stopped
        let _ = software.update_runtime_fields(
            &sw.id,
            SoftwareStatus::Stopped,
            None,
            None,
            Some(chrono::Local::now().naive_local()),
            None,
        );
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match crate::commands::software::do_start_software(software, app, &sw.id, None).await {
            Ok(_) => {
                crate::oplog!("auto_restart", &sw.name, &format!("第 {} 次", failures + 1));
                state.record_success(&key);
            }
            Err(e) => {
                tracing::warn!(id = %sw.id, error = %e, "看门狗重启软件失败");
                state.record_failure(&key);
            }
        }
    }
}

async fn watch_springboot(
    software: &Arc<SoftwareManager>,
    springboot: &Arc<SpringBootManager>,
    app: &tauri::AppHandle,
    state: &mut WatchdogState,
) {
    for sb in springboot.snapshot_apps() {
        let key = format!("springboot:{}", sb.id);
        let running = sb.status == AppStatus::Running;
        let alive = running && sb.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(sb.auto_restart, running, sb.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = springboot.update_status(&sb.id, AppStatus::Error, None, Some(msg.clone()));
                let _ = app.emit(
                    "springboot-status-changed",
                    (sb.id.clone(), "Error", None::<u32>, Some(msg)),
                );
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "springboot", "id": sb.id, "name": sb.name }),
                );
                crate::oplog!("auto_restart_giveup", &sb.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        // 陈旧 Running 会被 start_app 拒绝，先复位为 Stopped
        let _ = springboot.update_status(&sb.id, AppStatus::Stopped, None, None);
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match crate::services::springboot_manager::lifecycle::start_app(
            &sb.id, springboot, software, app,
        )
        .await
        {
            Ok(_) => {
                crate::oplog!("auto_restart", &sb.name, &format!("第 {} 次", failures + 1));
                state.record_success(&key);
            }
            Err(e) => {
                tracing::warn!(id = %sb.id, error = %e, "看门狗重启 SpringBoot 失败");
                state.record_failure(&key);
            }
        }
    }
}

async fn watch_node(
    node: &Arc<NodeAppManager>,
    app: &tauri::AppHandle,
    node_exe: Option<&std::path::Path>,
    state: &mut WatchdogState,
) {
    for na in node.snapshot() {
        let key = format!("node:{}", na.id);
        let running = na.status == crate::models::node_app::NodeAppStatus::Running;
        let alive = running && na.pid.map_or(false, health_check::is_process_alive);
        if running && alive {
            state.record_healthy(&key);
            continue;
        }
        if !is_unexpected_exit(na.auto_restart, running, na.pid, alive) {
            continue;
        }
        if state.is_given_up(&key) {
            continue;
        }
        let failures = state.failures(&key);
        if next_action(failures, MAX_FAILURES) == Action::GiveUp {
            state.mark_given_up(&key);
            {
                let msg = format!("自动重启失败，已放弃（连续 {} 次）", failures);
                let _ = node.set_status(&na.id, crate::models::node_app::NodeAppStatus::Error, None, Some(msg));
                let _ = app.emit(
                    "auto-restart-giveup",
                    serde_json::json!({ "kind": "node", "id": na.id, "name": na.name }),
                );
                crate::oplog!("auto_restart_giveup", &na.name, &format!("连续 {} 次失败", failures));
            }
            continue;
        }
        let Some(exe) = node_exe else {
            state.record_failure(&key);
            continue;
        };
        tokio::time::sleep(Duration::from_secs(RESTART_DELAY_SECS)).await;
        match node.start(&na.id, exe) {
            Ok(_) => {
                crate::oplog!("auto_restart", &na.name, &format!("第 {} 次", failures + 1));
                state.record_success(&key);
            }
            Err(e) => {
                tracing::warn!(id = %na.id, error = %e, "看门狗重启 Node 应用失败");
                state.record_failure(&key);
            }
        }
    }
}
```

> 注意：`NodeAppStatus` 需在 `src-tauri/src/models/node_app.rs` 中是 `pub`；`SpringBootApp.pid`/`status`/`auto_restart`/`name` 均为 `pub`。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo check --lib`
Expected: Finished，无错误。若 `NodeAppStatus` 未 pub 或字段私有，按编译提示补 `pub`。

- [ ] **Step 4: 在 lib.rs 启动看门狗**

在 `src-tauri/src/lib.rs` 的统一启动编排 spawn 之后（同一 `setup` 闭包内）追加：

```rust
            // 崩溃自愈看门狗：周期性检测意外退出并按策略自动拉起
            let wd_software = app
                .state::<std::sync::Arc<crate::services::software_manager::SoftwareManager>>()
                .inner()
                .clone();
            let wd_springboot = app
                .state::<std::sync::Arc<crate::services::springboot_manager::SpringBootManager>>()
                .inner()
                .clone();
            let wd_node = node_mgr.clone();
            let wd_app = app.handle().clone();
            let wd_node_exe = node_exe.clone();
            tauri::async_runtime::spawn(async move {
                crate::services::watchdog::run_watchdog(
                    wd_software,
                    wd_springboot,
                    wd_node,
                    wd_app,
                    wd_node_exe,
                )
                .await;
            });
```

- [ ] **Step 5: 全量测试**

Run: `cd src-tauri && cargo test --lib`
Expected: 全绿（含 Task 1/2 新测试）

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/watchdog.rs src-tauri/src/services/node_app_manager.rs src-tauri/src/services/springboot_manager/mod.rs src-tauri/src/lib.rs
git commit -m "feat(watchdog): 三实体意外退出自动拉起 + lib.rs 接线"
```

---

### Task 4: 软件启动设置接入 `auto_restart`

**Files:**
- Modify: `src-tauri/src/services/software_manager/mod.rs`（`update_startup_settings`）
- Modify: `src-tauri/src/commands/software.rs`（`save_startup_settings`）
- Modify: `src/modules/software-manager/components/StartupSettingsDialog.vue`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`（如需描述文案）

**Interfaces:**
- Consumes: `InstalledSoftware.auto_restart`（Task 1）
- Produces: `SoftwareManager::update_startup_settings(&self, installed_id: &str, auto_start: bool, order: u32, auto_restart: bool) -> Result<()>`；命令 `save_startup_settings(installedId, autoStart, order, autoRestart)`

- [ ] **Step 1: 后端签名与写入**

`src-tauri/src/services/software_manager/mod.rs` 的 `update_startup_settings` 增加参数并写入：

```rust
    pub fn update_startup_settings(
        &self,
        installed_id: &str,
        auto_start: bool,
        order: u32,
        auto_restart: bool,
    ) -> Result<()> {
```

函数体内，在原 `item.auto_start_on_app_start = auto_start;` / `item.startup_order = order;` 之后加：

```rust
        item.auto_restart = auto_restart;
```

`src-tauri/src/commands/software.rs` 的 `save_startup_settings` 增加参数并透传：

```rust
pub async fn save_startup_settings(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    auto_start: bool,
    order: u32,
    auto_restart: bool,
) -> Result<(), String> {
    let name = manager.find_installed(&installed_id).map(|s| s.name).unwrap_or_default();
    oplog!("save_startup", &format!("{} ({})", name, installed_id));
    manager
        .update_startup_settings(&installed_id, auto_start, order, auto_restart)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo check --lib`
Expected: 无错误。若其它调用点报错，按提示补第四参（本项目仅 `save_startup_settings` 一处调用）。

- [ ] **Step 3: 前端开关**

`src/modules/software-manager/components/StartupSettingsDialog.vue`：

- 在 `autoStart` 同处加 `const autoRestart = ref(false)`
- 初始化处（读取 `props.software.auto_start_on_app_start` / `startup_order` 旁）加：

```ts
  autoRestart.value = props.software.auto_restart ?? false
```

- 模板中在「应用启动时自动拉起」勾选框之后加：

```html
        <label class="check-row">
          <input type="checkbox" v-model="autoRestart" />
          {{ $t('autoRestart') }}
        </label>
```

（class 名沿用该对话框既有勾选框的类，保持样式一致；若既有类名不同，用同款。）

- 保存调用补参数：

```ts
    await invoke('save_startup_settings', {
      installedId: props.software.id,
      autoStart: autoStart.value,
      order: order.value,
      autoRestart: autoRestart.value,
    })
```

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/software_manager/mod.rs src-tauri/src/commands/software.rs src/modules/software-manager/components/StartupSettingsDialog.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(watchdog): 软件启动设置支持开启自动重启"
```

---

### Task 5: Node 应用接入 `auto_restart`

**Files:**
- Modify: `src-tauri/src/services/node_app_manager.rs`（create/update 透传）
- Modify: `src/modules/node-apps/NodeAppsPage.vue`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`（如需）

**Interfaces:**
- Consumes: `CreateNodeAppParams.auto_restart`、`UpdateNodeAppParams.auto_restart`（Task 1）
- Produces: Node 创建/更新保存 `auto_restart`

- [ ] **Step 1: manager 透传**

`src-tauri/src/services/node_app_manager.rs` 的 `create` 中，构造 `NodeApp { ... }` 处加入 `auto_restart: payload.auto_restart,`；`update` 中按既有可选字段写法加入：

```rust
        if let Some(v) = params.auto_restart {
            app.auto_restart = v;
        }
```

- [ ] **Step 2: 编译**

Run: `cd src-tauri && cargo check --lib`
Expected: 无错误（若 `NodeApp` 构造因新字段报 missing field，补 `auto_restart` 初始化）

- [ ] **Step 3: 前端表单**

`src/modules/node-apps/NodeAppsPage.vue`：

- `form` 默认值（现有 `auto_start: false, startup_order: 0` 处）加 `auto_restart: false`
- 编辑回显（现有 `auto_start: a.auto_start, startup_order: a.startup_order` 处）加 `auto_restart: a.auto_restart`
- 模板中「应用启动时自动拉起」勾选框旁加：

```html
          <label class="chk"><input type="checkbox" v-model="form.auto_restart" /> {{ $t('autoRestart') }}</label>
```

- 保存载荷（现有 `auto_start: form.value.auto_start, startup_order: form.value.startup_order` 处）加 `auto_restart: form.value.auto_restart`

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/services/node_app_manager.rs src/modules/node-apps/NodeAppsPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(watchdog): Node 应用支持开启自动重启"
```

---

### Task 6: 前端放弃通知

**Files:**
- Modify: `src/App.vue`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
- Modify: `src/modules/node-apps/NodeAppsPage.vue`（监听事件刷新列表）

**Interfaces:**
- Consumes: 后端事件 `auto-restart-giveup`，载荷 `{ kind: "software"|"springboot"|"node", id, name }`
- Produces: 放弃时 toast 提示

- [ ] **Step 1: i18n 键**

`src/locales/zh-CN.ts` 顶层加：

```ts
  autoRestartGiveUp: '「{name}」自动重启连续失败，已放弃（请检查日志）',
```

`src/locales/en-US.ts` 顶层加：

```ts
  autoRestartGiveUp: 'Auto-restart for "{name}" gave up after repeated failures (check logs)',
```

- [ ] **Step 2: App.vue 监听**

在 `src/App.vue` 既有 `listen('tray-software-stop', ...)` 之后加：

```ts
  unlistenAutoRestartGiveUp = await listen<{ name: string }>('auto-restart-giveup', (e) => {
    toast(t('autoRestartGiveUp', { name: e.payload?.name ?? '' }), 'err')
  })
```

并在 `onUnmounted` 中加 `unlistenAutoRestartGiveUp?.()`；按该文件既有写法声明 `let unlistenAutoRestartGiveUp: UnlistenFn | null = null`，并确保 `toast`、`t`、`UnlistenFn` 已在该文件可用（若无则按既有 import 方式补 `import { toast } from '@/composables/useToast'`、`useI18n`/`UnlistenFn`）。

- [ ] **Step 3: Node 页面刷新**

`src/modules/node-apps/NodeAppsPage.vue` 的 `onMounted` 中加监听：收到 `auto-restart-giveup` 且 `payload.kind === 'node'` 时重新拉取列表（复用该页既有的加载函数，如 `loadApps()`/`list_node_apps`）；`onUnmounted` 注销。

- [ ] **Step 4: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 5: 提交**

```bash
git add src/App.vue src/modules/node-apps/NodeAppsPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(watchdog): 放弃自动重启时 toast 通知 + Node 列表刷新"
```

---

### Task 7: 全量验证

**Files:** 无（仅运行）

- [ ] **Step 1: 后端全量测试**

Run: `cd src-tauri && cargo test --lib`
Expected: `test result: ok.` 且 0 failed（在既有 123 基础上新增约 7 条）

- [ ] **Step 2: 前端类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无输出

- [ ] **Step 3: 实机走查（推荐）**

`npm run tauri:dev` 启动，然后：
1. 软件管理 → 某软件「启动设置」开启「崩溃后自动重启」→ 启动该软件 → 用任务管理器结束其进程 → ~5–7s 内应自动重启（操作记录页出现 `auto_restart`）
2. 连续 3 次杀掉刚拉起的进程 → 应放弃，状态转 Error，弹出 toast，操作记录出现 `auto_restart_giveup`
3. 手动重新启动该软件 → 失败计数清零（再次杀掉仍会尝试重启，不再立即放弃）
4. Node 应用：其表单开启自动重启后重复步骤 1

- [ ] **Step 4: 提交（若走查有微调）**

```bash
git add -A
git commit -m "chore(watchdog): 实机走查微调"
```

---

## Self-Review

**Spec coverage：**
- 覆盖软件/SpringBoot/Node → Task 3 三个 `watch_*`；模型字段 → Task 1（SpringBoot 复用既有）
- 意外退出判定（仅进程消失、主动停止/退出不误判）→ Task 2 `is_unexpected_exit` + Task 3 状态前置条件
- 轮询 5s / 延迟 2s / 上限 3 / 稳定 60s 重置 → Task 2 常量 + Task 3 循环
- 配置入口（软件启动设置、Node 表单、SpringBoot 既有）→ Task 4 / Task 5
- 可观测（`auto_restart` / `auto_restart_giveup` 审计 + `auto-restart-giveup` 事件 → toast）→ Task 3 / Task 6
- 测试（纯函数单测 + 实机走查）→ Task 2 / Task 7
- 边界（不做假活/级联/退避/持久化）→ 未安排任务，符合 spec

**Placeholder scan：** 无 TBD/TODO；代码步骤均含完整代码。Task 4/6 中对既有文件「按既有类名/加载函数」的说明属定位指引，非占位实现。

**Type consistency：**
- `Action`、`next_action`、`is_unexpected_exit`、`WatchdogState::{new,failures,is_given_up,record_success,record_failure,record_healthy,mark_given_up}` 在 Task 2 定义、Task 3 使用，命名一致。
- `update_startup_settings` 四参版本在 Task 4 后端定义并在同任务前端调用一致；`save_startup_settings` 参数 `autoRestart`（JS camelCase）↔ `auto_restart`（Rust）符合 Tauri 约定。
- 事件名 `auto-restart-giveup`、载荷 `{kind,id,name}` 在 Task 3 定义、Task 6 消费一致。
