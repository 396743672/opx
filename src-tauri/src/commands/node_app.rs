use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use crate::models::node_app::{
    CreateNodeAppParams, NodeApp, UpdateNodeAppParams,
};
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;

/// 解析 Node.js 的 node.exe：node_id 非空时限定该实例，否则取第一个已装
pub fn resolve_node_exe(sw_mgr: &SoftwareManager, node_id: Option<&str>) -> Option<PathBuf> {
    for sw in sw_mgr.get_installed() {
        if sw.key == "node" {
            if let Some(id) = node_id {
                if !id.is_empty() && sw.id != id {
                    continue;
                }
            }
            let abs = crate::utils::paths::resolve_install_path(&sw.install_path);
            let exe = abs.join(if cfg!(windows) { "node.exe" } else { "node" });
            if exe.exists() {
                return Some(exe);
            }
        }
    }
    None
}

#[tauri::command]
pub async fn list_node_apps(
    manager: State<'_, Arc<NodeAppManager>>,
) -> Result<Vec<NodeApp>, String> {
    Ok(manager.list())
}

#[tauri::command]
pub async fn add_node_app(
    manager: State<'_, Arc<NodeAppManager>>,
    params: CreateNodeAppParams,
) -> Result<NodeApp, String> {
    manager.create(params)
}

#[tauri::command]
pub async fn update_node_app(
    manager: State<'_, Arc<NodeAppManager>>,
    id: String,
    params: UpdateNodeAppParams,
) -> Result<NodeApp, String> {
    manager.update(&id, params)
}

#[tauri::command]
pub async fn delete_node_app(
    manager: State<'_, Arc<NodeAppManager>>,
    id: String,
) -> Result<bool, String> {
    Ok(manager.delete(&id))
}

#[tauri::command]
pub async fn start_node_app(
    manager: State<'_, Arc<NodeAppManager>>,
    software: State<'_, Arc<SoftwareManager>>,
    id: String,
) -> Result<(), String> {
    let app = manager.get(&id).ok_or_else(|| "未找到应用".to_string())?;
    let nid = app.node_installed_id.clone();
    let exe = resolve_node_exe(&software, if nid.is_empty() { None } else { Some(&nid) })
        .ok_or_else(|| "未找到已安装的 Node.js 运行时，请先安装 Node".to_string())?;
    manager.start(&id, &exe)
}

#[tauri::command]
pub async fn stop_node_app(
    manager: State<'_, Arc<NodeAppManager>>,
    id: String,
) -> Result<(), String> {
    manager.stop(&id)
}

/// 读取 Node 应用日志尾部（最多 200 行）
#[tauri::command]
pub async fn read_node_app_log(
    manager: State<'_, Arc<NodeAppManager>>,
    id: String,
) -> Result<Vec<String>, String> {
    let app = manager.get(&id).ok_or_else(|| "未找到应用".to_string())?;
    let p = crate::utils::paths::resolve_data_path(&app.log_path);
    if app.log_path.is_empty() || !p.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read(p).map_err(|e| format!("读取日志失败: {}", e))?;
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(200);
    Ok(lines[start..].to_vec())
}