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

pub mod software_manager;
pub mod website_manager;