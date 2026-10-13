//! 软件进程生命周期管理 —— **壳层残留：仅含 core 符号的重导出**。
//!
//! 阶段 3 批次 3A2 把本模块的主体搬进了
//! `opx_core::services::software_manager::lifecycle`（`ProcessRegistry` /
//! `register` / `get` / `drain` / `spawn_process` / `stop_one` /
//! `run_graceful_stop` / `emit_status_changed` / `stop_all_on_exit` 等）。
//! 本文件只剩一行 `pub use`（见下方），壳层调用点经此转发到 core 实现。
//!
//! ## 历史：三段死代码已在本轮清理删除
//!
//! 本文件曾含 `auto_start_all` / `await_batch_ready` / `spawn_start` 三段
//! 死代码（全仓库无调用方，真正的自启走 `startup_bootstrap::run_bootstrap`）。
//! 它们留壳层的硬理由（`spawn_start` 调壳层 `do_start_software` 会形成
//! core → 壳层反向依赖）随批次 3B 搬迁 `do_start_software` 进 core 而解除，
//! 已于死代码清理轮次删除（git 历史可查），无行为变更。
//!
//! ## 🚨 同名模块的符号歧义
//!
//! 现在**两个 crate 各有一个 `lifecycle` 模块**：本文件（纯转发）与
//! `opx_core::services::software_manager::lifecycle`（实现）。Rust 允许同名，
//! 靠路径区分。规避手段：壳层调用点一律经本文件的重导出（或直接写 core
//! 全路径）；本文件**没有任何自有符号**，写错路径不可能静默用错实现——
//! 指向不存在的壳层符号会直接编译失败。

// ============================================================================
// 重导出：core 侧 `lifecycle` 的公开符号
// ============================================================================
//
// 3A2 把模块主体搬进 core 后，壳层调用点（`commands/app.rs` /
// `commands/software.rs` / `services/node_app_manager.rs` /
// `services/software_manager/backup.rs` 等）保持 `lifecycle::X` 原样不动，
// 故在此统一 `pub use` 转发，全部指向 core 实现。
//
// 清单依据：`grep -roh 'lifecycle::[a-z_]\+'` 全仓扫描后，逐个核对属主
// （`start_app` / `stop_app` / `is_pid_alive` 属 `springboot_manager::lifecycle`，不在此列）。
pub use opx_core::services::software_manager::lifecycle::{
    build_custom_command, emit_status_changed, monitored_pid, register, reset_data_dirs,
    run_first_run_init, run_graceful_stop, spawn_process, stop_all_on_exit, stop_one, unregister,
    validate_stop_transition, wipe_data_dir_if_nonempty,
};
