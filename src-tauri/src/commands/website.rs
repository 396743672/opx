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

/// 从当前站点列表重建 conf/sites/*.conf（幂等，天然处理删除/下线）。
/// 文件处理规则见 [`sync_site_files`]。
fn regenerate(sm: &SoftwareManager, wm: &WebsiteManager, reload: bool) -> Result<(), String> {
    let nginx = resolve_nginx(sm)?;
    let base = PathBuf::from(&nginx.install_path);
    let conf_dir = base.join("conf");
    let sites_dir = conf_dir.join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;

    sync_site_files(&sites_dir, &wm.list()).map_err(|e| e.to_string())?;

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

/// 同步 sites 目录下的 conf 文件到与站点列表一致的状态（纯文件 IO，便于单测）。
///
/// # 规则
/// - **表单站点**（custom_conf=false）：conf 由数据重建，可无损重建。
///   启用 → 写 `<name>.conf`；停用/删除 → 不写（清理阶段移除残留）。
/// - **手写站点**（custom_conf=true）：内容仅存在于文件中，**绝不删除**，避免永久丢失。
///   启用 → 保留/恢复为 `.conf`（若之前被禁用为 `.conf.disabled` 则改回）；
///   停用 → 改名为 `.conf.disabled`（内容保留，`include sites/*.conf` 不匹配 → 不加载）。
fn sync_site_files(sites_dir: &Path, sites: &[Site]) -> std::io::Result<()> {
    // 手写站点的文件名（启用态 .conf 与停用态 .conf.disabled）都要保护，清理阶段不得删除
    let protected: std::collections::HashSet<String> = sites
        .iter()
        .filter(|s| s.custom_conf)
        .flat_map(|s| {
            let f = nginx_conf::site_conf_filename(s);
            [format!("{}.disabled", f), f]
        })
        .collect();

    // 清理：删除所有非手写站点残留的 .conf / .conf.disabled（表单站点可重建，无损）
    if let Ok(rd) = std::fs::read_dir(sites_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_conf_like = name.ends_with(".conf") || name.ends_with(".conf.disabled");
            if is_conf_like && !protected.contains(name) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    for site in sites {
        let fname = nginx_conf::site_conf_filename(site);
        let conf_path = sites_dir.join(&fname);
        let disabled_path = sites_dir.join(format!("{}.disabled", fname));

        if site.custom_conf {
            // 手写站点：仅改名切换加载状态，绝不删除内容
            if site.enabled {
                if disabled_path.exists() && !conf_path.exists() {
                    std::fs::rename(&disabled_path, &conf_path)?;
                }
            } else if conf_path.exists() {
                std::fs::rename(&conf_path, &disabled_path)?;
            }
        } else if site.enabled {
            // 表单站点：启用则由数据重建
            let block = nginx_conf::generate_server_block(site);
            std::fs::write(&conf_path, block)?;
        }
        // 表单站点停用：清理阶段已移除其 .conf，无需处理
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
) -> Result<(), String> {
    wm.upsert(site).map_err(|e| e.to_string())?;
    // 保存即生效：nginx 运行中时 regenerate 内部会自动校验并 reload
    regenerate(&sm, &wm, true)
}

#[tauri::command]
pub fn delete_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    // 运行中（enabled）的站点禁止删除，须先停用
    if let Some(site) = wm.get(&id) {
        if site.enabled {
            return Err("请先停用站点后再删除".to_string());
        }
    }
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

/// 读取指定站点的 nginx 配置文件（conf/sites/<name_port_id>.conf），不存在则返回默认 server 模板
#[tauri::command]
pub fn get_site_conf(
    manager: State<'_, Arc<SoftwareManager>>,
    website_manager: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<String, String> {
    let nginx = manager
        .get_installed()
        .into_iter()
        .find(|s| s.key == "nginx")
        .ok_or_else(|| "未找到 nginx 安装".to_string())?;

    let site = website_manager.get(&id);

    // 站点已落库且配置文件已存在 → 读取实际内容（含停用态 .conf.disabled）
    if let Some(s) = &site {
        let sites_dir = PathBuf::from(&nginx.install_path).join("conf").join("sites");
        let fname = nginx_conf::site_conf_filename(s);
        for candidate in [sites_dir.join(&fname), sites_dir.join(format!("{}.disabled", fname))] {
            if candidate.exists() {
                return std::fs::read_to_string(&candidate).map_err(|e| e.to_string());
            }
        }
    }

    // 站点尚未落库（新建态）或尚无 conf 文件 → 回退默认模板，不报错
    let listen = site.as_ref().map(|s| s.listen).unwrap_or(80);
    let server_name = site
        .as_ref()
        .and_then(|s| s.server_name.as_deref())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("_")
        .to_string();
    let root = site
        .as_ref()
        .and_then(|s| s.locations.first())
        .and_then(|l| l.root.as_ref())
        .map(|s| s.replace('\\', "/"))
        .unwrap_or_default();

    Ok(format!(
        r#"# 自定义站点配置模板
# 修改后点击"保存"重新写入并 reload nginx
server {{
    listen {};
    server_name {};

    root "{}";
    index index.html;

    location / {{
        # 单页应用（Vue/React/Angular）回退入口，静态站可删除注释
        try_files $uri $uri/ /index.html;
    }}

    # location /api {{
    #     proxy_pass http://127.0.0.1:3000;
    #     proxy_set_header Host $host;
    #     proxy_set_header X-Real-IP $remote_addr;
    #     proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    #     proxy_set_header X-Forwarded-Proto $scheme;
    # }}
}}
"#,
        listen, server_name, root
    ))
}

/// 写入站点 nginx 配置文件，保存后自动 reload
#[tauri::command]
pub fn set_site_conf(
    manager: State<'_, Arc<SoftwareManager>>,
    website_manager: State<'_, Arc<WebsiteManager>>,
    id: String,
    content: String,
) -> Result<(), String> {
    let nginx = manager
        .get_installed()
        .into_iter()
        .find(|s| s.key == "nginx")
        .ok_or_else(|| "未找到 nginx 安装".to_string())?;

    let site = website_manager
        .get(&id)
        .ok_or_else(|| "站点不存在，请先保存站点".to_string())?;

    let sites_dir = PathBuf::from(&nginx.install_path).join("conf").join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;
    let fname = nginx_conf::site_conf_filename(&site);
    // 停用站点写入 .conf.disabled（不被 include 加载），启用站点写 .conf；
    // 并清除另一态的残留文件，避免同一站点同时存在两份。
    let (write_path, stale_path) = if site.enabled {
        (sites_dir.join(&fname), sites_dir.join(format!("{}.disabled", fname)))
    } else {
        (sites_dir.join(format!("{}.disabled", fname)), sites_dir.join(&fname))
    };
    std::fs::write(&write_path, content).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&stale_path);

    // 若 nginx 正在运行，先校验再 reload
    if nginx.status == crate::models::software::SoftwareStatus::Running {
        let test = run_nginx(Path::new(&nginx.install_path), &["-t"]).map_err(|e| e.to_string())?;
        if !test.status.success() {
            return Err(format!(
                "nginx 配置校验失败: {}",
                String::from_utf8_lossy(&test.stderr)
            ));
        }
        let rl = run_nginx(Path::new(&nginx.install_path), &["-s", "reload"]).map_err(|e| e.to_string())?;
        if !rl.status.success() {
            return Err(format!(
                "nginx reload 失败: {}",
                String::from_utf8_lossy(&rl.stderr)
            ));
        }
    }

    // 保存成功后锁定为手写模式，regenerate 不再覆盖该站点的 conf
    website_manager
        .set_custom_conf(&id, true)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 解除手写模式：清除 custom_conf 标记、删除该站点手写 conf，再由表单重新生成并 reload
#[tauri::command]
pub fn unlock_site_conf(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    wm.set_custom_conf(&id, false).map_err(|e| e.to_string())?;

    // 删除手写 conf（启用态 .conf 与停用态 .conf.disabled 都清掉），
    // 交给 regenerate 按表单重建（若站点已下线则重建时自然不生成）
    if let Some(site) = wm.get(&id) {
        let nginx = resolve_nginx(&sm)?;
        let sites_dir = PathBuf::from(&nginx.install_path).join("conf").join("sites");
        let fname = nginx_conf::site_conf_filename(&site);
        let _ = std::fs::remove_file(sites_dir.join(&fname));
        let _ = std::fs::remove_file(sites_dir.join(format!("{}.disabled", fname)));
    }

    regenerate(&sm, &wm, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // 构造一个最小 Site（配合 nginx_conf::site_conf_filename 只需要 id/name/listen/custom_conf/enabled）
    fn site(id: &str, name: &str, listen: u16, custom_conf: bool, enabled: bool) -> Site {
        Site {
            id: id.to_string(),
            name: name.to_string(),
            server_name: None,
            listen,
            ssl: Default::default(),
            enabled,
            custom_conf,
            locations: vec![],
        }
    }

    #[test]
    fn stop_custom_site_does_not_delete_file_but_renames() {
        // 手写站点启用 → 有 .conf 文件；停用 → 改为 .conf.disabled（内容保留）
        let dir = TempDir::new().unwrap();
        let mut s = site("12345678-aaaa", "mysite", 81, true, true);
        std::fs::write(dir.path().join("mysite_81_12345678.conf"), "# 手写内容").unwrap();

        // 启用态同步 → 保持 .conf
        s.enabled = true;
        sync_site_files(dir.path(), &[s.clone()]).unwrap();
        assert!(dir.path().join("mysite_81_12345678.conf").exists());
        assert!(!dir.path().join("mysite_81_12345678.conf.disabled").exists());
        let content = std::fs::read_to_string(dir.path().join("mysite_81_12345678.conf")).unwrap();
        assert_eq!(content, "# 手写内容");

        // 停用 → .conf 改名为 .conf.disabled，内容不变
        s.enabled = false;
        sync_site_files(dir.path(), &[s.clone()]).unwrap();
        assert!(!dir.path().join("mysite_81_12345678.conf").exists());
        assert!(dir.path().join("mysite_81_12345678.conf.disabled").exists());
        let content = std::fs::read_to_string(dir.path().join("mysite_81_12345678.conf.disabled")).unwrap();
        assert_eq!(content, "# 手写内容");

        // 重新启用 → .conf.disabled 改回 .conf，内容不变
        s.enabled = true;
        sync_site_files(dir.path(), &[s]).unwrap();
        assert!(dir.path().join("mysite_81_12345678.conf").exists());
        assert!(!dir.path().join("mysite_81_12345678.conf.disabled").exists());
        let content = std::fs::read_to_string(dir.path().join("mysite_81_12345678.conf")).unwrap();
        assert_eq!(content, "# 手写内容");
    }

    #[test]
    fn stop_form_site_deletes_conf_recoverable() {
        // 表单站点停用 → .conf 被删；重新启用 → 从数据重建，内容恢复
        let dir = TempDir::new().unwrap();
        let mut s = site("deadbeef", "formsite", 80, false, true);

        // 启用态同步 → 生成 .conf
        sync_site_files(dir.path(), &[s.clone()]).unwrap();
        assert!(dir.path().join("formsite_80_deadbeef.conf").exists());
        let content = std::fs::read_to_string(dir.path().join("formsite_80_deadbeef.conf")).unwrap();
        assert!(content.contains("listen 80;"));

        // 停用 → .conf 被删（内容在数据，无损）
        s.enabled = false;
        sync_site_files(dir.path(), &[s.clone()]).unwrap();
        assert!(!dir.path().join("formsite_80_deadbeef.conf").exists());

        // 重新启用 → 从数据重建 .conf
        s.enabled = true;
        sync_site_files(dir.path(), &[s]).unwrap();
        assert!(dir.path().join("formsite_80_deadbeef.conf").exists());
    }

    #[test]
    fn sanitize_seg_maps_root_and_paths() {
        assert_eq!(sanitize_seg("/"), "root");
        assert_eq!(sanitize_seg("/api"), "api");
        assert_eq!(sanitize_seg("/a/b"), "a_b");
    }
}
