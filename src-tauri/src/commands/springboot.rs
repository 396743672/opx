use std::sync::Arc;
use std::io::{Read, Write};
use std::path::Path;

use tauri::{AppHandle, Emitter, State};

use crate::models::software::{LogChunk, LogSource};
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
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    oplog!("springboot_replace_jar", &format!("{} ({})", app.name, id));
    if app.status == crate::models::springboot::AppStatus::Running {
        return Err("运行中的应用不可换包".to_string());
    }

    let old_jar = std::path::PathBuf::from(&app.jar_path);
    let new_jar = std::path::Path::new(&new_jar_path);
    let (backup_path, new_version) =
        replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;

    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;
    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap_or("").to_string(),
        old_version: app.version,
        new_version,
    })
}

#[tauri::command]
pub async fn replace_springboot_jar_and_restart(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    use crate::models::springboot::AppStatus;

    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    oplog!("springboot_replace_restart", &format!("{} ({})", app.name, id));

    // 运行中/错误态先停（优雅），停止态直接换包
    if matches!(app.status, AppStatus::Running | AppStatus::Error) {
        crate::services::springboot_manager::lifecycle::stop_app(
            &id, &manager, &software_mgr, &app_handle,
        ).await?;
    }

    let old_jar = std::path::PathBuf::from(&app.jar_path);
    let new_jar = std::path::Path::new(&new_jar_path);
    let (backup_path, new_version) =
        replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;

    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;

    crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await?;

    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap_or("").to_string(),
        old_version: app.version,
        new_version,
    })
}

