use std::sync::Arc;
use std::io::{Read, Write};
use std::path::Path;

use tauri::{AppHandle, Emitter, State};

use crate::models::springboot::{
    AppGroup, CreateAppParams, JvmInfo, JvmOptsTemplate, ReplaceResult, SpringBootApp,
    UpdateAppParams,
};
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::jvm_opts;
use crate::services::springboot_manager::SpringBootManager;
use crate::oplog;

#[tauri::command]
pub async fn list_springboot_apps(
    manager: State<'_, Arc<SpringBootManager>>,
) -> Result<Vec<SpringBootApp>, String> {
    Ok(manager.list_apps())
}

#[tauri::command]
pub async fn create_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    params: CreateAppParams,
) -> Result<SpringBootApp, String> {
    oplog!("springboot_create", &params.name);
    // ponytail: 有指定端口才查重，None 表示动态端口不校验
    if let Some(port) = params.port {
        let apps = manager.list_apps();
        if apps.iter().any(|a| a.port == Some(port)) {
            return Err(format!("端口 {} 已被其他应用占用", port));
        }
    }
    manager.create_app(params).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    params: UpdateAppParams,
) -> Result<SpringBootApp, String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_update", &format!("{} ({})", name, id));
    manager.update_app(&id, params).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_delete", &format!("{} ({})", name, id));
    manager.delete_app(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_start", &format!("{} ({})", name, id));
    crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
}

#[tauri::command]
pub async fn stop_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_stop", &format!("{} ({})", name, id));
    crate::services::springboot_manager::lifecycle::stop_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
}

#[tauri::command]
pub async fn restart_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_restart", &format!("{} ({})", name, id));
    crate::services::springboot_manager::lifecycle::restart_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
}

#[tauri::command]
pub async fn replace_springboot_jar(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    use chrono::Local;
    use std::path::Path;

    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    oplog!("springboot_replace_jar", &format!("{} ({})", app.name, id));
    if app.status == crate::models::springboot::AppStatus::Running {
        return Err("运行中的应用不可换包".to_string());
    }

    let new_path = Path::new(&new_jar_path);
    if !new_path.exists() {
        return Err("新 JAR 文件不存在".to_string());
    }

    let old_jar = Path::new(&app.jar_path);
    if !old_jar.exists() {
        return Err("原 JAR 文件不存在".to_string());
    }

    // 备份：{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak
    let backup_dir = crate::utils::paths::data_dir()
        .join("backups")
        .join(&app.name);
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let fname = old_jar.file_name().unwrap().to_str().unwrap();
    let backup_path = backup_dir.join(format!("{}.{}.bak", fname, timestamp));

    std::fs::copy(old_jar, &backup_path).map_err(|e| format!("备份失败: {}", e))?;

    // 替换
    std::fs::copy(new_path, old_jar).map_err(|e| format!("替换 JAR 失败: {}", e))?;

    // 读取新版本
    let new_version = crate::services::springboot_manager::read_jar_version(new_path.to_str().unwrap())
        .unwrap_or_else(|| "unknown".to_string());
    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;

    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap().to_string(),
        old_version: app.version,
        new_version,
    })
}

