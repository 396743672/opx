//! 软件进程生命周期管理 —— **壳层残留：仅含三段死代码**。
//!
//! 阶段 3 批次 3A2 把本模块的主体搬进了
//! `opx_core::services::software_manager::lifecycle`（`ProcessRegistry` /
//! `register` / `get` / `drain` / `spawn_process` / `stop_one` /
//! `run_graceful_stop` / `emit_status_changed` / `stop_all_on_exit` 等）。
//!
//! ## ⚠️ 为什么这三段必须留壳层
//!
//! [`auto_start_all`] / [`await_batch_ready`] / [`spawn_start`] **全仓库无调用方**
//! ——真正的软件自启走 `startup_bootstrap::run_bootstrap`，它在
//! `startup_bootstrap.rs:200` 直接调 `commands::software::do_start_software`。
//! 本模块是那次收敛之前的旧实现残留。
//!
//! 留壳层的直接原因：非私有的 [`spawn_start`] 会调壳层的
//! `crate::commands::software::do_start_software`，搬进 `opx-core` 就形成
//! **core → 壳层反向依赖**，违反 workspace 的依赖单向原则。
//!
//! ## 🚨 同名模块的符号歧义（最容易踩的坑）
//!
//! 现在**两个 crate 各有一个 `lifecycle` 模块**：本文件与
//! `opx_core::services::software_manager::lifecycle`。Rust 允许同名，靠路径区分，
//! **编译器不会报错**，所以路径写错不会立刻暴露，而是静默用错实现。
//!
//! 规避手段：**壳层 `lifecycle` 模块对 core 侧符号做 `pub use` 重导出**
//!（见下方「重导出」段）。全仓库 9 个文件、13 处 `lifecycle::X` 引用因此
//! **零改动**，且经由本模块统一转发，**不可能误指到别处**——即使有人写
//! `crate::services::software_manager::lifecycle::register`，拿到的也是 core 的
//! 那个实现（壳层自己那份只剩三段死代码，根本没有 `register`）。
//!
//! 代价是重导出层必须随 core 符号增删同步维护：core 删/改签名时这里会编译报错，
//! 属可接受的显式失败，优于静默分叉。
//!
//! ## 3B 待办
//!
//! [`do_start_software`] 搬进 core 后，本文件 [`spawn_start`] 里的调用路径要相应
//! 改指 core。**但请注意这三段不是活代码**——真正的自启逻辑在
//! `startup_bootstrap`，改动这里不会影响自启行为。清理见另起一轮死代码清理轮次。

use std::sync::Arc;
use std::time::Duration;

// 三段残留不推事件（无 `emit_*` 调用），故只需 `EventSink` 类型本身。
use opx_core::event::EventSink;
use opx_core::models::software::InstalledSoftware;
use opx_core::models::software::SoftwareStatus;

use crate::services::software_manager::SoftwareManager;

// ============================================================================
// 重导出：core 侧 `lifecycle` 的公开符号
// ============================================================================
//
// 3A2 把模块主体搬进 core 后，壳层 9 个文件共 13 处 `lifecycle::X` 引用
// （`commands/app.rs` / `commands/software.rs` / `services/node_app_manager.rs`
// / `services/software_manager/backup.rs` / `services/springboot_manager/lifecycle.rs`
// / `services/stack_manager.rs` / `services/startup_bootstrap.rs` / `services/watchdog.rs`
// / `services/software_manager/mod.rs`）若逐个改成全路径会污染可读性，
// 故在此统一 `pub use` 转发——**调用点保持 `lifecycle::X` 原样不动**。
//
// 这层重导出同时消除了同名歧义：`use crate::services::software_manager::lifecycle;`
// 拿到的就是这一组 re-export，全部指向 core 实现。
//
// 清单依据：`grep -roh 'lifecycle::[a-z_]\+'` 全仓扫描后，逐个核对属主
// （`start_app` / `stop_app` / `is_pid_alive` 属`springboot_manager::lifecycle`，不在此列）。
pub use opx_core::services::software_manager::lifecycle::{
    build_custom_command, emit_status_changed, monitored_pid, register, reset_data_dirs,
    run_first_run_init, run_graceful_stop, spawn_process, stop_all_on_exit, stop_one, unregister,
    validate_stop_transition, wipe_data_dir_if_nonempty,
};

// ============================================================================
// ⚠️ 以下三段（`auto_start_all` / `await_batch_ready` / `spawn_start`）是死代码
// ============================================================================
//
//阶段 3 批次 3A1 确认：**全仓库无外部调用方**。
// `grep -rn "auto_start_all|spawn_start|await_batch_ready"` 的全部命中都在本文件内
// （互相调用），外部只有 `node_app_manager::auto_start_all` 与
// `stack_manager::auto_start_all` 两个**同名但不同类型**的方法，与本段无关。
//
// **真正的软件自启路径**是 `startup_bootstrap::run_bootstrap`：它在
// `startup_bootstrap.rs:200` 直接调用 `commands::software::do_start_software`，
// 与 `lib.rs:112` 注释所述「把软件 / Node 应用 / 服务组 Stack 三路 auto_start
// 收敛为单一有序序列」一致。本段是那次收敛之前的旧实现残留。
//
// ## 为什么留在壳层（3A2 决策：方案 B）
//
// `spawn_start` 内调用壳层的 `crate::commands::software::do_start_software`。
// 若整段搬进 `opx-core`，就会形成 **core → 壳层反向依赖**，违反依赖单向原则。
// 故 3A2 搬 `lifecycle.rs` 其余部分时，把本段整体留下。
//
// ## 3B 搬 `do_start_software` 时注意
//
// 该函数搬进 core 后，本段的调用需改指core 路径（与 `lifecycle.rs` 其余部分
// 拆分后的 `use` 语句一起调整）。**但请注意本段不是活代码**——真正的自启在
// `startup_bootstrap`，改动这里不会影响自启行为。清理见另起的一轮死代码清理。
//
// ============================================================================