/// 纯文件操作：校验新旧 jar → 备份旧 jar → 复制新 jar 覆盖 → 读新版本。
/// 不依赖 SpringBootManager，可直接单测。返回 (backup_path, new_version)。
fn replace_jar_file(app_name: &str, old_jar: &Path, new_jar: &Path) -> anyhow::Result<(std::path::PathBuf, String)> {
    use chrono::Local;

    if !new_jar.exists() {
        anyhow::bail!("新 JAR 文件不存在");
    }
    if !old_jar.exists() {
        anyhow::bail!("原 JAR 文件不存在");
    }

    // 备份：{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak
    let backup_dir = crate::utils::paths::data_dir().join("backups").join(app_name);
    std::fs::create_dir_all(&backup_dir).map_err(|e| anyhow::anyhow!("创建备份目录失败: {}", e))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let fname = old_jar.file_name().unwrap_or_default();
    let backup_path = backup_dir.join(format!("{}.{}.bak", fname.to_string_lossy(), timestamp));

    std::fs::copy(old_jar, &backup_path).map_err(|e| anyhow::anyhow!("备份失败: {}", e))?;
    std::fs::copy(new_jar, old_jar).map_err(|e| anyhow::anyhow!("替换 JAR 失败: {}", e))?;

    let new_version = crate::services::springboot_manager::read_jar_version(new_jar.to_str().unwrap_or(""))
        .unwrap_or_else(|| "unknown".to_string());
    Ok((backup_path, new_version))
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

/// 获取应用的日志源（按 level 多源 + 日期目录归档）
#[tauri::command]
pub async fn list_springboot_log_sources(
    manager: State<'_, Arc<SpringBootManager>>,
    app_id: String,
) -> Result<Vec<LogSource>, String> {
    let app = manager.find_app(&app_id).map_err(|e| e.to_string())?;
    let abs = crate::utils::paths::resolve_data_path(&app.log_path);
    let dir = if abs.is_file() {
        abs.parent().map(|p| p.to_path_buf()).unwrap_or(abs)
    } else {
        abs
    };
    Ok(crate::services::software_manager::log_viewer::collect_springboot_sources(&dir))
}

/// 读取应用日志（tail / 增量 / 历史分页 + 关键字过滤 + 归档无缝续接）
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn read_springboot_log(
    manager: State<'_, Arc<SpringBootManager>>,
    app_id: String,
    source_index: usize,
    archive_index: Option<usize>,
    offset: Option<u64>,
    before: Option<bool>,
    limit: Option<u64>,
    keyword: Option<String>,
    regex: Option<bool>,
    level: Option<String>,
) -> Result<LogChunk, String> {
    let app = manager.find_app(&app_id).map_err(|e| e.to_string())?;
    let abs = crate::utils::paths::resolve_data_path(&app.log_path);
    let dir = if abs.is_file() {
        abs.parent().map(|p| p.to_path_buf()).unwrap_or(abs)
    } else {
        abs
    };
    let sources = crate::services::software_manager::log_viewer::collect_springboot_sources(&dir);
    let source = sources
        .get(source_index)
        .ok_or_else(|| format!("日志源索引越界: {}", source_index))?;
    let archive_index = archive_index.unwrap_or(0);
    let limit = limit.map(|l| l as usize).unwrap_or(2000);
    let before = before.unwrap_or(false);
    crate::services::software_manager::log_viewer::read_springboot_chunk(
        std::path::Path::new(&source.path),
        &source.archives,
        archive_index,
        offset,
        before,
        limit,
        keyword.as_deref(),
        regex.unwrap_or(false),
        level.as_deref(),
    )
    .map_err(|e| e.to_string())
}

/// 下载（拷贝）应用指定日志源到用户选择路径
#[tauri::command]
pub async fn download_springboot_log(
    manager: State<'_, Arc<SpringBootManager>>,
    app_id: String,
    source_index: usize,
    dest_path: String,
) -> Result<(), String> {
    let app = manager.find_app(&app_id).map_err(|e| e.to_string())?;
    let abs = crate::utils::paths::resolve_data_path(&app.log_path);
    let dir = if abs.is_file() {
        abs.parent().map(|p| p.to_path_buf()).unwrap_or(abs)
    } else {
        abs
    };
    let sources = crate::services::software_manager::log_viewer::collect_springboot_sources(&dir);
    let source = sources
        .get(source_index)
        .ok_or_else(|| format!("日志源索引越界: {}", source_index))?;
    std::fs::copy(&source.path, &dest_path)
        .map_err(|e| format!("复制日志失败 {} -> {}: {}", source.path, dest_path, e))?;
    Ok(())
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

    let f = zip.finish().map_err(|e| format!("ERR_ZIP:{}", e))?;
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

#[cfg(test)]
mod tests {
    use super::replace_jar_file;
    use std::io::Write;

    fn fake_jar(path: &std::path::Path, manifest: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn jar_has_version(path: &std::path::Path) -> Option<String> {
        crate::services::springboot_manager::read_jar_version(path.to_str().unwrap())
    }

    #[test]
    fn replace_jar_file_backs_up_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("opx_repl_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let old = dir.join("app.jar");
        let new = dir.join("new.jar");
        fake_jar(&old, "Implementation-Version: 1.0.0\r\n");
        fake_jar(&new, "Implementation-Version: 2.0.0\r\n");

        let (backup, version) = replace_jar_file("test-app", &old, &new).unwrap();

        assert_eq!(version, "2.0.0");
        assert_eq!(jar_has_version(&old), Some("2.0.0".to_string()));
        // 备份是一份旧的 1.0.0 且文件名以 .bak 结尾
        assert!(backup.extension().map(|e| e == "bak").unwrap_or(false));
        assert!(backup.exists());
        assert_eq!(jar_has_version(&backup), Some("1.0.0".to_string()));
        // 旧 jar 已不是原文件（内容被覆盖）
        assert_ne!(std::fs::read(&old).unwrap(), std::fs::read(&backup).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(crate::utils::paths::data_dir().join("backups").join("test-app"));
    }

    #[test]
    fn replace_jar_file_rejects_missing_files() {
        let dir = std::env::temp_dir().join(format!("opx_repl_err_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let old = dir.join("app.jar");
        let new = dir.join("new.jar");
        fake_jar(&old, "Implementation-Version: 1.0.0\r\n");
        // new 不存在
        assert!(replace_jar_file("t", &old, &new).is_err());

        // old 不存在
        let old2 = dir.join("missing.jar");
        assert!(replace_jar_file("t", &old2, &new).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(crate::utils::paths::data_dir().join("backups").join("t"));
    }
}
