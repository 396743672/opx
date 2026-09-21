//! 统一启动编排协调器（扩展 4）
//!
//! OPX 启动时，把原本分散的三路 auto_start（软件 / Node 应用 / 服务组 Stack）
//! 收敛为单一有序启动序列：按顺序拉起；任一项失败时对本次已成功拉起的项按逆序停止（回滚），
//! 并把逐项结果持久化为 `StartupReport` 供前端回显，同时逐项推 `startup-progress` 事件。
//!
//! 回滚边界（与扩展 1 软件级依赖区分）：
//! - 本协调器仅作用于「开机统一启动序列」：失败回滚本次已拉起项。
//! - 手动点启动软件拉依赖（扩展 1）：失败不回滚（不误杀共享依赖）。
//!
//! 可测性设计：真实启停通过 `StartupAction` 抽象注入（`ManagerStartup` 接三类 manager），
//! 因此「顺序」「失败逆序回滚」「报告内容」「事件序列」都能用 fake 动作确定性验证，
//! 不必依赖完整 manager 栈。

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tauri::Emitter;

use crate::models::startup::{
    KIND_NODE, KIND_SOFTWARE, KIND_STACK, StartupItemReport, StartupItemStatus, StartupReport,
    StartupTarget,
};
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;
use crate::services::stack_manager::StackManager;
use crate::utils::paths;

/// 启动报告持久化文件名（位于 `<app_data>/`）
const REPORT_FILE: &str = "startup_report.json";
/// 启动进度事件名（前端监听以实时追加卡片行）
pub const PROGRESS_EVENT: &str = "startup-progress";

type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// 启动/停止动作抽象：让编排逻辑与具体 manager 解耦，便于注入 fake 做确定性测试。
pub trait StartupAction: Send + Sync {
    fn start<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>>;
    fn stop<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>>;
}

/// 收集阶段的启动项候选（`kind` + 该类型内的排序值）
pub struct StartupCandidate {
    pub kind: &'static str,
    pub order: u64,
    pub id: String,
    pub name: String,
}

impl StartupCandidate {
    pub fn software(order: u64, id: String, name: String) -> Self {
        Self { kind: KIND_SOFTWARE, order, id, name }
    }
    pub fn node(order: u64, id: String, name: String) -> Self {
        Self { kind: KIND_NODE, order, id, name }
    }
    pub fn stack(id: String, name: String) -> Self {
        // 服务组无 startup_order，统一排在软件 / Node 之后（kind 已保证次序）
        Self { kind: KIND_STACK, order: 0, id, name }
    }
}

/// 执行顺序：软件 → Node 应用 → 服务组；同类内按 `order` 升序，同序保持收集顺序（稳定）。
///
/// 抽成纯函数以便单测——顺序错了会让依赖方先于被依赖方启动（最贵的错误）。
pub fn plan_targets(candidates: Vec<StartupCandidate>) -> Vec<StartupTarget> {
    fn rank(kind: &str) -> u8 {
        match kind {
            KIND_SOFTWARE => 0,
            KIND_NODE => 1,
            _ => 2,
        }
    }
    let mut rows: Vec<(u8, u64, usize, StartupCandidate)> = candidates
        .into_iter()
        .enumerate()
        .map(|(seq, c)| (rank(c.kind), c.order, seq, c))
        .collect();
    rows.sort_by_key(|(rank, order, seq, _)| (*rank, *order, *seq));
    rows.into_iter()
        .map(|(_, _, _, c)| StartupTarget {
            kind: c.kind.to_string(),
            id: c.id,
            name: c.name,
        })
        .collect()
}

