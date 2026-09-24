use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::State;

use crate::models::software::SoftwareStatus;
use crate::models::website::Site;
use crate::services::software_manager::SoftwareManager;
use crate::services::website_manager::{nginx_conf, WebsiteManager};
use crate::utils::archive;
use crate::{audited, audited_async};

/// 解析目标 nginx（软件管理里已安装的第一个 nginx 实例）
/// ponytail: 单 nginx 假设；多实例选择留待后续（Site 加 nginx_id）
pub fn resolve_nginx(
    sm: &SoftwareManager,
) -> Result<crate::models::software::InstalledSoftware, String> {
    let mut nginx = sm
        .get_installed()
        .into_iter()
        .find(|s| s.key == "nginx")
        .ok_or_else(|| "请先在软件管理中安装 nginx".to_string())?;
    nginx.install_path = crate::utils::paths::resolve_install_path(&nginx.install_path)
        .to_string_lossy()
        .to_string();
    Ok(nginx)
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
pub fn regenerate(sm: &SoftwareManager, wm: &WebsiteManager, reload: bool) -> Result<(), String> {
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
            tracing::warn!(stderr = %String::from_utf8_lossy(&test.stderr), "nginx 配置校验失败");
            return Err("i18n:nginxConfInvalid".to_string());
        }
        let rl = run_nginx(&base, &["-s", "reload"]).map_err(|e| e.to_string())?;
        if !rl.status.success() {
            tracing::warn!(stderr = %String::from_utf8_lossy(&rl.stderr), "nginx reload 失败");
            return Err("i18n:nginxReloadFailed".to_string());
        }
    }
    Ok(())
}

