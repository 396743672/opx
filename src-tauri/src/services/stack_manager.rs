//! B 扩展（一键启动栈 Stack）服务层
//!
//! `StackManager` 负责：
//! 1. 栈的持久化（`data_dir/stacks.json`，与 `installed.json` / `apps.json` 解耦）；
//! 2. 拓扑排序与环检测（`build_plan` / `compute_plan`，Kahn 算法）；
//! 3. 一键启动 / 停止 / 重启编排（逐批 + 批内并发 + 依赖就绪探测 + 重试 + 回滚）；
//! 4. 栈级状态聚合，emit `stack-status-changed` 事件；
//! 5. 栈的导出 / 导入（R7）。
//!
//! 复用现有能力，不重复造轮子：
//! - 启动 software 成员：`crate::commands::software::do_start_software`（已 `pub`）
//! - 启动 springboot 成员：`crate::services::springboot_manager::lifecycle::start_app`（已 `pub`）
//! - 停止 software 成员：`crate::services::software_manager::lifecycle::stop_one` 等（已 `pub`）
//! - 停止 springboot 成员：`crate::services::springboot_manager::lifecycle::stop_app`
//! - 就绪判定：`health_check::is_process_alive` + `is_port_free`

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use futures::future;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::commands::software as sw_commands;
use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::models::stack::{
    CreateStackPayload, Stack, StackItem, StackItemRefType, StackMemberReport, StackMemberRuntime,
    StackMemberStatus, StackRunReport, StackStartPlan, StackStatusEvent, UpdateStackPayload,
};
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::lifecycle as sb_lifecycle;
use crate::services::springboot_manager::SpringBootManager;
use crate::utils::paths;

/// 成员启动后等待就绪的超时上限（毫秒）。
/// software 的 `do_start_software` 内部健康检查最多 60s，这里给足余量。
const READY_TIMEOUT_MS: u64 = 65_000;
/// 就绪轮询间隔（毫秒）
const READY_POLL_MS: u64 = 500;

/// 启动编排管理器（包裹两个已存在的 Manager 的 Arc）
pub struct StackManager {
    software_mgr: Arc<SoftwareManager>,
    springboot_mgr: Arc<SpringBootManager>,
    inner: Mutex<StackManagerInner>,
}

struct StackManagerInner {
    stacks: Vec<Stack>,
    data_path: PathBuf,
}

impl StackManagerInner {
    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.data_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = serde_json::to_string_pretty(&self.stacks)
            .map_err(|e| format!("序列化栈数据失败: {}", e))?;
        std::fs::write(&self.data_path, content)
            .map_err(|e| format!("写入 {} 失败: {}", self.data_path.display(), e))?;
        Ok(())
    }
}

impl StackManager {
    /// 生产构造：使用 `data_dir/stacks.json` 作为持久化文件
    pub fn new(software_mgr: Arc<SoftwareManager>, springboot_mgr: Arc<SpringBootManager>) -> Self {
        let data_path = Self::default_data_path();
        let stacks = Self::read_stacks(&data_path);
        Self {
            software_mgr,
            springboot_mgr,
            inner: Mutex::new(StackManagerInner { stacks, data_path }),
        }
    }

    /// 测试构造：使用自定义持久化路径（如 `std::env::temp_dir()` 下的临时文件）
    #[cfg(test)]
    pub fn new_with_path(
        software_mgr: Arc<SoftwareManager>,
        springboot_mgr: Arc<SpringBootManager>,
        data_path: PathBuf,
    ) -> Self {
        let stacks = Self::read_stacks(&data_path);
        Self {
            software_mgr,
            springboot_mgr,
            inner: Mutex::new(StackManagerInner { stacks, data_path }),
        }
    }

    fn default_data_path() -> PathBuf {
        paths::data_dir().join("stacks.json")
    }