/// 按计划执行启动编排：成功项收集 → 失败即逆序回滚已拉起项 → 剩余项标记跳过。
///
/// 每完成一项（成功/失败）即调用 `on_progress`，由调用方决定推事件还是收集断言。
pub async fn run_plan(
    plan: Vec<StartupTarget>,
    action: &dyn StartupAction,
    on_progress: &(dyn Fn(StartupItemReport) + Send + Sync),
) -> StartupReport {
    let started_at = chrono::Local::now().to_rfc3339();
    let overall = Instant::now();
    let mut items: Vec<StartupItemReport> = Vec::with_capacity(plan.len());
    let mut launched: Vec<StartupTarget> = Vec::new();
    let mut rolled_back = false;

    for (idx, target) in plan.iter().enumerate() {
        let t0 = Instant::now();
        let result = action.start(target).await;
        let elapsed_ms = t0.elapsed().as_millis() as u64;
        let mut item = StartupItemReport {
            kind: target.kind.clone(),
            id: target.id.clone(),
            name: target.name.clone(),
            status: StartupItemStatus::Ok,
            elapsed_ms,
            message: String::new(),
        };
        match result {
            Ok(()) => {
                launched.push(target.clone());
            }
            Err(e) => {
                item.status = StartupItemStatus::Failed;
                item.message = e.clone();
                on_progress(item.clone());
                items.push(item);
                tracing::warn!(kind = %target.kind, id = %target.id, name = %target.name, error = %e, "启动编排项失败");
                // 失败：逆序停止本次已成功拉起的项（回滚），不触碰用户此前独立启动的服务
                for prev in launched.iter().rev() {
                    match action.stop(prev).await {
                        Ok(()) => tracing::info!(kind = %prev.kind, id = %prev.id, "已回滚启动编排拉起项"),
                        Err(e) => tracing::warn!(kind = %prev.kind, id = %prev.id, error = %e, "启动编排回滚项失败"),
                    }
                }
                rolled_back = !launched.is_empty();
                // 本次未执行到的项标记跳过，便于用户看清哪些没被拉起
                for rest in plan.iter().skip(idx + 1) {
                    items.push(StartupItemReport {
                        kind: rest.kind.clone(),
                        id: rest.id.clone(),
                        name: rest.name.clone(),
                        status: StartupItemStatus::Skipped,
                        elapsed_ms: 0,
                        message: String::new(),
                    });
                }
                break;
            }
        }
        on_progress(item.clone());
        items.push(item);
    }

    StartupReport {
        started_at,
        total_elapsed_ms: overall.elapsed().as_millis() as u64,
        items,
        rolled_back,
    }
}

// ===== 报告持久化 =====

pub fn report_path() -> PathBuf {
    paths::data_dir().join(REPORT_FILE)
}

pub fn write_report_to(path: &Path, report: &StartupReport) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(report)?)?;
    Ok(())
}

/// 读取报告；文件缺失或损坏都降级为 None（不把错误抛给前端）
pub fn read_report_from(path: &Path) -> Option<StartupReport> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 最近一次启动报告
pub fn last_report() -> Option<StartupReport> {
    read_report_from(&report_path())
}

// ===== 真实动作实现 =====

struct ManagerStartup {
    software: Arc<SoftwareManager>,
    node: Arc<NodeAppManager>,
    stack: Arc<StackManager>,
    app: tauri::AppHandle,
    node_exe: Option<PathBuf>,
}

impl StartupAction for ManagerStartup {
    fn start<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            match target.kind.as_str() {
                KIND_SOFTWARE => crate::commands::software::do_start_software(
                    &self.software,
                    &self.app,
                    &target.id,
                    None,
                )
                .await
                .map_err(|e| e.to_string()),
                KIND_NODE => match self.node_exe.as_ref() {
                    Some(exe) => self.node.start(&target.id, exe).map_err(|e| e.to_string()),
                    None => Err("未找到 Node.js 运行时".to_string()),
                },
                KIND_STACK => self
                    .stack
                    .start(&self.app, &target.id)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                other => Err(format!("未知启动项类型: {}", other)),
            }
        })
    }

    fn stop<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            match target.kind.as_str() {
                KIND_SOFTWARE => {
                    stop_software(&self.software, &target.id);
                    Ok(())
                }
                KIND_NODE => self.node.stop(&target.id).map_err(|e| e.to_string()),
                KIND_STACK => self
                    .stack
                    .stop(&self.app, &target.id)
                    .await
                    .map_err(|e| e.to_string()),
                other => Err(format!("未知启动项类型: {}", other)),
            }
        })
    }
}

/// 停止软件：查询当前状态，若有运行中的 pid 则停止并回落状态。
fn stop_software(manager: &SoftwareManager, id: &str) {
    if let Some(sw) = manager.find_installed(id) {
        if let Some(pid) = sw.pid {
            crate::services::software_manager::lifecycle::stop_one(pid);
            let _ = manager.update_runtime_fields(
                id,
                crate::models::software::SoftwareStatus::Stopped,
                None,
                None,
                None,
                None,
            );
        }
    }
}

