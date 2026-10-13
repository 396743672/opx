//! 启动编排报告模型（扩展 4）。
//!
//! 与应用启动时的统一启动编排（`services/startup_bootstrap.rs`）配套：
//! 记录本次启动逐项的结果与耗时，持久化后供前端「最近一次启动报告」卡片回显。

use serde::{Deserialize, Serialize};

/// 启动项类型：软件（InstalledSoftware）
pub const KIND_SOFTWARE: &str = "software";
/// 启动项类型：Node 应用
pub const KIND_NODE: &str = "node";
/// 启动项类型：服务组（Stack）
pub const KIND_STACK: &str = "stack";

/// 一个待启动目标
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupTarget {
    pub kind: String,
    pub id: String,
    pub name: String,
}

/// 单项结果状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StartupItemStatus {
    /// 已成功拉起
    Ok,
    /// 拉起失败（触发回滚）
    Failed,
    /// 因前序失败回滚而未执行
    Skipped,
}

/// 单项报告
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartupItemReport {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub status: StartupItemStatus,
    pub elapsed_ms: u64,
    /// 失败原因（成功/跳过为空串）
    #[serde(default)]
    pub message: String,
}

/// 一次启动编排的整体报告
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StartupReport {
    /// 编排开始时间（RFC3339，本地时区）
    #[serde(default)]
    pub started_at: String,
    /// 整体耗时（含回滚）
    #[serde(default)]
    pub total_elapsed_ms: u64,
    /// 逐项结果（按执行顺序；含被跳过的项）
    #[serde(default)]
    pub items: Vec<StartupItemReport>,
    /// 是否发生了失败回滚（本次已拉起项被逆序停止）
    #[serde(default)]
    pub rolled_back: bool,
}
