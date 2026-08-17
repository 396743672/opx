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
use crate::models::software::{SoftwareStatus};
use crate::models::springboot::AppStatus;
use crate::models::stack::{
    CreateStackPayload, Stack, StackItem, StackItemRefType, StackMemberRuntime,
    StackMemberStatus, StackStartPlan, StackStatusEvent, UpdateStackPayload,
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
    pub fn new(
        software_mgr: Arc<SoftwareManager>,
        springboot_mgr: Arc<SpringBootManager>,
    ) -> Self {
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
        };
        // 保存前环检测：存在环则拒绝写入
        let _ = Self::compute_plan(&stack)?;

        {
            let mut inner = self.inner.lock().unwrap();
            inner.stacks.push(stack.clone());
            inner
                .save()
                .map_err(|e| e.to_string())?;
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
        if let Some(name) = payload.name {
            let n = name.trim().to_string();
            if n.is_empty() {
                return Err("栈名称不能为空".to_string());
            }
            stack.name = n;
        }
        if let Some(desc) = payload.description {
            stack.description = desc;
        }
        if let Some(items) = payload.items {
            stack.items = items;
        }
        stack.updated_at = now_utc();
        // 保存前环检测
        let _ = Self::compute_plan(stack)?;
        let cloned = stack.clone();
        inner.save().map_err(|e| e.to_string())?;
        Ok(cloned)
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
        let stack = self
            .get(id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;
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

    /// 一键启动：逐批（layers 顺序）启动，批内并发；依赖就绪探测 + 重试 + 回滚。
    pub async fn start(&self, app: &AppHandle, id: &str) -> Result<StackStartPlan, String> {
        let stack = self
            .get(id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;

        // 运行前再次拓扑排序（双重保险）
        let plan = Self::compute_plan(&stack)?;

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

        let mut started: Vec<StackItem> = Vec::new();

        for layer in &plan.layers {
            // 批内并发启动
            let mut futs = Vec::new();
            for ref_id in layer {
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
                        started.push(
                            stack.items.iter().find(|it| &it.ref_id == ref_id).cloned().unwrap(),
                        );
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
                self.emit(
                    app,
                    &stack.id,
                    StackMemberStatus::Failed,
                    member_status.values().cloned().collect(),
                );
                return Err(format!(
                    "栈启动失败，已回滚本次已启动成员: {}",
                    failure_msgs.join("; ")
                ));
            }
        }

        self.emit(
            app,
            &stack.id,
            StackMemberStatus::Running,
            member_status.values().cloned().collect(),
        );
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
        // R9：依赖成员必须已就绪（Running）
        for dep_ref in &item.depends_on {
            let dep_item = stack.items.iter().find(|it| &it.ref_id == dep_ref);
            if let Some(di) = dep_item {
                if !self.is_member_running(di) {
                    return Err(format!(
                        "依赖成员 {} 未就绪（需先成功启动）",
                        dep_ref
                    ));
                }
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

    /// 实际执行一次启动（不含重试），返回是否成功
    async fn start_once(&self, app: &AppHandle, item: &StackItem) -> Result<(), String> {
        match item.ref_type {
            StackItemRefType::Software => {
                match self.software_mgr.find_installed(&item.ref_id) {
                    Some(sw) if sw.status == SoftwareStatus::Running => {
                        // 已在运行，视为就绪
                        Ok(())
                    }
                    Some(_) => {
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
                    None => Err(format!("未找到已装软件: {}", item.ref_id)),
                }
            }
            StackItemRefType::Springboot => match self.springboot_mgr.find_app(&item.ref_id) {
                Ok(app_model) if app_model.status == AppStatus::Running => Ok(()),
                Ok(_) => {
                    sb_lifecycle::start_app(
                        &item.ref_id,
                        &self.springboot_mgr,
                        &self.software_mgr,
                        app,
                    )
                    .await
                    .map_err(|e| format!("启动 Spring Boot {} 失败: {}", item.ref_id, e))
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
                    Some(pid) => crate::services::software_manager::health_check::is_process_alive(pid),
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
                    Some(pid) => crate::services::software_manager::health_check::is_process_alive(pid),
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

    /// 一键停止：逆序优雅停止（按拓扑分层逆序，后启动的先停）
    pub async fn stop(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let stack = self
            .get(id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;

        // 逆序：有拓扑计划则按分层逆序，否则按成员逆序
        let order: Vec<String> = match Self::compute_plan(&stack) {
            Ok(plan) => plan.layers.iter().rev().flatten().cloned().collect(),
            Err(_) => stack
                .items
                .iter()
                .rev()
                .map(|i| i.ref_id.clone())
                .collect(),
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

        let members: Vec<StackMemberRuntime> = stack
            .items
            .iter()
            .map(|it| StackMemberRuntime {
                ref_id: it.ref_id.clone(),
                status: StackMemberStatus::Stopped,
                message: String::new(),
            })
            .collect();
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
                if !matches!(sw.status, SoftwareStatus::Running | SoftwareStatus::Starting) {
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
                let _ = sb_lifecycle::stop_app(
                    &item.ref_id,
                    &self.springboot_mgr,
                    &self.software_mgr,
                    app,
                )
                .await;
            }
        }
    }

    // ------------------------- 导出 / 导入（R7） -------------------------

    /// 导出栈为 JSON 文件（含 items / depends_on），供团队分享
    pub fn export_stack(&self, id: &str, path: &str) -> Result<(), String> {
        let stack = self
            .get(id)
            .ok_or_else(|| format!("未找到栈: {}", id))?;
        let content = serde_json::to_string_pretty(&stack)
            .map_err(|e| format!("序列化栈失败: {}", e))?;
        if let Some(parent) = PathBuf::from(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, content)
            .map_err(|e| format!("写入导出文件 {} 失败: {}", path, e))?;
        Ok(())
    }

    /// 从 JSON 文件导入栈：重新生成 id / created_at，保存落盘，导入时同样做环检测
    pub fn import_stack(&self, path: &str) -> Result<Stack, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取导入文件 {} 失败: {}", path, e))?;
        let mut stack: Stack = serde_json::from_str(&content)
            .map_err(|e| format!("解析栈 JSON 失败: {}", e))?;
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
        });
        assert!(res.is_err(), "存在环的栈应被拒绝创建");
        let _ = std::fs::remove_file(&tmp);
    }
}
