//! B 扩展（一键启动栈 Stack）数据模型
//!
//! 栈（Stack）是跨模块（已装软件 `InstalledSoftware` / Spring Boot 应用 `SpringBootApp`）
//! 的统一编排抽象：用户可定义一组关联成员、显式依赖与启动顺序，一键整体启动 / 停止 / 重启。
//!
//! 所有结构均派生 `serde::{Serialize, Deserialize}`，与前端类型（`src/models/stack.ts`）一一对应。
//! 运行态结构（`StackMemberStatus` / `StackMemberRuntime` / `StackStatusEvent`）不参与持久化，
//! 仅用于命令返回与 `stack-status-changed` 事件推送。

use serde::{Deserialize, Serialize};

/// 栈成员引用的目标类型：
/// - `Software`：引用 `InstalledSoftware.id`
/// - `Springboot`：引用 `SpringBootApp.id`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StackItemRefType {
    Software,
    Springboot,
}

/// 栈内单个成员（引用某个已装软件或 Spring Boot 应用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackItem {
    /// 引用类型（software / springboot）
    pub ref_type: StackItemRefType,
    /// 栈内唯一引用键，同时即被引用实体的 id（InstalledSoftware.id / SpringBootApp.id）
    pub ref_id: String,
    /// 同层 tie-break（升序），仅在依赖拓扑同层内生效
    pub order: u32,
    /// 依赖的其它成员 `ref_id`（栈内引用）。依赖拓扑序优先于 `order`。
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// 是否纳入编排（默认 true）
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 启动失败重试次数（R10，默认 0 表示不重试）
    #[serde(default)]
    pub retry: u32,
}

fn default_true() -> bool {
    true
}

/// 一个栈（命名、可保存、可一键执行的跨模块服务集合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub items: Vec<StackItem>,
    /// ISO 8601 UTC
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// 上次启动由栈拉起的组外依赖（ref_id）。仅内存态运行时记录，用于停止时一并停止
    /// 本次拉起的依赖；持久化到 stacks.json 以在应用重启后仍能正确停止。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_externals: Option<Vec<String>>,
}

/// 成员运行态（不持久化）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StackMemberStatus {
    Pending,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// 单个成员的运行态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackMemberRuntime {
    pub ref_id: String,
    pub status: StackMemberStatus,
    #[serde(default)]
    pub message: String,
}

/// 启动执行计划（Kahn 拓扑分层结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackStartPlan {
    pub stack_id: String,
    /// 逐批顺序：layers[0] 为第一批，批内成员可并发
    pub layers: Vec<Vec<String>>,
    /// 检测到环时返回环路径
    pub cycle: Option<Vec<String>>,
}

/// 创建栈请求载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStackPayload {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub items: Vec<StackItem>,
}

/// 更新栈请求载荷（所有字段可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStackPayload {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub items: Option<Vec<StackItem>>,
}

/// `stack-status-changed` 事件载荷（栈级状态聚合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackStatusEvent {
    pub stack_id: String,
    /// 整体栈状态（Running / Failed / Starting / Stopped ...）
    pub status: StackMemberStatus,
    pub members: Vec<StackMemberRuntime>,
}