/// 统一启动编排入口：收集三类自启目标 → 排序 → 执行（失败回滚）→ 推事件 + 落盘报告。
#[allow(clippy::too_many_arguments)]
pub async fn run_bootstrap(
    software: Arc<SoftwareManager>,
    node: Arc<NodeAppManager>,
    stack: Arc<StackManager>,
    app: tauri::AppHandle,
    node_exe: Option<PathBuf>,
) {
    let mut candidates: Vec<StartupCandidate> = Vec::new();
    for sw in software.list_auto_start() {
        candidates.push(StartupCandidate::software(
            sw.startup_order as u64,
            sw.id.clone(),
            sw.name.clone(),
        ));
    }
    if node_exe.is_some() {
        for a in node.auto_start_list() {
            candidates.push(StartupCandidate::node(
                a.startup_order as u64,
                a.id.clone(),
                a.name.clone(),
            ));
        }
    }
    for st in stack.list() {
        if st.auto_start {
            candidates.push(StartupCandidate::stack(st.id.clone(), st.name.clone()));
        }
    }

    let plan = plan_targets(candidates);
    if plan.is_empty() {
        tracing::info!("启动编排：无自启项");
        return;
    }

    let action = ManagerStartup {
        software,
        node,
        stack,
        app: app.clone(),
        node_exe,
    };
    let app_for_progress = app.clone();
    let on_progress = move |item: StartupItemReport| {
        if let Err(e) = app_for_progress.emit(PROGRESS_EVENT, &item) {
            tracing::warn!(error = %e, "推送启动进度事件失败");
        }
    };

    let report = run_plan(plan, &action, &on_progress).await;
    if let Err(e) = write_report_to(&report_path(), &report) {
        tracing::warn!(error = %e, "写入启动报告失败");
    }
    tracing::info!(
        items = report.items.len(),
        total_ms = report.total_elapsed_ms,
        rolled_back = report.rolled_back,
        "启动编排完成"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// fake 动作：记录调用顺序；可指定第 N 次 start 失败
    struct FakeAction {
        fail_at: Option<usize>,
        calls: Mutex<usize>,
        started: Mutex<Vec<String>>,
        stopped: Mutex<Vec<String>>,
    }

    impl FakeAction {
        fn new(fail_at: Option<usize>) -> Self {
            Self {
                fail_at,
                calls: Mutex::new(0),
                started: Mutex::new(vec![]),
                stopped: Mutex::new(vec![]),
            }
        }
        fn started(&self) -> Vec<String> {
            self.started.lock().unwrap().clone()
        }
        fn stopped(&self) -> Vec<String> {
            self.stopped.lock().unwrap().clone()
        }
    }

    impl StartupAction for FakeAction {
        fn start<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                let idx = {
                    let mut calls = self.calls.lock().unwrap();
                    let idx = *calls;
                    *calls += 1;
                    idx
                };
                if self.fail_at == Some(idx) {
                    return Err(format!("{} 启动失败", target.name));
                }
                self.started.lock().unwrap().push(target.id.clone());
                Ok(())
            })
        }

        fn stop<'a>(&'a self, target: &'a StartupTarget) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                self.stopped.lock().unwrap().push(target.id.clone());
                Ok(())
            })
        }
    }

    fn targets(ids: &[&str]) -> Vec<StartupTarget> {
        ids.iter()
            .map(|id| StartupTarget {
                kind: KIND_SOFTWARE.to_string(),
                id: (*id).to_string(),
                name: (*id).to_string(),
            })
            .collect()
    }

    fn noop_progress() -> impl Fn(StartupItemReport) + Send + Sync {
        |_| {}
    }

    #[test]
    fn plan_orders_software_then_node_then_stack() {
        let plan = plan_targets(vec![
            StartupCandidate::stack("st1".into(), "组1".into()),
            StartupCandidate::node(5, "n1".into(), "应用1".into()),
            StartupCandidate::software(9, "s2".into(), "软件2".into()),
            StartupCandidate::software(1, "s1".into(), "软件1".into()),
            StartupCandidate::node(2, "n2".into(), "应用2".into()),
        ]);
        let got: Vec<&str> = plan.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(got, vec!["s1", "s2", "n2", "n1", "st1"]);
    }

    #[test]
    fn plan_keeps_collection_order_for_equal_order() {
        let plan = plan_targets(vec![
            StartupCandidate::software(1, "a".into(), "a".into()),
            StartupCandidate::software(1, "b".into(), "b".into()),
        ]);
        let got: Vec<&str> = plan.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(got, vec!["a", "b"], "同序应保持收集顺序");
    }

    #[tokio::test]
    async fn run_plan_all_ok_records_items_without_rollback() {
        let action = FakeAction::new(None);
        let report = run_plan(targets(&["a", "b"]), &action, &noop_progress()).await;

        assert_eq!(action.started(), vec!["a", "b"]);
        assert!(action.stopped().is_empty(), "无失败不应回滚");
        assert!(!report.rolled_back);
        assert_eq!(report.items.len(), 2);
        assert!(report.items.iter().all(|i| i.status == StartupItemStatus::Ok));
        assert!(report.items.iter().all(|i| i.message.is_empty()));
        assert!(!report.started_at.is_empty());
    }

    #[tokio::test]
    async fn run_plan_rolls_back_launched_items_in_reverse_on_failure() {
        // 第 3 项（索引 2，id=c）失败 → 应逆序停止 b、a；d 标记跳过
        let action = FakeAction::new(Some(2));
        let report = run_plan(targets(&["a", "b", "c", "d"]), &action, &noop_progress()).await;

        assert_eq!(action.started(), vec!["a", "b"], "失败项不计入已拉起");
        assert_eq!(action.stopped(), vec!["b", "a"], "回滚须逆序且只含本次拉起项");
        assert!(report.rolled_back);

        let statuses: Vec<(String, StartupItemStatus)> = report
            .items
            .iter()
            .map(|i| (i.id.clone(), i.status))
            .collect();
        assert_eq!(
            statuses,
            vec![
                ("a".to_string(), StartupItemStatus::Ok),
                ("b".to_string(), StartupItemStatus::Ok),
                ("c".to_string(), StartupItemStatus::Failed),
                ("d".to_string(), StartupItemStatus::Skipped),
            ]
        );
        let failed = report.items.iter().find(|i| i.id == "c").unwrap();
        assert!(failed.message.contains("启动失败"), "失败项应带原因: {}", failed.message);
        assert_eq!(report.items.iter().find(|i| i.id == "d").unwrap().elapsed_ms, 0);
    }

    #[tokio::test]
    async fn run_plan_first_item_failure_marks_rest_skipped_without_rollback() {
        let action = FakeAction::new(Some(0));
        let report = run_plan(targets(&["a", "b"]), &action, &noop_progress()).await;

        assert_eq!(action.started(), Vec::<String>::new());
        assert!(action.stopped().is_empty(), "无可回滚项时不应调用 stop");
        assert!(!report.rolled_back, "没有已拉起项时 rolled_back 应为 false");
        assert_eq!(report.items[0].status, StartupItemStatus::Failed);
        assert_eq!(report.items[1].status, StartupItemStatus::Skipped);
    }

    #[tokio::test]
    async fn run_plan_emits_progress_only_for_executed_items() {
        let action = FakeAction::new(Some(1));
        let events: Mutex<Vec<(String, StartupItemStatus)>> = Mutex::new(vec![]);
        let collect = |item: StartupItemReport| {
            events.lock().unwrap().push((item.id, item.status));
        };
        run_plan(targets(&["a", "b", "c"]), &action, &collect).await;

        let got = events.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![
                ("a".to_string(), StartupItemStatus::Ok),
                ("b".to_string(), StartupItemStatus::Failed),
            ],
            "跳过的 item 不推进度事件"
        );
    }

    #[test]
    fn report_round_trips_through_disk() {
        let dir = std::env::temp_dir().join("opx-startup-report-test");
        let path = dir.join("startup_report.json");
        let _ = std::fs::remove_file(&path);

        let report = StartupReport {
            started_at: "2026-09-21T13:00:00+08:00".to_string(),
            total_elapsed_ms: 1234,
            items: vec![StartupItemReport {
                kind: KIND_STACK.to_string(),
                id: "st1".to_string(),
                name: "联调组".to_string(),
                status: StartupItemStatus::Failed,
                elapsed_ms: 900,
                message: "端口占用".to_string(),
            }],
            rolled_back: true,
        };
        write_report_to(&path, &report).unwrap();
        assert_eq!(read_report_from(&path).unwrap(), report);

        // 损坏文件降级为 None，不 panic
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_report_from(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
