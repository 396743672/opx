//! 统一启动编排协调器（扩展 4）
//!
//! OPX 启动时，把原本分散的三路 auto_start（软件 / Node 应用 / 服务组 Stack）
//! 收敛为单一有序启动序列：按顺序拉起，每项实时推送进度，产出持久化启动报告；
//! 任一项失败时对本次已成功拉起的项按逆序停止（回滚）。
//!
//! 回滚边界（与扩展 1 软件级依赖区分）：
//! - 本协调器仅作用于「开机统一启动序列」：失败回滚本次已拉起项。
//! - 手动点启动软件拉依赖（扩展 1）：失败不回滚（不误杀共享依赖）。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;
use crate::services::stack_manager::StackManager;
use crate::utils::paths;

/// 启动报告文件
const REPORT_FILE: &str = "startup_report.json";

/// 单个被启动目标的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupItemReport {
    pub kind: String,    // software / node / stack
    pub id: String,
    pub name: String,
    pub status: String,  // running / failed / skipped
    pub elapsed_ms: u64,
    #[serde(default)]
    pub message: String,
}

/// 最近一次启动编排的整体报告
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartupReport {
    pub started_at: String,
    pub total_elapsed_ms: u64,
    #[serde(default)]
    pub items: Vec<StartupItemReport>,
}

fn report_path() -> PathBuf {
    paths::data_dir().join(REPORT_FILE)
}

/// 读取最近一次启动报告
pub fn read_startup_report() -> Option<StartupReport> {
    let path = report_path();
    if !path.exists() {
        return None;
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn write_startup_report(report: &StartupReport) {
    if let Some(parent) = report_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(report) {
        let _ = std::fs::write(report_path(), s);
    }
}

/// 统一启动编排：软件 → Node → Stack（各自按内部排序字段升序）。
#[allow(clippy::too_many_arguments)]
pub async fn run_bootstrap(
    software: Arc<SoftwareManager>,
    node: Arc<NodeAppManager>,
    stack: Arc<StackManager>,
    app: tauri::AppHandle,
    node_exe: Option<PathBuf>,
) {
    let started_at = chrono::Utc::now().to_rfc3339();
    let t0 = std::time::Instant::now();

    // 1. 收集三类目标并合并排序（软件/Node 按 startup_order，Stack 按创建时间靠后）
    // 用 (order, seq) 稳定排序：Stack 排在软件/Node 之后。
    let mut targets: Vec<(u64, usize, String, String, String, u32)> = Vec::new();
    let mut seq = 0usize;
    // note: order 用 u64 统一，stack 给一个超大值排最后
    let stack_base: u64 = 1_000_000;
    for sw in software.list_auto_start() {
        targets.push((sw.startup_order as u64, seq, "software".into(), sw.id.clone(), sw.name.clone(), 0));
        seq += 1;
    }
    if node_exe.is_some() {
        for a in node.auto_start_list() {
            targets.push((a.startup_order as u64, seq, "node".into(), a.id.clone(), a.name.clone(), 0));
            seq += 1;
        }
    }
    for st in stack.list() {
        if st.auto_start {
            targets.push((stack_base, seq, "stack".into(), st.id.clone(), st.name.clone(), 0));
            seq += 1;
        }
    }
    targets.sort_by_key(|(o, s, _, _, _, _)| (*o, *s));

    let mut report = StartupReport {
        started_at,
        total_elapsed_ms: 0,
        items: Vec::new(),
    };
    let mut launched: Vec<StartupItemReport> = Vec::new(); // 本次已成功拉起的项（回滚用逆序）

    // 2. 逐项启动并记录
    for (_order, _seq, kind, id, name, _) in &targets {
        let item_t0 = std::time::Instant::now();
        let result: Result<(), String> = match kind.as_str() {
            "software" => crate::commands::software::do_start_software(&software, &app, id, None)
                .await
                .map_err(|e| e.to_string()),
            "node" => {
                match node_exe.clone() {
                    Some(exe) => node.start(id, &exe).map_err(|e| e.to_string()),
                    None => Err("未找到 Node.js 运行时".to_string()),
                }
            }
            "stack" => stack
                .start(&app, id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
            _ => unreachable!(),
        };
        let elapsed = item_t0.elapsed().as_millis() as u64;

        let item = match result {
            Ok(_) => {
                launched.push(StartupItemReport {
                    kind: kind.clone(),
                    id: id.clone(),
                    name: name.clone(),
                    status: "running".into(),
                    elapsed_ms: elapsed,
                    message: String::new(),
                });
                StartupItemReport {
                    kind: kind.clone(),
                    id: id.clone(),
                    name: name.clone(),
                    status: "running".into(),
                    elapsed_ms: elapsed,
                    message: String::new(),
                }
            }
            Err(e) => {
                tracing::warn!(kind, id, error = %e, "启动编排项失败");
                StartupItemReport {
                    kind: kind.clone(),
                    id: id.clone(),
                    name: name.clone(),
                    status: "failed".into(),
                    elapsed_ms: elapsed,
                    message: e,
                }
            }
        };
        // 实时推送进度
        let _ = app.emit("startup-progress", &item);
        // 失败：逆序停止本次已成功拉起的项（回滚）
        if item.status == "failed" {
            rollback(&software, &node, &stack, &app, &launched).await;
            report.items.push(item);
            break;
        }
        report.items.push(item);
    }

    report.total_elapsed_ms = t0.elapsed().as_millis() as u64;
    write_startup_report(&report);
    let _ = app.emit("startup-completed", &report);
    tracing::info!(items = report.items.len(), "启动编排完成");
}

/// 对已拉起项按逆序停止（回滚）。
async fn rollback(
    software: &SoftwareManager,
    node: &NodeAppManager,
    stack: &StackManager,
    app: &tauri::AppHandle,
    launched: &[StartupItemReport],
) {
    for item in launched.iter().rev() {
        match item.kind.as_str() {
            "software" => stop_software(software, &item.id),
            "node" => {
                let _ = node.stop(&item.id);
            }
            "stack" => {
                let _ = stack.stop(app, &item.id).await;
            }
            _ => {}
        }
        tracing::info!(kind = %item.kind, id = %item.id, "启动失败回滚已拉起项");
    }
}

/// 停止软件：查询当前状态，若有运行中的 pid 则停止并回落状态。
fn stop_software(manager: &SoftwareManager, id: &str) {
    let sw = manager.find_installed(id);
    if let Some(sw) = sw {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_roundtrip_default_empty() {
        let r = StartupReport::default();
        assert!(r.items.is_empty());
        assert_eq!(r.total_elapsed_ms, 0);
    }
}
