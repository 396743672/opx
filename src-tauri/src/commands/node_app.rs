use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use opx_core::models::node_app::{
    CreateNodeAppParams, NodeApp, UpdateNodeAppParams,
};
use crate::services::node_app_manager::NodeAppManager;
use crate::services::software_manager::SoftwareManager;

/// 解析 Node.js 的 node 可执行文件：node_id 非空时限定该实例，否则取第一个已装
///
/// 官方包的层级两端不同：Windows zip 解出 `<root>/node.exe`，Linux tar.gz 解出
/// `<root>/bin/node`，存量安装还可能多一层（`<root>/node-v22-linux-x64/bin/node`）。
/// 所以不能拼死路径，统一走 [`opx_core::utils::exec::resolve_bin`]。
pub fn resolve_node_exe(sw_mgr: &SoftwareManager, node_id: Option<&str>) -> Option<PathBuf> {
    for sw in sw_mgr.get_installed() {
        if sw.key == "node" {
            if let Some(id) = node_id {
                if !id.is_empty() && sw.id != id {
                    continue;
                }
            }
            let abs = opx_core::utils::paths::resolve_install_path(&sw.install_path);
            if let Some(exe) = opx_core::utils::exec::resolve_bin(&abs, "node") {
                // 存量安装可能丢了执行位（旧版解压不还原权限）→ 就地补，避免 `os error 13`
                opx_core::utils::exec::ensure_executable(&exe);
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

/// 上传 Node 入口文件（Web 端）。
///
/// 桌面端由 dialog 返回本机路径直接填 `entry_path`；浏览器给不出路径，
/// 于是先落到 `data_dir/uploads/node/`，再把该路径当 `entry_path` 交给
/// `add_node_app`/`update_node_app`——它们本来就会把入口复制进
/// `node-apps/<name>/<name>.js`，所以暂存文件只是一次中转，不参与运行。
#[tauri::command]
pub async fn upload_node_entry(name: String, data_base64: String) -> Result<String, String> {
    use base64::Engine as _;

    let safe = crate::commands::springboot::safe_upload_name(&name);
    // 入口必须是 JS 家族文件：`.js` 是 Node 的默认，`.mjs`/`.cjs` 按各自模块语义跑，
    // `.ts` 只在用户装了运行时转译时才有意义——这里只做后缀白名单，不解释语义。
    let ext_ok = ["js", "mjs", "cjs", "ts"].iter().any(|e| {
        safe.to_ascii_lowercase()
            .ends_with(&format!(".{}", e))
    });
    if safe.is_empty() || !ext_ok {
        return Err("仅支持 .js / .mjs / .cjs / .ts 入口文件".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("文件数据 base64 解码失败: {e}"))?;
    const MAX: usize = 20 * 1024 * 1024;
    if bytes.len() > MAX {
        return Err(format!("入口文件超过 {} MB 上限", MAX / 1024 / 1024));
    }

    let dir = opx_core::utils::paths::data_dir().join("uploads").join("node");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建上传目录失败: {e}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("{ts}_{safe}"));
    opx_core::utils::paths::write_file_restricted(&path, &bytes)
        .map_err(|e| format!("写入上传文件失败: {e}"))?;
    // 入口脚本不是敏感文件：放开成可读，否则复制进运行目录后同样是 0600/只读，
    // 下一次「重新上传入口」覆盖会失败。
    opx_core::utils::paths::relax_file_permissions(&path);
    Ok(path.to_string_lossy().into_owned())
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
    let p = opx_core::utils::paths::resolve_data_path(&app.log_path);
    if app.log_path.is_empty() || !p.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read(p).map_err(|e| format!("读取日志失败: {}", e))?;
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(200);
    Ok(lines[start..].to_vec())
}