    fn read_stacks(path: &PathBuf) -> Vec<Stack> {
        if !path.exists() {
            return Vec::new();
        }
        match std::fs::read_to_string(path) {
            Ok(c) => match serde_json::from_str::<Vec<Stack>>(&c) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[stack] 解析 stacks.json 失败（已回退为空）: {}", e);
                    Vec::new()
                }
            },
            Err(e) => {
                eprintln!("[stack] 读取 stacks.json 失败（已回退为空）: {}", e);
                Vec::new()
            }
        }
    }

    // ------------------------- CRUD -------------------------

    pub fn list(&self) -> Vec<Stack> {
        self.inner.lock().unwrap().stacks.clone()
    }

    pub fn get(&self, id: &str) -> Option<Stack> {
        self.inner
            .lock()
            .unwrap()
            .stacks
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn create(&self, payload: CreateStackPayload) -> Result<Stack, String> {
        let name = payload.name.trim().to_string();
        if name.is_empty() {
            return Err("栈名称不能为空".to_string());
        }
        let stack = Stack {
            id: Uuid::new_v4().to_string(),
            name,
            description: payload.description,
            items: payload.items,
            created_at: now_utc(),
            updated_at: String::new(),
            managed_externals: None,
            auto_start: payload.auto_start,
            last_run_report: None,
        };
        // 保存前环检测：存在环则拒绝写入
        let _ = Self::compute_plan(&stack)?;

        {
            let mut inner = self.inner.lock().unwrap();
            inner.stacks.push(stack.clone());
            inner.save().map_err(|e| e.to_string())?;
        }
        Ok(stack)
    }

    pub fn update(&self, id: &str, payload: UpdateStackPayload) -> Result<Stack, String> {
        let mut inner = self.inner.lock().unwrap();
        let stack = inner
            .stacks
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;

        // 先基于克隆（候选）应用变更并做环检测，保证失败原子性：
        // 环检测失败时直接返回，内存状态与磁盘均不被污染。
        let mut candidate = stack.clone();
        if let Some(name) = payload.name {
            let n = name.trim().to_string();
            if n.is_empty() {
                return Err("栈名称不能为空".to_string());
            }
            candidate.name = n;
        }
        if let Some(desc) = payload.description {
            candidate.description = desc;
        }
        if let Some(items) = payload.items {
            candidate.items = items;
        }
        if let Some(auto_start) = payload.auto_start {
            candidate.auto_start = auto_start;
        }
        candidate.updated_at = now_utc();

        // 保存前环检测（作用于候选，失败即返回，不触碰内存/磁盘）
        let _ = Self::compute_plan(&candidate)?;

        // 校验通过：原子地应用到内存并落盘
        *stack = candidate;
        let cloned = stack.clone();
        inner.save().map_err(|e| e.to_string())?;
        Ok(cloned)
    }

    /// 应用启动时按启动顺序自动拉起启用自启的服务组（逐个，不并发，单组失败不阻塞后续）
    pub async fn auto_start_all(&self, app: &AppHandle) {
        let stacks = self.list();
        let mut eligible: Vec<&Stack> = stacks.iter().filter(|s| s.auto_start).collect();
        eligible.sort_by_key(|s| s.updated_at.clone());
        for stack in eligible {
            if let Err(e) = self.start(app, &stack.id).await {
                tracing::warn!(stack_id = %stack.id, error = %e, "服务组自启失败");
            }
        }
    }

    /// 记录某栈本次拉起的组外依赖清单（持久化到 stacks.json，供应用重启后停止时使用）
    fn set_managed_externals(&self, id: &str, externals: Vec<String>) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        let stack = inner
            .stacks
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;
        if externals.is_empty() {
            stack.managed_externals = None;
        } else {
            stack.managed_externals = Some(externals);
        }
        inner.save().map_err(|e| e.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<bool, String> {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.stacks.len();
        inner.stacks.retain(|s| s.id != id);
        if inner.stacks.len() == before {
            return Ok(false);
        }
        inner.save().map_err(|e| e.to_string())?;
        Ok(true)
    }

    // ------------------------- 拓扑排序 + 环检测 -------------------------

    /// 对已保存的栈计算启动计划（无环 -> Ok，有环 -> Err(环路径)）
    pub fn build_plan(&self, id: &str) -> Result<StackStartPlan, String> {
        let stack = self.get(id).ok_or_else(|| format!("未找到栈: {}", id))?;
        Self::compute_plan(&stack)
    }

    /// 纯函数：对任意 `Stack` 计算拓扑分层计划。
    /// 设计 §6 #3：以 `depends_on` 拓扑序为主，`order` 仅作同层 tie-break。
    /// 存在环时返回明确环路径错误。
    pub fn compute_plan(stack: &Stack) -> Result<StackStartPlan, String> {
        let nodes: Vec<&StackItem> = stack.items.iter().filter(|i| i.enabled).collect();
        let node_ids: HashSet<String> = nodes.iter().map(|i| i.ref_id.clone()).collect();

        let mut indeg: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for n in &nodes {
            indeg.entry(n.ref_id.clone()).or_insert(0);
            adj.entry(n.ref_id.clone()).or_insert_with(Vec::new);
        }
        for n in &nodes {
            for dep in &n.depends_on {
                if node_ids.contains(dep) {
                    *indeg.get_mut(&n.ref_id).unwrap() += 1;
                    adj.get_mut(dep).unwrap().push(n.ref_id.clone());
                }
            }
        }

        // 初始队列：indeg == 0，按 order 升序
        let mut queue: Vec<&StackItem> = nodes
            .iter()
            .filter(|n| indeg[&n.ref_id] == 0)
            .cloned()
            .collect();
        queue.sort_by_key(|n| n.order);

        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut visited: usize = 0;

        while !queue.is_empty() {
            let layer_ids: Vec<String> = queue.iter().map(|n| n.ref_id.clone()).collect();
            layers.push(layer_ids);

            let mut next: Vec<&StackItem> = Vec::new();
            for n in &queue {
                if let Some(deps) = adj.get(&n.ref_id) {
                    for m_id in deps {
                        let e = indeg.get_mut(m_id).unwrap();
                        *e -= 1;
                        if *e == 0 {
                            let m = nodes.iter().find(|x| x.ref_id == *m_id).unwrap();
                            next.push(m);
                        }
                    }
                }
            }
            next.sort_by_key(|n| n.order);
            visited += queue.len();
            queue = next;
        }

        if visited != nodes.len() {
            let cycle = find_cycle(&nodes, &node_ids);
            return Err(format!(
                "检测到循环依赖，栈无法启动: {}",
                cycle.join(" -> ")
            ));
        }

        Ok(StackStartPlan {
            stack_id: stack.id.clone(),
            layers,
            cycle: None,
        })
    }

    // ------------------------- 启动 / 停止 / 重启 -------------------------

    /// 聚合启动报告（纯函数，便于单测）
    pub fn build_run_report(
        started_at: String,
        total_elapsed_ms: u64,
        members: Vec<(String, StackMemberStatus, u64, String)>,
    ) -> StackRunReport {
        StackRunReport {
            started_at,
            total_elapsed_ms,
            members: members
                .into_iter()
                .map(|(ref_id, status, elapsed_ms, message)| StackMemberReport {
                    ref_id,
                    status,
                    elapsed_ms,
                    message,
                })
                .collect(),
        }
    }

    /// 把本次启动结果写回栈记录并持久化（启动成功/失败回滚路径统一调用）
    fn record_run_report(
        &self,
        stack_id: &str,
        started_at: String,
        total_elapsed_ms: u64,
        member_status: &HashMap<String, StackMemberRuntime>,
        t0: &HashMap<String, Instant>,
    ) {
        let members = member_status
            .iter()
            .map(|(ref_id, m)| {
                let elapsed_ms = t0
                    .get(ref_id)
                    .map(|i| i.elapsed().as_millis() as u64)
                    .unwrap_or(0);
                (ref_id.clone(), m.status, elapsed_ms, m.message.clone())
            })
            .collect();
        let report = Self::build_run_report(started_at, total_elapsed_ms, members);
        let mut inner = self.inner.lock().unwrap();
        if let Some(s) = inner.stacks.iter_mut().find(|s| s.id == stack_id) {
            s.last_run_report = Some(report);
        }
        let _ = inner.save();
    }

    /// 一键启动：逐批（layers 顺序）启动，批内并发；依赖就绪探测 + 重试 + 回滚。
    pub async fn start(&self, app: &AppHandle, id: &str) -> Result<StackStartPlan, String> {
        let stack = self.get(id).ok_or_else(|| format!("未找到栈: {}", id))?;

        // 运行前再次拓扑排序（双重保险）
        let plan = Self::compute_plan(&stack)?;

        // 启动报告埋点：起始时刻 + 每成员 t0（启动发起时刻）
        let started_at = Utc::now().to_rfc3339();
        let start_instant = Instant::now();
        let mut t0: HashMap<String, Instant> = HashMap::new();

        // 初始化成员运行态
        let mut member_status: HashMap<String, StackMemberRuntime> = HashMap::new();
        for it in &stack.items {
            member_status.insert(
                it.ref_id.clone(),
                StackMemberRuntime {
                    ref_id: it.ref_id.clone(),
                    status: StackMemberStatus::Pending,
                    message: String::new(),
                },
            );
        }
        self.emit(
            app,
            &stack.id,
            StackMemberStatus::Starting,
            member_status.values().cloned().collect(),
        );

        // 记录本次编排前已在运行的成员，回滚时将其排除，避免误杀预运行服务（P2 回滚过杀防护）
        let pre_running: HashSet<String> = stack
            .items
            .iter()
            .filter(|it| self.is_member_running(it))
            .map(|it| it.ref_id.clone())
            .collect();

        let mut started: Vec<StackItem> = Vec::new();

        // 外部依赖（enabled 成员对组外已装软件的依赖，如 nacos 依赖未入组的 mysql）：
        // 先于组内启动，保证依赖方就绪探测通过；启动前已在运行的不纳入组管理（停止时不误杀）。
        let external_deps = self.collect_external_deps(&stack);
        let mut managed_external: Vec<String> = Vec::new();
        if !external_deps.is_empty() {
            for dep in &external_deps {
                t0.insert(dep.clone(), Instant::now());
            }
            for dep in &external_deps {
                member_status.insert(
                    dep.clone(),
                    StackMemberRuntime {
                        ref_id: dep.clone(),
                        status: StackMemberStatus::Pending,
                        message: String::new(),
                    },
                );
            }
            self.emit(
                app,
                &stack.id,
                StackMemberStatus::Starting,
                member_status.values().cloned().collect(),
            );
            let was_running: HashMap<String, bool> = external_deps
                .iter()
                .map(|d| (d.clone(), self.ref_running(d)))
                .collect();
            let results =
                future::join_all(external_deps.iter().map(|d| self.start_external(app, d))).await;
            let mut ext_err: Option<String> = None;
            for (dep, res) in external_deps.iter().zip(results.into_iter()) {
                match res {
                    Ok(()) => {
                        if let Some(m) = member_status.get_mut(dep) {
                            m.status = StackMemberStatus::Running;
                        }
                        if !was_running.get(dep).copied().unwrap_or(false) {
                            managed_external.push(dep.clone());
                        }
                    }
                    Err(e) => {
                        ext_err = Some(format!("{}: {}", dep, e));
                        if let Some(m) = member_status.get_mut(dep) {
                            m.status = StackMemberStatus::Failed;
                            m.message = e.clone();
                        }
                    }
                }
            }
            if let Some(err) = ext_err {
                for dep in managed_external.iter().rev() {
                    self.stop_external(app, dep).await;
                    if let Some(m) = member_status.get_mut(dep) {
                        m.status = StackMemberStatus::Stopped;
                    }
                }
                self.emit(
                    app,
                    &stack.id,
                    StackMemberStatus::Failed,
                    member_status.values().cloned().collect(),
                );
                self.record_run_report(
                    &stack.id,
                    started_at.clone(),
                    start_instant.elapsed().as_millis() as u64,
                    &member_status,
                    &t0,
                );
                return Err(format!("外部依赖启动失败，已回滚本次拉起的依赖: {}", err));
            }
        }

        for layer in &plan.layers {
            // 批内并发启动
            let mut futs = Vec::new();
            for ref_id in layer {
                t0.insert(ref_id.clone(), Instant::now());
                let item = stack
                    .items
                    .iter()
                    .find(|it| &it.ref_id == ref_id)
                    .cloned()
                    .unwrap();
                futs.push(self.start_one(app, &stack, item));
            }
            let results = future::join_all(futs).await;

            let mut blocking_failure = false;
            let mut failure_msgs: Vec<String> = Vec::new();

            for (ref_id, res) in layer.iter().zip(results.into_iter()) {
                match res {
                    Ok(()) => {
                        // 仅将“本次编排启动”的成员纳入回滚集合；预运行成员不计入，避免误杀
                        if !pre_running.contains(ref_id) {
                            started.push(
                                stack
                                    .items
                                    .iter()
                                    .find(|it| &it.ref_id == ref_id)
                                    .cloned()
                                    .unwrap(),
                            );
                        }
                        if let Some(m) = member_status.get_mut(ref_id) {
                            m.status = StackMemberStatus::Running;
                            m.message = String::new();
                        }
                    }
                    Err(e) => {
                        // 是否阻断整栈：本成员是否被其它成员依赖（依赖其作为上游）
                        let is_blocking = stack.items.iter().any(|it| {
                            it.enabled && &it.ref_id != ref_id && it.depends_on.contains(ref_id)
                        });
                        if is_blocking {
                            blocking_failure = true;
                        }
                        if let Some(m) = member_status.get_mut(ref_id) {
                            m.status = StackMemberStatus::Failed;
                            m.message = e.clone();
                        }
                        failure_msgs.push(format!("{}: {}", ref_id, e));
                    }
                }
            }

            if blocking_failure {
                // R9 回滚：逆序停止本次已启动的成员（仅本栈启动的）
                for item in started.iter().rev() {
                    self.stop_one(app, item).await;
                    if let Some(m) = member_status.get_mut(&item.ref_id) {
                        m.status = StackMemberStatus::Stopped;
                    }
                }
                // 组拉起的组外依赖一并回滚
                for dep in managed_external.iter().rev() {
                    self.stop_external(app, dep).await;
                    if let Some(m) = member_status.get_mut(dep) {
                        m.status = StackMemberStatus::Stopped;
                    }
                }
                let _ = self.set_managed_externals(&stack.id, vec![]);
                self.emit(
                    app,
                    &stack.id,
                    StackMemberStatus::Failed,
                    member_status.values().cloned().collect(),
                );
                self.record_run_report(
                    &stack.id,
                    started_at.clone(),
                    start_instant.elapsed().as_millis() as u64,
                    &member_status,
                    &t0,
                );
                return Err(format!(
                    "栈启动失败，已回滚本次已启动成员: {}",
                    failure_msgs.join("; ")
                ));
            }
        }

        self.record_run_report(
            &stack.id,
            started_at.clone(),
            start_instant.elapsed().as_millis() as u64,
            &member_status,
            &t0,
        );
        self.emit(
            app,
            &stack.id,
            StackMemberStatus::Running,
            member_status.values().cloned().collect(),
        );
        self.set_managed_externals(&stack.id, managed_external)?;
        Ok(plan)
    }

    /// 启动单个成员：
    /// 1. R9 依赖就绪探测（依赖成员须处于 Running）；
    /// 2. 依 `ref_type` 调 `do_start_software` / `start_app`（已运行则跳过）；
    /// 3. 轮询就绪（进程存活 + 端口监听），超时上限 `READY_TIMEOUT_MS`；
    /// 4. 失败按 `retry` 重试（R10）。
    async fn start_one(
        &self,
        app: &AppHandle,
        stack: &Stack,
        item: StackItem,
    ) -> Result<(), String> {
        // R9：依赖成员必须已就绪（Running）。与 compute_plan 的拓扑语义保持一致：
        // 被禁用（enabled=false）的依赖成员不参与编排，直接跳过其就绪检查；
        // 组外依赖（外部依赖）已在 start 前置启动，此处校验其对应服务确为 Running。
        for dep_ref in &item.depends_on {
            let dep_item = stack.items.iter().find(|it| &it.ref_id == dep_ref);
            if let Some(di) = dep_item {
                if !di.enabled {
                    continue;
                }
                if !self.is_member_running(di) {
                    return Err(format!("依赖成员 {} 未就绪（需先成功启动）", dep_ref));
                }
            } else if !self.ref_running(dep_ref) {
                return Err(format!("外部依赖 {} 未就绪", dep_ref));
            }
        }

        let max_attempts = 1 + item.retry;
        let mut last_err: Option<String> = None;
        for attempt in 0..max_attempts {
            match self.start_once(app, &item).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                    if attempt + 1 < max_attempts {
                        tracing::warn!(
                            ref_id = %item.ref_id,
                            attempt = attempt + 1,
                            max = max_attempts,
                            "start_one 重试中"
                        );
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| "启动失败（未知原因）".to_string()))
    }

    /// 实际执行一次启动（不含重试），返回是否成功。
    /// 服务组拉起的成员也是用户可见的启动动作，补记操作结果（否则操作记录缺失）。
    async fn start_once(&self, app: &AppHandle, item: &StackItem) -> Result<(), String> {
        match item.ref_type {
            StackItemRefType::Software => {
                match self.software_mgr.find_installed(&item.ref_id) {
                    Some(sw) if sw.status == SoftwareStatus::Running => {
                        // 已在运行，视为就绪
                        Ok(())
                    }
                    Some(sw) => {
                        let detail = format!("{} ({}, 服务组)", sw.version, sw.id);
                        let r = async {
                            sw_commands::do_start_software(
                                &self.software_mgr,
                                app,
                                &item.ref_id,
                                None,
                            )
                            .await
                            .map_err(|e| format!("启动软件 {} 失败: {}", item.ref_id, e))?;
                            self.wait_ready(item).await
                        }
                        .await;
                        crate::oplog_result!("start", sw.name, detail, r);
                        r
                    }
                    None => Err(format!("未找到已装软件: {}", item.ref_id)),
                }
            }
            StackItemRefType::Springboot => match self.springboot_mgr.find_app(&item.ref_id) {
                Ok(app_model) if app_model.status == AppStatus::Running => Ok(()),
                Ok(m) => {
                    let target = format!("{} ({})", m.name, m.id);
                    let r = sb_lifecycle::start_app(
                        &item.ref_id,
                        &self.springboot_mgr,
                        &self.software_mgr,
                        app,
                    )
                    .await
                    .map_err(|e| format!("启动 Spring Boot {} 失败: {}", item.ref_id, e));
                    crate::oplog_result!("springboot_start", target, "服务组", r);
                    r
                }
                Err(e) => Err(format!("未找到 Spring Boot 应用: {}", e)),
            },
        }
    }

    /// 等待成员就绪（进程存活 + 端口监听）。无端口服务回退为仅进程存活。
    async fn wait_ready(&self, item: &StackItem) -> Result<(), String> {
        let start = Instant::now();
        let max_ms = READY_TIMEOUT_MS;
        loop {
            if self.probe_ready(item) {
                return Ok(());
            }
            if start.elapsed().as_millis() as u64 >= max_ms {
                return Err(format!(
                    "成员 {} 启动超时（{}ms 内未就绪）",
                    item.ref_id, max_ms
                ));
            }
            tokio::time::sleep(Duration::from_millis(READY_POLL_MS)).await;
        }
    }

    /// 探测成员是否就绪：进程存活，且（若声明了端口）端口已被监听。
    fn probe_ready(&self, item: &StackItem) -> bool {
        match item.ref_type {
            StackItemRefType::Software => {
                let sw = match self.software_mgr.find_installed(&item.ref_id) {
                    Some(s) => s,
                    None => return false,
                };
                let pid_alive = match sw.pid {
                    Some(pid) => {
                        crate::services::software_manager::health_check::is_process_alive(pid)
                    }
                    None => return false,
                };
                if !pid_alive {
                    return false;
                }
                // 端口 0 视为无端口服务，跳过端口检查
                if sw.port != 0
                    && crate::services::software_manager::health_check::is_port_free(sw.port)
                {
                    return false;
                }
                true
            }
            StackItemRefType::Springboot => {
                let app_model = match self.springboot_mgr.find_app(&item.ref_id) {
                    Ok(a) => a,
                    Err(_) => return false,
                };
                let pid_alive = match app_model.pid {
                    Some(pid) => {
                        crate::services::software_manager::health_check::is_process_alive(pid)
                    }
                    None => return false,
                };
                if !pid_alive {
                    return false;
                }
                if let Some(port) = app_model.port {
                    if crate::services::software_manager::health_check::is_port_free(port) {
                        return false;
                    }
                }
                true
            }
        }
    }

    /// 成员是否已处于运行态（由被引用 Manager 的状态判定，复用其健康检查结论）
    fn is_member_running(&self, item: &StackItem) -> bool {
        match item.ref_type {
            StackItemRefType::Software => self
                .software_mgr
                .find_installed(&item.ref_id)
                .map(|s| s.status == SoftwareStatus::Running)
                .unwrap_or(false),
            StackItemRefType::Springboot => self
                .springboot_mgr
                .find_app(&item.ref_id)
                .map(|a| a.status == AppStatus::Running)
                .unwrap_or(false),
        }
    }

    /// 按 ref_id 解析成员类型（组外依赖可能是已装软件或 Spring Boot 应用）
    fn resolve_ref_type(&self, ref_id: &str) -> Option<StackItemRefType> {
        if self.software_mgr.find_installed(ref_id).is_some() {
            Some(StackItemRefType::Software)
        } else if self.springboot_mgr.find_app(ref_id).is_ok() {
            Some(StackItemRefType::Springboot)
        } else {
            None
        }
    }

    /// 按 ref_id 判断对应服务是否已运行
    fn ref_running(&self, ref_id: &str) -> bool {
        if let Some(sw) = self.software_mgr.find_installed(ref_id) {
            return sw.status == SoftwareStatus::Running;
        }
        if let Ok(app) = self.springboot_mgr.find_app(ref_id) {
            return app.status == AppStatus::Running;
        }
        false
    }

    /// 收集栈的外部依赖：enabled 成员 depends_on 中不在组内、且系统里可解析的 ref_id（去重）。
    /// 外部依赖先于组内启动，保证依赖方就绪探测能通过。
    fn collect_external_deps(&self, stack: &Stack) -> Vec<String> {
        let group: HashSet<String> = stack
            .items
            .iter()
            .filter(|i| i.enabled)
            .map(|i| i.ref_id.clone())
            .collect();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for it in stack.items.iter().filter(|i| i.enabled) {
            for dep in &it.depends_on {
                if group.contains(dep) || seen.contains(dep) {
                    continue;
                }
                if self.resolve_ref_type(dep).is_none() {
                    continue;
                }
                seen.insert(dep.clone());
                out.push(dep.clone());
            }
        }
        out
    }

    /// 启动单个外部依赖（复用成员启动逻辑；已在运行则跳过）
    async fn start_external(&self, app: &AppHandle, ref_id: &str) -> Result<(), String> {
        let rt = self
            .resolve_ref_type(ref_id)
            .ok_or_else(|| format!("未找到依赖软件: {}", ref_id))?;
        if self.ref_running(ref_id) {
            return Ok(());
        }
        let item = StackItem {
            ref_type: rt,
            ref_id: ref_id.to_string(),
            order: 0,
            depends_on: vec![],
            enabled: true,
            retry: 0,
        };
        self.start_once(app, &item)
            .await
            .map_err(|e| format!("外部依赖 {} 启动失败: {}", ref_id, e))
    }

    /// 停止单个外部依赖（仅对组拉起的调用；按 ref_id 解析类型后复用 stop_one）
    async fn stop_external(&self, app: &AppHandle, ref_id: &str) {
        if let Some(rt) = self.resolve_ref_type(ref_id) {
            let item = StackItem {
                ref_type: rt,
                ref_id: ref_id.to_string(),
                order: 0,
                depends_on: vec![],
                enabled: true,
                retry: 0,
            };
            self.stop_one(app, &item).await;
        }
    }

    /// 一键停止：逆序优雅停止（按拓扑分层逆序，后启动的先停）
    pub async fn stop(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let stack = self.get(id).ok_or_else(|| format!("未找到栈: {}", id))?;

        // 逆序：有拓扑计划则按分层逆序，否则按成员逆序
        let order: Vec<String> = match Self::compute_plan(&stack) {
            Ok(plan) => plan.layers.iter().rev().flatten().cloned().collect(),
            Err(_) => stack.items.iter().rev().map(|i| i.ref_id.clone()).collect(),
        };

        for ref_id in &order {
            if let Some(item) = stack
                .items
                .iter()
                .find(|it| &it.ref_id == ref_id && it.enabled)
            {
                self.stop_one(app, item).await;
            }
        }

        // 停止本次由栈拉起的组外依赖（启动前已在运行的不停，避免误杀用户独立使用的服务）。
        // 从持久化的栈记录读取，应用重启后仍能正确停止上次拉起的依赖。
        let managed = stack.managed_externals.clone().unwrap_or_default();
        if !managed.is_empty() {
            for dep in managed.iter().rev() {
                self.stop_external(app, dep).await;
            }
            let _ = self.set_managed_externals(&stack.id, vec![]);
        }

        let mut members: Vec<StackMemberRuntime> = stack
            .items
            .iter()
            .map(|it| StackMemberRuntime {
                ref_id: it.ref_id.clone(),
                status: StackMemberStatus::Stopped,
                message: String::new(),
            })
            .collect();
        members.extend(managed.into_iter().map(|ref_id| StackMemberRuntime {
            ref_id,
            status: StackMemberStatus::Stopped,
            message: String::new(),
        }));
        self.emit(app, &stack.id, StackMemberStatus::Stopped, members);
        Ok(())
    }

    /// 一键重启：先停后起，返回新启动计划
    pub async fn restart(&self, app: &AppHandle, id: &str) -> Result<StackStartPlan, String> {
        self.stop(app, id).await?;
        self.start(app, id).await
    }

    /// 停止单个成员（逆序编排的最小单元）
    async fn stop_one(&self, app: &AppHandle, item: &StackItem) {
        match item.ref_type {
            StackItemRefType::Software => {
                let sw = self.software_mgr.find_installed(&item.ref_id);
                let sw = match sw {
                    Some(s) => s,
                    None => return,
                };
                // 已停止：直接确保状态
                if !matches!(
                    sw.status,
                    SoftwareStatus::Running | SoftwareStatus::Starting
                ) {
                    self.software_mgr
                        .update_runtime_fields(
                            &item.ref_id,
                            SoftwareStatus::Stopped,
                            None,
                            None,
                            Some(chrono::Local::now().naive_local()),
                            None,
                        )
                        .ok();
                    lifecycle::unregister(&item.ref_id);
                    lifecycle::emit_status_changed(
                        app,
                        &item.ref_id,
                        SoftwareStatus::Stopped,
                        None,
                        None,
                    );
                    return;
                }
                // 更新为 Stopping
                self.software_mgr
                    .update_runtime_fields(
                        &item.ref_id,
                        SoftwareStatus::Stopping,
                        None,
                        None,
                        None,
                        None,
                    )
                    .ok();
                lifecycle::emit_status_changed(
                    app,
                    &item.ref_id,
                    SoftwareStatus::Stopping,
                    None,
                    None,
                );
                // 优雅停止 + 强杀（与 stop_software 命令一致）
                let pid = sw.pid;
                if let Some(p) = pid {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(15),
                        tokio::task::spawn_blocking(move || lifecycle::stop_one(p)),
                    )
                    .await;
                }
                self.software_mgr
                    .update_runtime_fields(
                        &item.ref_id,
                        SoftwareStatus::Stopped,
                        None,
                        None,
                        Some(chrono::Local::now().naive_local()),
                        None,
                    )
                    .ok();
                lifecycle::unregister(&item.ref_id);
                lifecycle::emit_status_changed(
                    app,
                    &item.ref_id,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                );
            }
            StackItemRefType::Springboot => {
                let _ =
                    sb_lifecycle::stop_app(&item.ref_id, &self.springboot_mgr, app).await;
            }
        }
    }

    // ------------------------- 导出 / 导入（R7） -------------------------

    /// 导出栈为 JSON 文件（含 items / depends_on），供团队分享
    pub fn export_stack(&self, id: &str, path: &str) -> Result<(), String> {
        let stack = self.get(id).ok_or_else(|| format!("未找到栈: {}", id))?;
        let content =
            serde_json::to_string_pretty(&stack).map_err(|e| format!("序列化栈失败: {}", e))?;
        if let Some(parent) = PathBuf::from(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, content).map_err(|e| format!("写入导出文件 {} 失败: {}", path, e))?;
        Ok(())
    }

    /// 从 JSON 文件导入栈：重新生成 id / created_at，保存落盘，导入时同样做环检测
    pub fn import_stack(&self, path: &str) -> Result<Stack, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取导入文件 {} 失败: {}", path, e))?;
        let mut stack: Stack =
            serde_json::from_str(&content).map_err(|e| format!("解析栈 JSON 失败: {}", e))?;
        stack.id = Uuid::new_v4().to_string();
        stack.created_at = now_utc();
        stack.updated_at = String::new();
        // 导入前环检测
        let _ = Self::compute_plan(&stack)?;

        {
            let mut inner = self.inner.lock().unwrap();
            inner.stacks.push(stack.clone());
            inner.save().map_err(|e| e.to_string())?;
        }
        Ok(stack)
    }

    // ------------------------- 事件 -------------------------

    fn emit(
        &self,
        app: &AppHandle,
        stack_id: &str,
        status: StackMemberStatus,
        members: Vec<StackMemberRuntime>,
    ) {
        let event = StackStatusEvent {
            stack_id: stack_id.to_string(),
            status,
            members,
        };
        if let Err(e) = app.emit("stack-status-changed", event) {
            tracing::warn!(error = %e, stack_id = %stack_id, "emit stack-status-changed 失败");
        }
    }
}