/// 同步 sites 目录下的 conf 文件到与站点列表一致的状态（纯文件 IO，便于单测）。
///
/// # 规则（全部站点统一）
/// - **启用** → 确保 `.conf` 存在（若当前为 `.conf.disabled` 则改回；不存在则从数据生成）
/// - **停用** → `.conf` 改名为 `.conf.disabled`（内容保留，nginx 不加载）
/// - **已删除** → 清理阶段删掉不再属于任何站点的 `.conf` / `.conf.disabled`
/// - **手写站点** 的 `.conf` 通过源码视图写入，此处仅改名不覆盖
fn sync_site_files(sites_dir: &Path, sites: &[Site]) -> std::io::Result<()> {
    // 所有站点的文件名（启用态 .conf 与停用态 .conf.disabled）都要保护，清理阶段不得删除
    let protected: std::collections::HashSet<String> = sites
        .iter()
        .flat_map(|s| {
            let f = nginx_conf::site_conf_filename(s);
            [format!("{}.disabled", f), f]
        })
        .collect();

    // 清理：删除不属于任何站点的残留 .conf / .conf.disabled（如已删除站点）
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

        if site.enabled {
            // 启用态：确保 .conf 存在
            if disabled_path.exists() && !conf_path.exists() {
                // 从停用恢复：只改名，不写内容
                std::fs::rename(&disabled_path, &conf_path)?;
            } else if !conf_path.exists() && !disabled_path.exists() {
                // 全新启用：从数据生成
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&conf_path, block)?;
            } else if !site.custom_conf {
                // ponytail: 表单模式 → 每次保存都从表单数据重建 .conf，确保路由更改生效
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&conf_path, block)?;
            } // 手写模式（custom_conf=true）→ 跳过，保留源码视图写入的内容
        } else {
            // 停用态：改为 .disabled（nginx 不加载）
            if conf_path.exists() {
                std::fs::rename(&conf_path, &disabled_path)?;
            }
            // ponytail: 表单模式 → 停用态也随表单重建 .disabled，
            // 避免站点停用时编辑（如切换 SSL/路由）后配置残留旧内容，下次启用备份失效
            if !site.custom_conf && disabled_path.exists() {
                let block = nginx_conf::generate_server_block(site);
                std::fs::write(&disabled_path, block)?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn list_websites(wm: State<'_, Arc<WebsiteManager>>) -> Vec<Site> {
    wm.list()
}

/// 解析站点中待处理的 zip 标记（_pending_/*），解压到 sites-data/<name>/<path>/ 并替换 root
fn resolve_pending_zips(site: &mut Site, install_path: &Path) -> anyhow::Result<()> {
    let pending_dir = install_path.join("sites-data").join(".pending");
    for loc in &mut site.locations {
        let root = match &loc.root {
            Some(r) if r.starts_with("_pending_/") => r.clone(),
            _ => continue,
        };
        // root = "_pending_/{id}_{path}.zip" → 匹配出 zip 文件名
        let zip_name = root.trim_start_matches("_pending_/");
        let staged = pending_dir.join(zip_name);
        if !staged.exists() {
            continue;
        }
        // 目标目录：sites-data/<sanitized_name>/<sanitized_path>/
        let name_seg = sanitize_seg(&site.name);
        let name_seg = if name_seg.is_empty() || name_seg == "root" {
            site.id.chars().take(8).collect::<String>()
        } else {
            name_seg
        };
        let path_seg = sanitize_seg(&loc.path);
        let dest = install_path
            .join("sites-data")
            .join(&name_seg)
            .join(&path_seg);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::create_dir_all(&dest)?;
        archive::extract_zip_flatten(&staged, &dest, |_, _| {})?;
        let _ = std::fs::remove_file(&staged);
        // ponytail: 存相对路径 sites-data/{name}/{path}，nginx 配置生成时再解析为绝对路径
        loc.root = Some(format!("sites-data/{}/{}", name_seg, path_seg));
    }
    Ok(())
}

#[tauri::command]
pub fn save_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    mut site: Site,
) -> Result<(), String> {
    let target = format!("{} ({})", site.name, site.id);
    audited!("website_save", target, "", {
        // 先验证 nginx 可用（失败则立即返回，不碰内存和磁盘）
        let nginx = resolve_nginx(&sm)?;
        resolve_pending_zips(&mut site, &Path::new(&nginx.install_path))
            .map_err(|e| e.to_string())?;
        // 写入内存（不持久化），regenerate 验证后再持久化
        wm.upsert_mem(site).map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn delete_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    let site = wm.get(&id);
    let target = site
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.clone());
    audited!("website_delete", target, "", {
        if let Some(ref s) = site {
            if s.enabled {
                return Err("i18n:deleteRunningHint".to_string());
            }
        }
        // 记录名称用于清理上传文件（必须在 wm.remove 之前获取）
        let name_seg = site.as_ref().and_then(|s| {
            let n = sanitize_seg(&s.name);
            if n.is_empty() || n == "root" {
                None
            } else {
                Some(n)
            }
        });
        wm.remove_mem(&id).map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())?;
        // 删除站点对应的上传文件（sites-data/<name>/），避免下次同名站点文件残留
        if let Some(seg) = name_seg {
            if let Ok(nginx) = resolve_nginx(&sm) {
                let data_dir = PathBuf::from(&nginx.install_path)
                    .join("sites-data")
                    .join(&seg);
                let _ = std::fs::remove_dir_all(&data_dir);
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_website_enabled(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let name = wm.get(&id).map(|s| s.name).unwrap_or_default();
    let action = if enabled { "enable" } else { "disable" };
    let target = format!("{} ({})", name, action);
    audited!("website_toggle", target, "", {
        wm.set_enabled_mem(&id, enabled)
            .map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())
    })
}

/// 上传静态包：将 zip 暂存到 <nginx>/sites-data/.pending/，返回标记路径供保存时解压
#[tauri::command]
pub fn upload_site_bundle(
    sm: State<'_, Arc<SoftwareManager>>,
    id: String,
    loc_path: String,
    zip_path: String,
) -> Result<String, String> {
    let nginx = resolve_nginx(&sm)?;
    let pending_dir = PathBuf::from(&nginx.install_path)
        .join("sites-data")
        .join(".pending");
    std::fs::create_dir_all(&pending_dir).map_err(|e| e.to_string())?;
    let sub = sanitize_seg(&loc_path);
    let staged = pending_dir.join(format!("{}_{}.zip", id, sub));
    // 覆盖式复制：重新上传时替换旧文件
    std::fs::copy(&zip_path, &staged).map_err(|e| e.to_string())?;
    // 返回标记路径，格式：_pending_/{id}/{path}.zip
    Ok(format!("_pending_/{}_{}.zip", id, sub))
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
    let nginx = resolve_nginx(&manager)?;

    let site = website_manager.get(&id);

    // 站点已落库且配置文件已存在 → 读取实际内容（含停用态 .conf.disabled）
    if let Some(s) = &site {
        let sites_dir = PathBuf::from(&nginx.install_path)
            .join("conf")
            .join("sites");
        let fname = nginx_conf::site_conf_filename(s);
        for candidate in [
            sites_dir.join(&fname),
            sites_dir.join(format!("{}.disabled", fname)),
        ] {
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
    let nginx = resolve_nginx(&manager)?;

    let site = website_manager
        .get(&id)
        .ok_or_else(|| "站点不存在，请先保存站点".to_string())?;

    let sites_dir = PathBuf::from(&nginx.install_path)
        .join("conf")
        .join("sites");
    std::fs::create_dir_all(&sites_dir).map_err(|e| e.to_string())?;
    let fname = nginx_conf::site_conf_filename(&site);
    // 停用站点写入 .conf.disabled（不被 include 加载），启用站点写 .conf；
    // 并清除另一态的残留文件，避免同一站点同时存在两份。
    let (write_path, stale_path) = if site.enabled {
        (
            sites_dir.join(&fname),
            sites_dir.join(format!("{}.disabled", fname)),
        )
    } else {
        (
            sites_dir.join(format!("{}.disabled", fname)),
            sites_dir.join(&fname),
        )
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
        let rl = run_nginx(Path::new(&nginx.install_path), &["-s", "reload"])
            .map_err(|e| e.to_string())?;
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
        let sites_dir = PathBuf::from(&nginx.install_path)
            .join("conf")
            .join("sites");
        let fname = nginx_conf::site_conf_filename(&site);
        let _ = std::fs::remove_file(sites_dir.join(&fname));
        let _ = std::fs::remove_file(sites_dir.join(format!("{}.disabled", fname)));
    }

    regenerate(&sm, &wm, true)
}

/// 生成自签 HTTPS 证书（纯 Rust rcgen，规避 Windows nginx 包无 openssl CLI）
/// 写入 <nginx>/sites-data/certs/<domain>.crt / .key，返回相对路径供 ssl 配置使用
#[tauri::command]
pub fn generate_self_signed_cert(
    sm: State<'_, Arc<SoftwareManager>>,
    domain: String,
) -> Result<(String, String), String> {
    let d = domain.trim().to_string();
    if d.is_empty() || d.contains('/') || d.contains('\\') || d.contains(char::is_whitespace) {
        tracing::warn!(domain = %domain, "自签证书：域名不合法");
        return Err("i18n:certDomainInvalid".to_string());
    }

    let nginx = resolve_nginx(&sm)?;
    let certs_dir = PathBuf::from(&nginx.install_path)
        .join("sites-data")
        .join("certs");
    std::fs::create_dir_all(&certs_dir).map_err(|e| format!("创建证书目录失败: {e}"))?;

    // rcgen 生成自签 X.509：SAN=domain，CN=domain
    let mut params =
        rcgen::CertificateParams::new(vec![d.clone()]).map_err(|e| format!("证书参数错误: {e}"))?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, &d);
    let key_pair = rcgen::KeyPair::generate().map_err(|e| format!("密钥生成失败: {e}"))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("证书生成失败: {e}"))?;

    let file_base = sanitize_cert_name(&d);
    let rel = |ext: &str| format!("sites-data/certs/{file_base}.{ext}");
    std::fs::write(certs_dir.join(format!("{file_base}.crt")), cert.pem())
        .map_err(|e| format!("证书写入失败: {e}"))?;
    std::fs::write(
        certs_dir.join(format!("{file_base}.key")),
        key_pair.serialize_pem(),
    )
    .map_err(|e| format!("私钥写入失败: {e}"))?;

    Ok((rel("crt"), rel("key")))
}

/// 为站点申请/重新申请 Let's Encrypt 证书（DNS-01）。
#[tauri::command]
pub async fn issue_site_certificate(
    app: tauri::AppHandle,
    wm: State<'_, Arc<WebsiteManager>>,
    sm: State<'_, Arc<SoftwareManager>>,
    dns_accounts: State<'_, Arc<crate::services::dns_account::DnsAccountManager>>,
    site_id: String,
) -> Result<(), String> {
    let site = wm
        .get(&site_id)
        .ok_or_else(|| format!("未找到站点: {}", site_id))?;
    let raw = site
        .server_name
        .clone()
        .ok_or_else(|| "请先填写 server_name（域名）".to_string())?;
    let domain = sanitize_domain(&raw)?;
    let target = format!("{} ({})", site.name, domain);

    audited_async!("acme_issue", target, "", {
        use tauri::Emitter;
        let accounts = dns_accounts.list();
        let account = site
            .ssl
            .dns_account_id
            .as_deref()
            .and_then(|id| accounts.iter().find(|a| a.id == id))
            .cloned()
            .ok_or_else(|| "请先在站点 SSL 配置里选择 DNS 账号".to_string())?;
        let acme_settings = crate::services::acme::AcmeSettings {
            account,
            use_staging: crate::commands::config::read_settings()?.acme_use_staging,
        };

        let nginx = resolve_nginx(&sm)?;
        let cert_dir = PathBuf::from(&nginx.install_path)
            .join("sites-data")
            .join("certs");

        let app_for_progress = app.clone();
        let domain_for_progress = domain.clone();
        let on_progress = move |phase: &str, msg: &str| {
            let _ = app_for_progress.emit(
                "acme-progress",
                serde_json::json!({ "domain": domain_for_progress, "phase": phase, "message": msg }),
            );
        };

        crate::services::acme::issue_certificate(&domain, &acme_settings, &cert_dir, on_progress)
            .await
            // {:#} 展开 anyhow 的 error chain，否则前端只看到最外层 context
            // （如「创建 DNS 挑战记录失败」）而看不到真实原因（如 Cloudflare 权限不足）
            .map_err(|e| format!("{:#}", e))?;

        // 更新站点 ssl 并落盘 + 重新生成 nginx 配置并 reload
        let mut updated = site.clone();
        updated.ssl.enabled = true;
        updated.ssl.acme = true;
        // 与自签一致存**相对**路径（生成 conf 时会按 conf/ 基准补 ../）；
        // 存绝对路径会在 nginx 卸载/重装到别的目录后失效
        let base = format!("sites-data/certs/{}", domain);
        updated.ssl.cert_path = Some(format!("{base}.crt"));
        updated.ssl.key_path = Some(format!("{base}.key"));
        updated.ssl.cert_expires_at =
            Some((chrono::Local::now() + chrono::Duration::days(90)).to_rfc3339());
        wm.upsert_mem(updated).map_err(|e| e.to_string())?;
        if let Err(e) = regenerate(&sm, &wm, true) {
            // 回滚内存中的 ssl，避免内存/磁盘/nginx 三者不一致
            let _ = wm.upsert_mem(site.clone());
            return Err(e);
        }
        wm.persist().map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// 返回所有站点的 (站点 id, 站点名, 绑定的账号 id)，供账号删除前的引用检查。
#[tauri::command]
pub fn list_account_refs(wm: State<'_, Arc<WebsiteManager>>) -> Vec<(String, String, String)> {
    wm.list()
        .into_iter()
        .filter_map(|s| {
            s.ssl
                .dns_account_id
                .map(|aid| (s.id.clone(), s.name.clone(), aid))
        })
        .collect()
}

fn sanitize_cert_name(s: &str) -> String {
    let c: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if c.is_empty() {
        "cert".to_string()
    } else {
        c
    }
}

/// 校验并规范化域名：去空白；空、含空白或路径分隔符则拒绝。
pub(crate) fn sanitize_domain(domain: &str) -> Result<String, String> {
    let d = domain.trim();
    if d.is_empty() {
        return Err("请先填写 server_name（域名）".to_string());
    }
    if d.chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '\\')
    {
        return Err(format!("域名不合法: {}", d));
    }
    Ok(d.to_string())
}

#[cfg(test)]
mod domain_tests {
    use super::sanitize_domain;

    #[test]
    fn sanitize_domain_trims_and_rejects_bad() {
        assert_eq!(sanitize_domain("  example.com ").unwrap(), "example.com");
        assert!(sanitize_domain("").is_err());
        assert!(sanitize_domain("   ").is_err());
        assert!(sanitize_domain("a b.com").is_err());
        assert!(sanitize_domain("a/b.com").is_err());
        assert!(sanitize_domain("a\\b.com").is_err());
    }
}
