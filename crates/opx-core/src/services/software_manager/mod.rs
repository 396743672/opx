//! 软件管理的**基础件**（阶段 3 批次 3A1 起逐步下沉）。
//!
//! # 迁移进度
//!
//! | 子模块 | 状态 | 批次 |
//! |---|---|---|
//! | [`providers`] | ✅ 已搬入| 3A1 |
//! | [`catalog`] | ✅ 已搬入 | 3A1 |
//! | [`log_viewer`] | ✅ 已搬入 | 3A1 |
//! | `SoftwareManager` 本体（[`mod.rs`] 之外） | ⏳ 待搬 | 3A2 |
//! | `audit` / `audit_log` | ⏳ 待搬 | 3A2 |
//! | `health_check` / `process_monitor` / `netutils` | ⏳ 待搬 | 3A2 |
//! | `lifecycle` | ⏳ 待搬 | 3A2（`auto_start_all` 三段留壳层，见下） |
//! | `installer` / `backup` / `backup_scheduler` / `config_editor` / `log_watcher` | ⏳ 待搬 | 3B / 3.5 |
//!
//! ## 为什么 `SoftwareManager` 本体还没搬
//!
//! 它要等3A2，因为本模块树里`mod.rs` 之外的部分仍在壳层：
//! `lifecycle.rs` 的 `auto_start_all` 三段（`auto_start_all` / `spawn_start` /
//! `await_batch_ready`）会调壳层的 `commands::software::do_start_software`，
//! 那三段留壳层（且已确认**无调用方**，见壳层同模块内的注释），
//! `lifecycle.rs` 其余部分才能零反向依赖地搬进来。
//!
//! ## 3A1 的一处解耦：`log_viewer` 不认识 `SoftwareManager`
//!
//! `log_viewer` 原先 3 个函数收 `&SoftwareManager`，而本体 3A2 才搬，
//! 直接搬会形成 core → 壳层反向依赖。故按「按字段切而非按类型切」改为收
//! `&InstalledSoftware` / `&[InstalledSoftware]` 切片 —— 它对 manager 的全部需求
//! 就是 `find_installed()` 与 `get_installed()` 两个读方法。
//!
//! ⚠️ **`install_path` 解析状态必须由调用方保证**：`SoftwareManager::find_installed`
//! 会对 `install_path` 调 `paths::resolve_install_path`（相对路径 → 绝对），
//! 而 `get_installed()` **不做**。壳层各调用点原样传入自己那份求值结果，
//! core 内部不再调任何 manager 方法，故解析状态与搬迁前逐字节一致。
pub mod catalog;
pub mod log_viewer;
pub mod providers;