/// 应用启动时按 startup_order 拉起 auto_start=true 的实例
///
/// ⚠️ **死代码**：无调用方，实际自启走 `startup_bootstrap::run_bootstrap`。见上方说明。
///
/// 同 startup_order 的实例会被分组并发拉起（不等单个完成，仅 sleep 500ms 间隔）；
/// 不同 startup_order 的批次之间会等待上一批「真正就绪」（status==Running，由后台
/// 健康检查任务在就绪后设置）再拉起下一批，使依赖拓扑在自启场景生效——
/// 旧实现仅 sleep 500ms，不保证依赖方已就绪（下游可能连不上）。
/// 应在 Tauri setup hook 中通过 `tauri::async_runtime::spawn` 调用。
pub async fn auto_start_all(manager: &Arc<SoftwareManager>, sink: &Arc<dyn EventSink>) {
    let auto_list = manager.list_auto_start();
    if auto_list.is_empty() {
        return;
    }
    tracing::info!(count = auto_list.len(), "auto_start_on_boot");

    // 同 startup_order 分组：碰到不同的 order 时先 drain 当前 pending 批次
    let mut last_order: u32 = 0;
    let mut pending: Vec<InstalledSoftware> = Vec::new();

    for sw in auto_list {
        if !pending.is_empty() && sw.startup_order != last_order {
            let ids: Vec<String> = pending.iter().map(|s| s.id.clone()).collect();
            for s in pending.drain(..) {
                spawn_start(manager.clone(), sink.clone(), s.id).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            // 等待上一批全部就绪（或失败/超时）后再进入下一批，依赖拓扑生效
            await_batch_ready(manager, &ids, 60_000).await;
        }
        last_order = sw.startup_order;
        pending.push(sw);
    }
    // 处理剩余批次
    let ids: Vec<String> = pending.iter().map(|s| s.id.clone()).collect();
    for s in pending {
        spawn_start(manager.clone(), sink.clone(), s.id).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    await_batch_ready(manager, &ids, 60_000).await;
}

/// 等待一批自启实例真正就绪：轮询各自 status，直到全部 Running（依赖拓扑生效）、
/// 或任一进入 Error、或超时。status==Running 仅在健康检查通过后由后台任务设置，
/// 故轮询它等价于等待就绪，无需改动 do_start_software 的「提前返回」契约。
///
/// ⚠️ **死代码**：仅被 `auto_start_all` 调用，而后者无调用方。见上方说明。
async fn await_batch_ready(manager: &Arc<SoftwareManager>, ids: &[String], timeout_ms: u64) {
    let start = std::time::Instant::now();
    let deadline = std::time::Duration::from_millis(timeout_ms);
    loop {
        let all_ready = ids.iter().all(|id| {
            manager
                .find_installed(id)
                .map(|s| s.status == SoftwareStatus::Running)
                .unwrap_or(false)
        });
        if all_ready {
            return;
        }
        let any_err = ids.iter().any(|id| {
            manager
                .find_installed(id)
                .map(|s| s.status == SoftwareStatus::Error)
                .unwrap_or(false)
        });
        if any_err {
            tracing::warn!(ids = ?ids, "auto_start 依赖实例进入 Error，跳过等待继续");
            return;
        }
        if start.elapsed() >= deadline {
            tracing::warn!(ids = ?ids, "auto_start 等待批量就绪超时");
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// spawn 单个 auto_start 任务（fire-and-forget）
/// 不等待 do_start_software 完成，避免单个慢启动阻塞后续实例
///
/// ⚠️ **死代码**：仅被 `auto_start_all` 调用，而后者无调用方。见上方说明。
/// 3B 搬 `do_start_software` 进 core 后，本函数体里的调用需改指 core 路径。
async fn spawn_start(
    manager: Arc<SoftwareManager>,
    sink: Arc<dyn EventSink>,
    installed_id: String,
) {
    let manager_clone = manager.clone();
    // spawn 边界需要 'static：先 clone 出owned 的 sink 再 move 进去，
    // 直接捕获外层 `&Arc` 会报 `sink escapes function body`。
    let sink_clone = sink.clone();
    let id_clone = installed_id.clone();
    tokio::spawn(async move {
        let result =
            crate::commands::software::do_start_software(&manager_clone, &sink_clone, &id_clone, None)
                .await;
        if let Err(e) = result {
            tracing::error!(error = %e, installed_id = %id_clone, "auto_start failed");
        }
    });
}
