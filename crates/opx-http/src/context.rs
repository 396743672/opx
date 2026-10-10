//! [`AppContext`] —— 桌面壳与 headless 壳共用的命令胶水上下文（设计 D7-A）。
//!
//! ## 为什么放 opx-http
//!
//! 全部 6 个管理器在阶段 3 已搬入 `opx-core`，`EventSink` 亦是 core 抽象；
//! 本结构只做「Arc 集合 + 事件出口」的聚合，零 tauri 依赖，因此桌面壳
//! （src-tauri）与未来的 headless 壳（opx-server）都能构造同一份上下文，
//! 命令胶水**写一份、两边共享**（方案 A 的核心收益）。
//!
//! ## 字段语义
//!
//! - 六个管理器：与 Tauri `State<'_, Arc<XxxManager>>` 同一实例（lib.rs 装配时
//!   clone 同一 Arc），命令层两种取法等价
//! - [`AppContext::sink`]：宿主装配的事件出口。桌面 + web 关闭 = `FanOut[Tauri]`
//!   （与直用 TauriEventSink 行为逐字节一致）；桌面 + web 开启 = `FanOut[Tauri, Ws]`；
//!   headless = `FanOut[Ws]`
//! - [`AppContext::node_exe`]：Node 可执行文件路径（启动时解析一次，供
//!   watchdog / startup_bootstrap 等使用）

use std::path::PathBuf;
use std::sync::Arc;

use opx_core::event::EventSink;
use opx_core::services::dns_account::DnsAccountManager;
use opx_core::services::node_app_manager::NodeAppManager;
use opx_core::services::software_manager::SoftwareManager;
use opx_core::services::springboot_manager::SpringBootManager;
use opx_core::services::stack_manager::StackManager;
use opx_core::services::website_manager::WebsiteManager;

/// 命令层共享上下文：六管理器 + 事件出口 + Node 路径。
///
/// 以 `State<'_, AppContext>` 形态注入 Tauri 命令，也可在 HTTP 分发器
/// （批次 4.2+）中经 `Arc<AppContext>` 复用——字段全部是可 clone 的句柄。
/// `Clone`（批次 4.5）：字段全为 Arc 句柄，克隆廉价且共享底层实例——壳层
/// 装配点用同一份字段克隆出「plain 管理 + Arc 管理」两种 State 形态。
#[derive(Clone)]
pub struct AppContext {
    /// 软件管理器（安装/启停/卸载/日志等）
    pub software: Arc<SoftwareManager>,
    /// 站点（nginx website）管理器
    pub website: Arc<WebsiteManager>,
    /// SpringBoot 应用管理器
    pub springboot: Arc<SpringBootManager>,
    /// Node 应用管理器
    pub node: Arc<NodeAppManager>,
    /// DNS 账号管理器（ACME DNS-01 依赖）
    pub dns: Arc<DnsAccountManager>,
    /// 一键启动栈管理器
    pub stack: Arc<StackManager>,
    /// 事件出口（宿主装配：桌面 FanOut[Tauri(,Ws)] / headless FanOut[Ws]）
    pub sink: Arc<dyn EventSink>,
    /// Node 可执行文件路径（None = 未检测到，启动 Node 应用时回退在线下载/报错）
    pub node_exe: Option<PathBuf>,
}
