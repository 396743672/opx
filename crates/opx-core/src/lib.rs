//! OPX 平台无关核心。
//!
//! 依据 `docs/2026-10-09-opx-web-architecture.md`（ADR，形态 A：单进程双入口）拆分：
//! 本 crate 只承载**不依赖 Tauri** 的业务逻辑，供两个壳共用——
//! - `opx`：Tauri 桌面壳（Windows / macOS 原生窗口）
//! - `opx-server`：headless 壳（Linux 无桌面场景，浏览器 web 管理）
//!
//! **依赖方向单向：`壳 → opx-core`。** 本 crate 不得依赖 `tauri` / `tauri-plugin-*`，
//! 也不得反向依赖壳层（否则 headless 壳无法在无 GUI 环境下构建）。
//!
//! 迁移进度（阶段 1 逐层搬迁，每层保持 Windows 行为不变）：
//! - [x] `models` —— 数据模型（仅依赖 serde / serde_json / chrono）
//! - [x] `utils` —— 通用工具（下载 / 解压 / 路径 / 进程 / 拓扑 / IP 探测；
//!   `utils::update` 因依赖 `tauri-plugin-updater`，留在壳层）

pub mod models;
pub mod utils;
