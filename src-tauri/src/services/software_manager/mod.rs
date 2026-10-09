//! 软件管理服务 —— **壳层残留**（阶段 3 批次 3A2 之后）。
//!
//! 主体已下沉到 `opx_core::services::software_manager`，本模块只声明**尚未下沉**的子模块，
//! 并把 core 的常用类型 `pub use` 重导出，使壳层调用点可用短路径。
//!
//! ## 迁移进度
//!
//! | 子模块 | 状态 | 批次 |
//! |---|---|---|
//! | `SoftwareManager` 本体 / `providers` / `catalog` / `log_viewer` | ✅ 在 core | 3A1 / 3A2 |
//! | `audit` / `audit_log` / `health_check` / `process_monitor` / `netutils` / `lifecycle` | ✅ 在 core | 3A2 |
//! | `installer` / `backup` / `backup_scheduler` / `config_editor` / `log_watcher` | ⏳ 仍在壳层 | 3B / 3.5 |
//!
//! ## 🚨 `lifecycle.rs` 是拆分搬迁的产物
//!
//! 本目录的 [`lifecycle`] **只含三段死代码**（`auto_start_all` /
//! `await_batch_ready` / `spawn_start`，全仓库无调用方，真正的自启走
//! `startup_bootstrap::run_bootstrap`）。主体（`register` / `get` / `stop_one` /
//! `run_graceful_stop` / `stop_all_on_exit` 等）已搬进
//! `opx_core::services::software_manager::lifecycle`。详见本文件内 [`lifecycle`] 的文件头。
//!
//! ## 🚨 同名模块的符号歧义
//!
//! 壳层与 core **各有一个 `services::software_manager` 模块**。Rust 允许同名，
//! **编译器不会报错**，路径写错会静默用错实现。故壳层一律用**全路径**
//! `opx_core::services::software_manager::X` 访问 core 侧符号。

pub mod backup;
pub mod backup_scheduler;
pub mod config_editor;
pub mod installer;
pub mod lifecycle;
pub mod log_watcher;

// —— core 侧符号的重导出：让壳层可写 `services::software_manager::SoftwareManager` ——
// SoftwareManager 本体在 core，但壳层 13 个文件的引用点遍布各服务；重导出可免去
// 逐处改路径，也与前几批的薄封装模式一致。
//
// 注：`lifecycle::stop_all_on_exit` **不**在此重导出——它已由 [`lifecycle`] 模块
// 自身转出（见该文件的重导出段）。两处都写会形成两条等价路径，徒增歧义。
pub use opx_core::services::software_manager::audit;
pub use opx_core::services::software_manager::audit_log;
pub use opx_core::services::software_manager::health_check;
pub use opx_core::services::software_manager::netutils;
pub use opx_core::services::software_manager::process_monitor;
pub use opx_core::services::software_manager::uninstall_guard;
pub use opx_core::services::software_manager::{InstallTaskState, SoftwareManager};