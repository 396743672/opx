pub mod acme;
pub mod autostart;
pub mod ddns;
pub mod dns_account;
pub mod lock_screen;
pub mod software_manager;
pub mod stack_manager;
pub mod startup_bootstrap;
pub mod system_monitor;
pub mod watchdog;

// —— 批次 4B：springboot_manager / node_app_manager 已搬入 core，整模块重导出 ——
// 使 commands/springboot.rs（~15 处全路径）、commands/node_app.rs 及 lib.rs 的
// state 注册与启动点零改动。注意 core 侧 `springboot_manager::lifecycle` 与
// core `software_manager::lifecycle` 并存（不同父模块，合法）；经此重导出转发，
// 壳层调用点写错路径会编译失败而非静默用错实现。
pub use opx_core::services::node_app_manager;
pub use opx_core::services::springboot_manager;
