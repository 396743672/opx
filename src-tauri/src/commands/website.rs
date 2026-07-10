use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;

use crate::models::software::SoftwareStatus;
use crate::models::website::Site;
use crate::services::software_manager::SoftwareManager;
use crate::services::website_manager::{nginx_conf, WebsiteManager};
use crate::utils::archive;

/// 解析目标 nginx（软件管理里已安装的第一个 nginx 实例）
/// ponytail: 单 nginx 假设；多实例选择留待后续（Site 加 nginx_id）
fn resolve_nginx(sm: &SoftwareManager) -> Result<crate::models::software::InstalledSoftware, String> {
    sm.get_installed()
        .into_iter()
        .find(|s| s.key == "nginx")
        .ok_or_else(|| "请先在软件管理中安装 nginx".to_string())
}

#[cfg(windows)]
fn run_nginx(install_path: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(install_path.join("nginx.exe"))
        .args(args)
        .current_dir(install_path)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
}

#[cfg(not(windows))]
fn run_nginx(install_path: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("nginx")
        .args(args)
        .current_dir(install_path)
        .output()
}

/// 从当前站点列表重建 conf/sites/*.conf（幂等，天然处理删除/下线）；
/// 确保主配置 include；reload=true 且 nginx 运行时先 -t 校验再 reload。
fn regenerate(sm: &SoftwareManager, wm: &WebsiteManager, reload: bool) -> Result<(), String> {
    let nginx = resolve_nginx(sm)?;
    let base = PathBuf::from(&nginx.install_path);
    let conf_dir = base.join("conf");
    let sites_dir = conf_dir.join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;

    // 清空旧站点 conf，从启用列表重建
    if let Ok(rd) = std::fs::read_dir(&sites_dir) {
        for entry in rd.flatten() {
            if entry.path().extension().map(|x| x == "conf").unwrap_or(false) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    for site in wm.list().into_iter().filter(|s| s.enabled) {
        let block = nginx_conf::generate_server_block(&site);
        std::fs::write(sites_dir.join(format!("{}.conf", site.id)), block)
            .map_err(|e| e.to_string())?;
    }

    // 确保主配置 include（幂等）
    let main_conf = conf_dir.join("nginx.conf");
    if let Ok(content) = std::fs::read_to_string(&main_conf) {
        let updated = nginx_conf::ensure_include(&content);
        if updated != content {
            std::fs::write(&main_conf, updated).map_err(|e| e.to_string())?;
        }
    }

    if reload && nginx.status == SoftwareStatus::Running {
        let test = run_nginx(&base, &["-t"]).map_err(|e| e.to_string())?;
        if !test.status.success() {
            return Err(format!(
                "nginx 配置校验失败：{}",
                String::from_utf8_lossy(&test.stderr)
            ));
        }
        let rl = run_nginx(&base, &["-s", "reload"]).map_err(|e| e.to_string())?;
        if !rl.status.success() {
            return Err(format!(
                "nginx reload 失败：{}",
                String::from_utf8_lossy(&rl.stderr)
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_websites(wm: State<'_, Arc<WebsiteManager>>) -> Vec<Site> {
    wm.list()
}

#[tauri::command]
pub fn save_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    site: Site,
    apply: bool,
) -> Result<(), String> {
    wm.upsert(site).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, apply)
}

#[tauri::command]
pub fn delete_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    wm.remove(&id).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, true)
}

#[tauri::command]
pub fn set_website_enabled(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    wm.set_enabled(&id, enabled).map_err(|e| e.to_string())?;
    regenerate(&sm, &wm, true)
}

/// 上传静态包：解压 zip 到 <nginx>/sites-data/<id>/<sanitized-path>/，返回该目录路径供前端写入 location.root
#[tauri::command]
pub fn upload_site_bundle(
    sm: State<'_, Arc<SoftwareManager>>,
    id: String,
    loc_path: String,
    zip_path: String,
) -> Result<String, String> {
    let nginx = resolve_nginx(&sm)?;
    let base = PathBuf::from(&nginx.install_path);
    let sub = sanitize_seg(&loc_path);
    let dest = base.join("sites-data").join(&id).join(&sub);
    // 重新上传：先清空目标目录
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    // 复用带路径穿越防护的解压（enclosed_name 已过滤 ..）
    archive::extract_zip(Path::new(&zip_path), &dest, |_, _| {}).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().replace('\\', "/"))
}

/// 把 location path 转成安全目录段："/" -> "root"，其余非字母数字转 '_'
fn sanitize_seg(path: &str) -> String {
    let cleaned: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "root".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_seg;

    #[test]
    fn sanitize_seg_maps_root_and_paths() {
        assert_eq!(sanitize_seg("/"), "root");
        assert_eq!(sanitize_seg("/api"), "api");
        assert_eq!(sanitize_seg("/a/b"), "a_b");
    }
}
