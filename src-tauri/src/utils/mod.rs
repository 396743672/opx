//! 壳层专用的工具模块（**依赖 Tauri**）。
//!
//! 通用工具（下载 / 解压 / 路径 / 进程 / 拓扑 / IP 探测）已随阶段 1 迁至
//! `opx_core::utils`；此处只保留必须经 Tauri 插件的能力。

pub mod update;
