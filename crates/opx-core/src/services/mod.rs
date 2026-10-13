//! 站点管理：站点列表持久化 + nginx 配置生成与重建。
//!
//! 原在壳层 `opx::services::website_manager`，随阶段 3 搬迁进 core：
//! - [`website_manager`] —— `WebsiteManager`（`websites.json` 的读写，纯文件 IO）
//! - [`nginx_conf`] —— nginx 配置文本生成（纯字符串拼接）
//! - [`regenerate`] —— 把站点列表同步成 `conf/sites/*.conf` 并可选 reload
//!
//! 三者均零 tauri 依赖，headless 壳同样需要（改站点列表 → 重建 nginx 配置）。
//!
//! 另见 [`crate::utils::website`]：`resolve_nginx` / `sanitize_domain` 目前临时寄居
//! 在 `utils`（它们的下沉早于本模块迁入），待本模块归位后可一并收拢。
//!
//! [`software_manager`] —— 软件管理的基础件（provider 目录、软件目录 catalog、日志读取）。
//!
//! [`node_app_manager`] / [`springboot_manager`] —— 应用运行时管理（Node 应用、
//! Spring Boot JAR），批次 4B 自壳层迁入。注意 core 内同时存在
//! [`crate::services::software_manager::lifecycle`] 与
//! [`springboot_manager::lifecycle`]：不同父模块，合法并存，靠全路径区分。
//!
//! 阶段 1.3 收尾（2026-10-13）：以下模块自壳层迁入，壳层 services 只剩
//! `web_server`（tauri AppHandle 耦合，搬不动）与 `software_manager` 薄余留
//! （re-export 转发层 + `log_watcher` 壳层职责）：
//! - [`autostart`] / [`lock_screen`] —— 开机自启（三平台）与锁屏（凭据库存 verifier）；
//! - [`system_monitor`] / [`watchdog`] / [`startup_bootstrap`] —— 监控告警、
//!   崩溃自愈、启动编排。三者的前置是 [`crate::event::sinks`] 下沉：
//!   `WsEventSink`/`FanOutSink` 自 opx-http 迁入 core（零 axum 耦合），
//!   解除「事件产出方反向依赖 opx-http」的死锁。

pub mod acme;
pub mod autostart;
pub mod ddns;
pub mod dns_account;
pub mod lock_screen;
pub mod node_app_manager;
pub mod software_manager;
pub mod springboot_manager;
pub mod stack_manager;
pub mod startup_bootstrap;
pub mod system_monitor;
pub mod watchdog;
pub mod website_manager;