/// 当前 UTC 时间（ISO 8601）
fn now_utc() -> String {
    Utc::now().to_rfc3339()
}

/// 在节点集合中查找一个环路径（DFS 三色标记）。
/// 找不到环时回退返回所有残留 indeg>0 的节点。
fn find_cycle(nodes: &[&StackItem], node_ids: &HashSet<String>) -> Vec<String> {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes {
        let mut v = Vec::new();
        for dep in &n.depends_on {
            if node_ids.contains(dep) {
                v.push(dep.clone());
            }
        }
        adj.insert(n.ref_id.clone(), v);
    }

    let mut color: HashMap<String, u8> = HashMap::new(); // 0=未访问 1=在栈 2=完成
    let mut trace: Vec<String> = Vec::new();
    let mut result: Vec<String> = Vec::new();

    fn dfs(
        node: &str,
        adj: &HashMap<String, Vec<String>>,
        color: &mut HashMap<String, u8>,
        trace: &mut Vec<String>,
        result: &mut Vec<String>,
    ) -> bool {
        color.insert(node.to_string(), 1);
        trace.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for nb in neighbors {
                let c = color.get(nb).copied().unwrap_or(0);
                if c == 1 {
                    if let Some(pos) = trace.iter().position(|x| x == nb) {
                        let mut cyc = trace[pos..].to_vec();
                        cyc.push(nb.clone());
                        result.extend(cyc);
                        return true;
                    }
                } else if c == 0 {
                    if dfs(nb, adj, color, trace, result) {
                        return true;
                    }
                }
            }
        }
        trace.pop();
        color.insert(node.to_string(), 2);
        false
    }

    for n in nodes {
        if color.get(&n.ref_id).copied().unwrap_or(0) == 0 {
            if dfs(&n.ref_id, &adj, &mut color, &mut trace, &mut result) {
                break;
            }
        }
    }

    if result.is_empty() {
        result = nodes.iter().map(|n| n.ref_id.clone()).collect();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::software_manager::SoftwareManager;
    use crate::services::springboot_manager::SpringBootManager;

    fn item(ref_id: &str, order: u32, deps: &[&str]) -> StackItem {
        StackItem {
            ref_type: StackItemRefType::Software,
            ref_id: ref_id.to_string(),
            order,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            enabled: true,
            retry: 0,
        }
    }

    fn make_stack(items: Vec<StackItem>) -> Stack {
        Stack {
            id: "test".to_string(),
            name: "test".to_string(),
            description: String::new(),
            items,
            created_at: String::new(),
            updated_at: String::new(),
            managed_externals: None,
            auto_start: false,
            last_run_report: None,
        }
    }

    #[test]
    fn test_compute_plan_topological_order() {
        // A(无依赖) -> B(依赖A) -> C(依赖B)：期望分层 [[A],[B],[C]]
        let stack = make_stack(vec![
            item("C", 2, &["B"]),
            item("A", 0, &[]),
            item("B", 1, &["A"]),
        ]);
        let plan = StackManager::compute_plan(&stack).expect("应为无环计划");
        assert_eq!(plan.layers.len(), 3);
        assert_eq!(plan.layers[0], vec!["A".to_string()]);
        assert_eq!(plan.layers[1], vec!["B".to_string()]);
        assert_eq!(plan.layers[2], vec!["C".to_string()]);
    }

    #[test]
    fn test_compute_plan_same_layer_tie_break_by_order() {
        // A、B 均独立，order A=1 B=0 -> 同层按 order 升序 => [B, A]
        let stack = make_stack(vec![item("A", 1, &[]), item("B", 0, &[])]);
        let plan = StackManager::compute_plan(&stack).expect("应为无环计划");
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0], vec!["B".to_string(), "A".to_string()]);
    }

    #[test]
    fn test_compute_plan_detects_cycle() {
        // A 依赖 B，B 依赖 A => 环
        let stack = make_stack(vec![item("A", 0, &["B"]), item("B", 0, &["A"])]);
        let res = StackManager::compute_plan(&stack);
        assert!(res.is_err(), "应检测到环并返回错误");
        let msg = res.err().unwrap();
        assert!(msg.contains("循环依赖"), "错误信息应提示循环依赖: {}", msg);
    }

    #[test]
    fn test_save_load_roundtrip_with_temp_file() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "demo".to_string(),
                description: "roundtrip".to_string(),
                items: vec![item("A", 0, &[])],
                auto_start: false,
            })
            .expect("create 应成功");
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(mgr.get(&created.id).unwrap().name, "demo");

        // 重新从同一文件加载，验证持久化
        let mgr2 = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        assert_eq!(mgr2.list().len(), 1);
        assert_eq!(mgr2.get(&created.id).unwrap().name, "demo");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_create_rejects_cycle() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let res = mgr.create(CreateStackPayload {
            name: "cyclic".to_string(),
            description: String::new(),
            items: vec![item("A", 0, &["B"]), item("B", 0, &["A"])],
            auto_start: false,
        });
        assert!(res.is_err(), "存在环的栈应被拒绝创建");
        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------- compute_plan 拓扑排序 -------------------------

    #[test]
    fn test_compute_plan_empty_stack() {
        let stack = make_stack(vec![]);
        let plan = StackManager::compute_plan(&stack).expect("空栈应为无环计划");
        assert!(plan.layers.is_empty(), "空栈应产生空分层");
        assert!(plan.cycle.is_none());
    }

    #[test]
    fn test_build_run_report_aggregates_elapsed() {
        let members = vec![
            (
                "mysql".to_string(),
                StackMemberStatus::Running,
                1200u64,
                String::new(),
            ),
            (
                "redis".to_string(),
                StackMemberStatus::Running,
                800u64,
                String::new(),
            ),
            (
                "app".to_string(),
                StackMemberStatus::Failed,
                3000u64,
                "port busy".to_string(),
            ),
        ];
        let report =
            StackManager::build_run_report("2026-08-26T00:00:00Z".to_string(), 5000u64, members);
        assert_eq!(report.total_elapsed_ms, 5000);
        assert_eq!(report.members.len(), 3);
        assert_eq!(report.members[2].status, StackMemberStatus::Failed);
        assert_eq!(report.members[2].elapsed_ms, 3000);
    }

    #[test]
    fn test_compute_plan_single_node() {
        let stack = make_stack(vec![item("A", 0, &[])]);
        let plan = StackManager::compute_plan(&stack).expect("单节点应为无环计划");
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0], vec!["A".to_string()]);
    }

    #[test]
    fn test_compute_plan_diamond_dependency() {
        // A -> {B,C} -> D：期望分层 [[A],[B,C],[D]]，B/C 同层并发
        let stack = make_stack(vec![
            item("D", 3, &["B", "C"]),
            item("C", 2, &["A"]),
            item("B", 1, &["A"]),
            item("A", 0, &[]),
        ]);
        let plan = StackManager::compute_plan(&stack).expect("菱形依赖应为无环计划");
        assert_eq!(plan.layers.len(), 3);
        assert_eq!(plan.layers[0], vec!["A".to_string()]);
        assert_eq!(plan.layers[1], vec!["B".to_string(), "C".to_string()]);
        assert_eq!(plan.layers[2], vec!["D".to_string()]);
    }

    #[test]
    fn test_compute_plan_disabled_item_excluded() {
        // A(启用) -> B(禁用) -> C(启用, 依赖B)。
        // 禁用项不参与拓扑：B 被排除；C 对 B 的依赖被忽略 -> C 与 A 同层。
        let mut b = item("B", 1, &["A"]);
        b.enabled = false;
        let stack = make_stack(vec![item("A", 0, &[]), b, item("C", 2, &["B"])]);
        let plan = StackManager::compute_plan(&stack).expect("禁用项应被排除");
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0], vec!["A".to_string(), "C".to_string()]);
        assert!(!plan.layers[0].contains(&"B".to_string()));
    }

    #[test]
    fn test_compute_plan_depends_on_external_ignored() {
        // A 依赖一个不存在于栈内的 id -> 该边应被忽略，不应误判为环
        let stack = make_stack(vec![item("A", 0, &["not_a_member"])]);
        let plan = StackManager::compute_plan(&stack).expect("引用栈外成员不应形成环");
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(plan.layers[0], vec!["A".to_string()]);
    }

    #[test]
    fn test_compute_plan_self_dependency_is_cycle() {
        // A 依赖自身 -> 应检测为环
        let stack = make_stack(vec![item("A", 0, &["A"])]);
        let res = StackManager::compute_plan(&stack);
        assert!(res.is_err(), "自依赖应被检测为环");
        let msg = res.err().unwrap();
        assert!(msg.contains("循环依赖"), "错误信息应提示循环依赖: {}", msg);
    }

    #[test]
    fn test_compute_plan_order_tie_break_multiple_same_layer() {
        // 三个无依赖节点，order 分别为 2/0/1 -> 同层按 order 升序 [Y,Z,X]
        let stack = make_stack(vec![
            item("X", 2, &[]),
            item("Y", 0, &[]),
            item("Z", 1, &[]),
        ]);
        let plan = StackManager::compute_plan(&stack).expect("应为无环计划");
        assert_eq!(plan.layers.len(), 1);
        assert_eq!(
            plan.layers[0],
            vec!["Y".to_string(), "Z".to_string(), "X".to_string()]
        );
    }

    // ------------------------- build_plan -------------------------

    #[test]
    fn test_build_plan_missing_id() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let res = mgr.build_plan("does_not_exist");
        assert!(res.is_err(), "不存在的栈 build_plan 应返回错误");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_build_plan_existing_matches_compute_plan() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "p".to_string(),
                description: String::new(),
                items: vec![item("A", 0, &[]), item("B", 1, &["A"])],
                auto_start: false,
            })
            .unwrap();
        let plan = mgr.build_plan(&created.id).expect("已保存栈应有计划");
        assert_eq!(plan.layers.len(), 2);
        assert_eq!(plan.layers[0], vec!["A".to_string()]);
        assert_eq!(plan.layers[1], vec!["B".to_string()]);
        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------- create -------------------------

    #[test]
    fn test_create_empty_name_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        for name in ["", "   ", "\t"] {
            let res = mgr.create(CreateStackPayload {
                name: name.to_string(),
                description: String::new(),
                items: vec![],
                auto_start: false,
            });
            assert!(res.is_err(), "空白名称 '{}' 应被拒绝", name);
        }
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_create_valid_generates_id_and_timestamps() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "  spaced  ".to_string(),
                description: "desc".to_string(),
                items: vec![item("A", 0, &[])],
                auto_start: false,
            })
            .expect("合法栈应创建成功");
        assert!(!created.id.is_empty(), "创建后应生成 id");
        assert_eq!(created.name, "spaced", "名称应被 trim");
        assert!(!created.created_at.is_empty(), "应写入 created_at");
        assert_eq!(created.updated_at, "", "新建栈 updated_at 应为空");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_create_empty_items_ok() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "empty".to_string(),
                description: String::new(),
                items: vec![],
                auto_start: false,
            })
            .expect("无成员的空栈应允许创建");
        assert!(created.items.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------- update -------------------------

    #[test]
    fn test_update_rename_and_timestamp() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "orig".to_string(),
                description: String::new(),
                items: vec![item("A", 0, &[])],
                auto_start: false,
            })
            .unwrap();
        let updated = mgr
            .update(
                &created.id,
                UpdateStackPayload {
                    name: Some("renamed".to_string()),
                    description: None,
                    items: None,
                    auto_start: None,
                },
            )
            .expect("重命名应成功");
        assert_eq!(updated.name, "renamed");
        assert!(!updated.updated_at.is_empty(), "更新应写入 updated_at");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_update_empty_name_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "orig".to_string(),
                description: String::new(),
                items: vec![item("A", 0, &[])],
                auto_start: false,
            })
            .unwrap();
        let res = mgr.update(
            &created.id,
            UpdateStackPayload {
                name: Some("  ".to_string()),
                description: None,
                items: None,
                auto_start: None,
            },
        );
        assert!(res.is_err(), "更新为空名称应被拒绝");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_update_items_none_keeps_old_items() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "orig".to_string(),
                description: String::new(),
                items: vec![item("A", 0, &[]), item("B", 1, &[])],
                auto_start: false,
            })
            .unwrap();
        let updated = mgr
            .update(
                &created.id,
                UpdateStackPayload {
                    name: Some("renamed".to_string()),
                    description: None,
                    items: None,
                    auto_start: None,
                },
            )
            .unwrap();
        assert_eq!(updated.items.len(), 2, "未提供 items 时应保留原成员");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_update_missing_id_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let res = mgr.update(
            "nope",
            UpdateStackPayload {
                name: Some("x".to_string()),
                description: None,
                items: None,
                auto_start: None,
            },
        );
        assert!(res.is_err(), "更新不存在的栈应返回错误");
        assert!(res.err().unwrap().contains("未找到栈"), "应提示未找到栈");
        let _ = std::fs::remove_file(&tmp);
    }

    /// 回归测试：update 在环检测失败时，必须不破坏内存中已有的合法状态。
    /// 设计预期：保存前环检测 -> 拒绝；但不应已把内存里的 items 改成非法内容。
    #[test]
    fn test_update_rejects_cycle_preserves_in_memory_state() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        // 初始为单成员合法栈
        let created = mgr
            .create(CreateStackPayload {
                name: "orig".to_string(),
                description: String::new(),
                items: vec![item("A", 0, &[])],
                auto_start: false,
            })
            .unwrap();
        // 尝试更新为环（A<->B）
        let res = mgr.update(
            &created.id,
            UpdateStackPayload {
                name: None,
                description: None,
                items: Some(vec![item("A", 0, &["B"]), item("B", 0, &["A"])]),
                auto_start: None,
            },
        );
        assert!(res.is_err(), "更新为环应被拒绝");
        // 拒绝后，内存状态应仍为最初的单成员合法栈
        let after = mgr.get(&created.id).expect("栈应仍然存在");
        assert_eq!(
            after.items.len(),
            1,
            "拒绝环后内存中的成员数不应被改为 2（状态一致性 bug）"
        );
        assert_eq!(after.items[0].ref_id, "A");
        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------- delete -------------------------

    #[test]
    fn test_delete_existing_returns_true() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "d".to_string(),
                description: String::new(),
                items: vec![],
                auto_start: false,
            })
            .unwrap();
        let ok = mgr.delete(&created.id).expect("delete 不应返回错误");
        assert!(ok, "删除存在的栈应返回 true");
        assert_eq!(mgr.list().len(), 0);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_delete_missing_returns_false() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let ok = mgr.delete("missing").expect("delete 不应返回错误");
        assert!(!ok, "删除不存在的栈应返回 false");
        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------- 导出 / 导入 -------------------------

    #[test]
    fn test_export_import_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let created = mgr
            .create(CreateStackPayload {
                name: "roundtrip".to_string(),
                description: "d".to_string(),
                items: vec![item("A", 0, &[]), item("B", 1, &["A"])],
                auto_start: false,
            })
            .unwrap();

        let export_path = std::env::temp_dir().join(format!("opx_export_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&export_path);
        mgr.export_stack(&created.id, export_path.to_str().unwrap())
            .expect("导出应成功");
        assert!(export_path.exists(), "导出文件应被创建");

        // 用全新 manager 导入同一导出文件
        let mgr2 = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let imported = mgr2
            .import_stack(export_path.to_str().unwrap())
            .expect("导入应成功");
        assert_ne!(imported.id, created.id, "导入应生成新 id");
        assert_eq!(imported.name, created.name, "导入应保留名称");
        assert_eq!(imported.items.len(), 2, "导入应保留成员");
        assert_eq!(imported.items[1].depends_on, vec!["A".to_string()]);
        assert_eq!(mgr2.list().len(), 2, "导入后应有 2 个栈");

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&export_path);
    }

    #[test]
    fn test_import_missing_file_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let res = mgr.import_stack("C:/no/such/file.json");
        assert!(res.is_err(), "导入不存在的文件应返回错误");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_import_invalid_json_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let bad = std::env::temp_dir().join(format!("opx_bad_{}.json", Uuid::new_v4()));
        std::fs::write(&bad, "{ this is not json ]").unwrap();
        let res = mgr.import_stack(bad.to_str().unwrap());
        assert!(res.is_err(), "导入非法 JSON 应返回错误");
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&bad);
    }

    #[test]
    fn test_import_cyclic_rejected() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        // 手写一个含环的栈 JSON
        let mut cyc = make_stack(vec![item("A", 0, &["B"]), item("B", 0, &["A"])]);
        cyc.id = "x".to_string();
        cyc.created_at = "t".to_string();
        let json = serde_json::to_string_pretty(&cyc).unwrap();
        let path = std::env::temp_dir().join(format!("opx_cyc_{}.json", Uuid::new_v4()));
        std::fs::write(&path, json).unwrap();
        let res = mgr.import_stack(path.to_str().unwrap());
        assert!(res.is_err(), "导入含环的栈应被拒绝");
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&path);
    }

    // ------------------------- get / list -------------------------

    #[test]
    fn test_get_and_list() {
        let tmp = std::env::temp_dir().join(format!("opx_test_stacks_{}.json", Uuid::new_v4()));
        let _ = std::fs::remove_file(&tmp);
        let mgr = StackManager::new_with_path(
            Arc::new(SoftwareManager::new()),
            Arc::new(SpringBootManager::new()),
            tmp.clone(),
        );
        let a = mgr
            .create(CreateStackPayload {
                name: "a".to_string(),
                description: String::new(),
                items: vec![],
                auto_start: false,
            })
            .unwrap();
        let b = mgr
            .create(CreateStackPayload {
                name: "b".to_string(),
                description: String::new(),
                items: vec![],
                auto_start: false,
            })
            .unwrap();
        assert_eq!(mgr.list().len(), 2);
        assert!(mgr.get(&a.id).is_some());
        assert!(mgr.get(&b.id).is_some());
        assert!(mgr.get("missing").is_none());
        let _ = std::fs::remove_file(&tmp);
    }
}