#[tauri::command]
pub async fn get_springboot_jvm_metrics(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    id: String,
) -> Result<Option<JvmInfo>, String> {
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    if let Some(pid) = app.pid {
        // ponytail: 从 JDK 目录找 jcmd，不用 PATH
        let jdk_path = software_mgr.find_installed(&app.jdk_installed_id).map(|j| j.install_path.clone());
        Ok(crate::services::springboot_manager::monitor::collect_jvm_metrics(pid, jdk_path))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn list_springboot_groups(
    manager: State<'_, Arc<SpringBootManager>>,
) -> Result<Vec<AppGroup>, String> {
    Ok(manager.list_groups())
}

#[tauri::command]
pub async fn save_springboot_groups(
    manager: State<'_, Arc<SpringBootManager>>,
    groups: Vec<AppGroup>,
) -> Result<(), String> {
    oplog!("springboot_save_groups", &format!("{} groups", groups.len()));
    manager.save_groups(groups).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_springboot_global_env_vars(
    manager: State<'_, Arc<SpringBootManager>>,
) -> Result<Vec<(String, String)>, String> {
    Ok(manager.get_global_env_vars())
}

#[tauri::command]
pub async fn set_springboot_global_env_vars(
    manager: State<'_, Arc<SpringBootManager>>,
    env_vars: Vec<(String, String)>,
) -> Result<(), String> {
    oplog!("springboot_set_global_env", &format!("{} vars", env_vars.len()));
    manager.set_global_env_vars(env_vars).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recommended_jvm_opts(
    jdk_installed_id: String,
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<JvmOptsTemplate, String> {
    let jdk = software_mgr
        .find_installed(&jdk_installed_id)
        .ok_or("所选 JDK 未找到")?;
    let version = jvm_opts::detect_jdk_version(&jdk.install_path)
        .ok_or("无法检测 JDK 版本")?;
    Ok(jvm_opts::generate_opts(version))
}

#[tauri::command]
pub async fn list_springboot_dependency_candidates(
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<crate::models::software::InstalledSoftware>, String> {
// ponytail: inlined deps::list_dependency_candidates
    let managed_keys = ["mysql", "redis", "nginx", "minio"];
    Ok(software_mgr.get_installed()
        .into_iter()
        .filter(|s| managed_keys.contains(&s.key.as_str()))
        .collect())
}

#[tauri::command]
pub async fn read_jar_version_info(
    jar_path: String,
) -> Result<String, String> {
    Ok(crate::services::springboot_manager::read_jar_version(&jar_path)
        .unwrap_or_else(|| "unknown".to_string()))
}

#[tauri::command]
pub async fn read_jar_port(
    jar_path: String,
) -> Result<Option<u16>, String> {
    Ok(crate::services::springboot_manager::read_port_from_jar(&jar_path))
}

/// ponytail: tail -f 风格，前端传 offset 增量读取，首次传 0 读尾部 64KB
#[derive(serde::Serialize)]
pub struct LogChunk { pub lines: Vec<String>, pub offset: u64 }

#[tauri::command]
pub async fn read_springboot_log(path: String, offset: u64) -> Result<LogChunk, String> {
    use std::io::{Read, Seek, SeekFrom};
    // ponytail: 相对路径解析为绝对路径（log_path 在 apps.json 中存的是相对 data_dir 的路径）
    let abs_path = crate::utils::paths::resolve_data_path(&path);
    let p = abs_path.as_path();
    // ponytail: 精确文件不存在时递归找最新 .log（Spring Boot 可能在子目录）
    let p = if p.exists() { p.to_path_buf() } else if let Some(dir) = p.parent().filter(|d| d.exists()) {
        let mut best: Option<(std::path::PathBuf, u64)> = None;
        for entry in walkdir::WalkDir::new(dir).max_depth(5).into_iter().filter_map(|e| e.ok()) {
            if entry.path().extension().and_then(|x| x.to_str()) == Some("log") {
                let age = entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
                if best.as_ref().map_or(true, |&(_, a)| age < a) { best = Some((entry.path().to_path_buf(), age)); }
            }
        }
        best.map(|(p, _)| p).unwrap_or_else(|| p.to_path_buf())
    } else { p.to_path_buf() };
    if !p.exists() { return Ok(LogChunk { lines: vec!["日志文件尚未生成".to_string()], offset: 0 }); }
    let mut f = std::fs::File::open(&p).map_err(|e| format!("打开失败: {}", e))?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if offset >= len { return Ok(LogChunk { lines: vec![], offset }); }
    if offset == 0 {
        // 首次：读尾部 64KB
        let skip = len.saturating_sub(65536);
        let mut buf = vec![0u8; (len - skip) as usize];
        f.seek(SeekFrom::Start(skip)).map_err(|e| format!("seek: {}", e))?;
        f.read_exact(&mut buf).map_err(|e| format!("read: {}", e))?;
        let content = String::from_utf8_lossy(&buf);
        let ls: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        Ok(LogChunk { lines: ls, offset: len })
    } else {
        // ponytail: 增量读取 ── 真正的 tail -f
        f.seek(SeekFrom::Start(offset)).map_err(|e| format!("seek: {}", e))?;
        let size = len - offset;
        let mut buf = vec![0u8; size.min(65536) as usize];
        f.read_exact(&mut buf).map_err(|e| format!("read: {}", e))?;
        let content = String::from_utf8_lossy(&buf);
        let ls: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        Ok(LogChunk { lines: ls, offset: len })
    }
}

/// 导出应用（按分组过滤）到 zip 文件，不含日志目录
#[tauri::command]
pub async fn export_springboot_config(
    app_handle: AppHandle,
    manager: State<'_, Arc<SpringBootManager>>,
    file_path: String,
    group_names: Option<Vec<String>>,
) -> Result<(), String> {
    let all_apps = manager.export_apps();
    let groups = manager.list_groups();
    let env_vars = manager.get_global_env_vars();

    // 按分组过滤
    let apps: Vec<&SpringBootApp> = if let Some(ref names) = group_names {
        all_apps.iter().filter(|a| a.group.as_deref().map_or(false, |g| names.iter().any(|n| n == g))).collect()
    } else {
        all_apps.iter().collect()
    };

    let manifest = serde_json::json!({
        "apps": apps,
        "groups": groups,
        "global_env_vars": env_vars,
    });
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;

    let f = std::fs::File::create(&file_path).map_err(|e| format!("ERR_WRITE:创建文件失败: {}", e))?;
    let mut zip = zip::ZipWriter::new(f);
    let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("manifest.json", opts).map_err(|e| format!("ERR_ZIP:{}", e))?;
    zip.write_all(manifest_json.as_bytes()).map_err(|e| format!("ERR_ZIP:{}", e))?;

    let total = apps.len();
    for (i, app) in apps.iter().enumerate() {
        let _ = app_handle.emit("export-progress", serde_json::json!({ "current": i + 1, "total": total, "name": app.name }));

        // ponytail: jar_path 是相对 data_dir 的相对路径，需转绝对路径
        let jar = std::path::PathBuf::from(crate::utils::paths::data_dir()).join(&app.jar_path);
        if !jar.exists() { continue; }
        let app_home = jar.parent().unwrap_or(&jar);
        let app_dir_name = format!("apps/{}", sanitize_name(&app.name));
        let log_canonical = {
            let lp = Path::new(&app.log_path);
            if lp.exists() { lp.canonicalize().ok() } else { None }
        };
        add_dir_to_zip(&mut zip, app_home, &app_dir_name, log_canonical.as_deref(), opts)
            .map_err(|e| format!("ERR_ZIP:{}({}):{}", app.name, app.id, e))?;
    }

    let mut f = zip.finish().map_err(|e| format!("ERR_ZIP:{}", e))?;
    f.sync_all().map_err(|e| format!("ERR_ZIP:{}", e))?;
    let _ = app_handle.emit("export-progress", serde_json::json!({ "done": true }));
    Ok(())
}

/// 从 zip 文件导入应用配置和数据
#[tauri::command]
pub async fn import_springboot_config(
    app_handle: AppHandle,
    manager: State<'_, Arc<SpringBootManager>>,
    file_path: String,
) -> Result<(), String> {
    let _ = app_handle.emit("import-progress", serde_json::json!({ "phase": "extracting" }));
    let tmp_dir = std::env::temp_dir().join(format!("opx_import_{}", std::time::UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos()));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("ERR_TMP:{}", e))?;

    let f = std::fs::File::open(&file_path).map_err(|e| format!("ERR_READ:读取文件失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("ERR_ZIP_PARSE:文件格式错误: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("ERR_ZIP:{}", e))?;
        let out_path = tmp_dir.join(sanitize_zip_path(entry.name()));
        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
            }
            let mut outfile = std::fs::File::create(&out_path).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
        }
    }

    let _ = app_handle.emit("import-progress", serde_json::json!({ "phase": "config" }));
    let manifest_content = std::fs::read_to_string(&tmp_dir.join("manifest.json"))
        .map_err(|e| format!("ERR_IMPORT:manifest.json 不存在或无法读取: {}", e))?;
    let data: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| format!("ERR_IMPORT:manifest.json 格式错误: {}", e))?;

    use serde_json::Value;
    if let Some(apps) = data.get("apps").and_then(|v| v.as_array()) {
        let existing = manager.list_apps();
        for app_val in apps {
            let imported: SpringBootApp = serde_json::from_value(app_val.clone())
                .map_err(|e| format!("ERR_IMPORT:应用数据错误: {}", e))?;

            // 复制应用数据
            let app_data_dir = tmp_dir.join("apps").join(sanitize_name(&imported.name));
            if app_data_dir.exists() {
                let jar = Path::new(&imported.jar_path);
                if let Some(target) = jar.parent() {
                    copy_dir_all(&app_data_dir, target)
                        .map_err(|e| format!("ERR_COPY:复制应用数据失败: {}", e))?;
                }
            }

            // ponytail: 按名称匹配（应用名称唯一），id 随机器不同
            let existing_app = existing.iter().find(|a| a.name == imported.name);
            if let Some(existing) = existing_app {
                manager.update_app(&existing.id, UpdateAppParams {
                    name: Some(imported.name),
                    jdk_installed_id: Some(imported.jdk_installed_id),
                    jvm_opts: Some(imported.jvm_opts),
                    program_args: Some(imported.program_args),
                    profile: Some(imported.profile),
                    env_vars: Some(imported.env_vars),
                    port: imported.port,
                    log_path: Some(imported.log_path),
                    dependencies: Some(imported.dependencies),
                    auto_start: Some(imported.auto_start),
                    startup_order: Some(imported.startup_order),
                    auto_restart: Some(imported.auto_restart),
                    group: Some(imported.group),
                    jdk_type: Some(imported.jdk_type),
                }).map_err(|e| e.to_string())?;
            } else {
                manager.create_app(CreateAppParams {
                    name: imported.name,
                    jar_path: imported.jar_path,
                    jdk_installed_id: imported.jdk_installed_id,
                    jvm_opts: imported.jvm_opts,
                    program_args: imported.program_args,
                    profile: imported.profile,
                    env_vars: imported.env_vars,
                    port: imported.port,
                    log_path: imported.log_path,
                    dependencies: imported.dependencies,
                    auto_start: imported.auto_start,
                    startup_order: imported.startup_order,
                    auto_restart: imported.auto_restart,
                    group: imported.group,
                    jdk_type: imported.jdk_type,
                }).map_err(|e| e.to_string())?;
            }
        }
    }
    if let Some(groups) = data.get("groups").and_then(|v| v.as_array()) {
        let parsed: Vec<AppGroup> = serde_json::from_value(Value::Array(groups.clone())).map_err(|e| format!("ERR_IMPORT:分组错误: {}", e))?;
        manager.save_groups(parsed).map_err(|e| e.to_string())?;
    }
    if let Some(env_vars) = data.get("global_env_vars") {
        let parsed: Vec<(String, String)> = serde_json::from_value(env_vars.clone()).map_err(|e| format!("ERR_IMPORT:环境变量错误: {}", e))?;
        manager.set_global_env_vars(parsed).map_err(|e| e.to_string())?;
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
    let _ = app_handle.emit("import-progress", serde_json::json!({ "done": true }));
    Ok(())
}

/// 将目录递归添加到 zip，跳过 excluded_dir
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    prefix: &str,
    exclude: Option<&Path>,
    opts: zip::write::FileOptions,
) -> Result<(), String> {
    if !src.is_dir() {
        if exclude.map_or(true, |e| !is_parent_or_self(e, src)) {
            let name = format!("{}/{}", prefix, src.file_name().unwrap_or_default().to_string_lossy());
            zip.start_file(&name, opts).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            std::fs::File::open(src).map_err(|e| e.to_string())?.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if let Some(ex) = exclude {
            if is_parent_or_self(ex, &path) { continue; }
        }
        let rel_name = entry.file_name().to_string_lossy().to_string();
        let zip_name = format!("{}/{}", prefix, rel_name);

        if path.is_dir() {
            zip.add_directory(&format!("{}/", &zip_name), opts).map_err(|e| e.to_string())?;
            add_dir_to_zip(zip, &path, &zip_name, exclude, opts)?;
        } else {
            zip.start_file(&zip_name, opts).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            std::fs::File::open(&path).map_err(|e| e.to_string())?.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 判断 parent 是否是 path 的父目录或自身
fn is_parent_or_self(parent: &Path, path: &Path) -> bool {
    if let Ok(canon_parent) = parent.canonicalize() {
        if let Ok(canon_path) = path.canonicalize() {
            return canon_path.starts_with(&canon_parent);
        }
    }
    path.starts_with(parent)
}

/// 安全文件名（去除非字母数字字符）
fn sanitize_name(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

/// 安全路径（防止 zip slip）
fn sanitize_zip_path(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches('/').to_string()
}

/// 递归复制目录
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(&entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
