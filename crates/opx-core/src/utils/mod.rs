//! 平台无关的通用工具：下载 / 解压 / 路径 / 进程 / 拓扑 / IP 探测等。
//!
//! 原 `update.rs`（应用内更新，走 `tauri-plugin-updater`）因依赖 Tauri，留在壳层
//! `opx::utils::update`。

pub mod archive;
pub mod download;
pub mod http;
pub mod local_ip;
pub mod paths;
pub mod platform;
pub mod process;
pub mod topo;
