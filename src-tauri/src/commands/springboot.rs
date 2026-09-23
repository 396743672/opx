use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::models::software::{LogChunk, LogSource};
use crate::models::springboot::{
    AppGroup, CreateAppParams, JarInfo, JvmInfo, JvmOptsTemplate, ReplaceResult, SpringBootApp,
    UpdateAppParams,
};
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::jvm_opts;
use crate::services::springboot_manager::lifecycle::StopOutcome;
use crate::services::springboot_manager::SpringBootManager;
use crate::{audited_async, oplog_result};

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
    let target = params.name.clone();
    audited_async!("springboot_create", target, "", {
        // ponytail: 有指定端口才查重，None 表示动态端口不校验
        if let Some(port) = params.port {
            let apps = manager.list_apps();
            if apps.iter().any(|a| a.port == Some(port)) {
                return Err(format!("端口 {} 已被其他应用占用", port));
            }
        }
        manager.create_app(params).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn update_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    params: UpdateAppParams,
) -> Result<SpringBootApp, String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = manager.update_app(&id, params).map_err(|e| e.to_string());
    oplog_result!("springboot_update", target, "", r);
    r
}

#[tauri::command]
pub async fn delete_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = manager.delete_app(&id).map_err(|e| e.to_string());
    oplog_result!("springboot_delete", target, "", r);
    r
}

#[tauri::command]
pub async fn start_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = crate::services::springboot_manager::lifecycle::start_app(
        &id,
        &manager,
        &software_mgr,
        &app_handle,
    )
    .await;
    oplog_result!("springboot_start", target, "", r);
    r
}

#[tauri::command]
pub async fn stop_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<StopOutcome, String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = crate::services::springboot_manager::lifecycle::stop_app(&id, &manager, &app_handle)
        .await;
    oplog_result!("springboot_stop", target, "", r);
    r
}

#[tauri::command]
pub async fn restart_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<StopOutcome, String> {
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = crate::services::springboot_manager::lifecycle::restart_app(
        &id,
        &manager,
        &software_mgr,
        &app_handle,
    )
    .await;
    oplog_result!("springboot_restart", target, "", r);
    r
}

#[tauri::command]
pub async fn replace_springboot_jar(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    let target = format!("{} ({})", app.name, id);
    audited_async!("springboot_replace_jar", target, "", {
        if app.status == crate::models::springboot::AppStatus::Running {
            return Err("运行中的应用不可换包".to_string());
        }
        let old_jar = std::path::PathBuf::from(&app.jar_path);
        let new_jar = std::path::Path::new(&new_jar_path);
        let (backup_path, new_version) =
            replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;
        manager
            .update_version(&id, new_version.clone())
            .map_err(|e| e.to_string())?;
        Ok(ReplaceResult {
            backup_path: backup_path.to_str().unwrap_or("").to_string(),
            old_version: app.version,
            new_version,
        })
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
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    let target = format!("{} ({})", app.name, id);
    audited_async!("springboot_replace_restart", target, "", {
        use crate::models::springboot::AppStatus;

        // 运行中/错误态先停（优雅），停止态直接换包。
        // 停止成功但走了强杀时只记日志：换包流程必须继续往下走。
        if matches!(app.status, AppStatus::Running | AppStatus::Error) {
            let stop = crate::services::springboot_manager::lifecycle::stop_app(
                &id,
                &manager,
                &app_handle,
            )
            .await?;
            if let Some(w) = stop.message {
                tracing::warn!(app_id = %id, warning = %w, "换包前停止未走优雅路径");
            }
        }

        let old_jar = std::path::PathBuf::from(&app.jar_path);
        let new_jar = std::path::Path::new(&new_jar_path);
        let (backup_path, new_version) =
            replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;

        manager
            .update_version(&id, new_version.clone())
            .map_err(|e| e.to_string())?;

        crate::services::springboot_manager::lifecycle::start_app(
            &id,
            &manager,
            &software_mgr,
            &app_handle,
        )
        .await?;

        Ok(ReplaceResult {
            backup_path: backup_path.to_str().unwrap_or("").to_string(),
            old_version: app.version,
            new_version,
        })
    })
}

/// 纯文件操作：校验新旧 jar → 备份旧 jar → 复制新 jar 覆盖 → 读新版本。
/// 不依赖 SpringBootManager，可直接单测。返回 (backup_path, new_version)。
fn replace_jar_file(
    app_name: &str,
    old_jar: &Path,
    new_jar: &Path,
) -> anyhow::Result<(std::path::PathBuf, String)> {
    use chrono::Local;

    if !new_jar.exists() {
        anyhow::bail!("新 JAR 文件不存在");
    }
    if !old_jar.exists() {
        anyhow::bail!("原 JAR 文件不存在");
    }

    // 备份：{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak
    let backup_dir = crate::utils::paths::data_dir()
        .join("backups")
        .join(app_name);
    std::fs::create_dir_all(&backup_dir).map_err(|e| anyhow::anyhow!("创建备份目录失败: {}", e))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let fname = old_jar.file_name().unwrap_or_default();
    let backup_path = backup_dir.join(format!("{}.{}.bak", fname.to_string_lossy(), timestamp));

    std::fs::copy(old_jar, &backup_path).map_err(|e| anyhow::anyhow!("备份失败: {}", e))?;
    std::fs::copy(new_jar, old_jar).map_err(|e| anyhow::anyhow!("替换 JAR 失败: {}", e))?;

    let new_version =
        crate::services::springboot_manager::read_jar_version(new_jar.to_str().unwrap_or(""))
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
        // 进程已退出（弹窗开着时应用被停止是正常操作）→ None，前端静默等待；
        // 进程还在但采集失败 → Err 带真实原因，前端直接展示，不再猜「缺 JDK」
        if !crate::services::software_manager::health_check::is_process_alive(pid) {
            return Ok(None);
        }
        // ponytail: 从 JDK 目录找 jcmd，不用 PATH
        let jdk_path = software_mgr
            .find_installed(&app.jdk_installed_id)
            .map(|j| j.install_path.clone());
        return crate::services::springboot_manager::monitor::collect_jvm_metrics(pid, jdk_path)
            .map(Some);
    }
    Ok(None)
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
    let target = format!("{} groups", groups.len());
    let r = manager.save_groups(groups).map_err(|e| e.to_string());
    oplog_result!("springboot_save_groups", target, "", r);
    r
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
    let target = format!("{} vars", env_vars.len());
    let r = manager
        .set_global_env_vars(env_vars)
        .map_err(|e| e.to_string());
    oplog_result!("springboot_set_global_env", target, "", r);
    r
}

#[tauri::command]
pub async fn get_recommended_jvm_opts(
    jdk_installed_id: String,
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<JvmOptsTemplate, String> {
    let jdk = software_mgr
        .find_installed(&jdk_installed_id)
        .ok_or("所选 JDK 未找到")?;
    let version = jvm_opts::detect_jdk_version(&jdk.install_path).ok_or("无法检测 JDK 版本")?;
    Ok(jvm_opts::generate_opts(version))
}

#[tauri::command]
pub async fn list_springboot_dependency_candidates(
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<crate::models::software::InstalledSoftware>, String> {
    // ponytail: inlined deps::list_dependency_candidates
    let managed_keys = ["mysql", "redis", "nginx", "minio"];
    Ok(software_mgr
        .get_installed()
        .into_iter()
        .filter(|s| managed_keys.contains(&s.key.as_str()))
        .collect())
}

/// 读取 JAR 元信息：应用版本 + 构建该 JAR 的 Spring Boot 版本 + 所需最低 JDK。
///
/// 前端在选择 jar 之后调用，用于显示「Spring Boot 3.5.11 · 需 JDK 17+」这类提示。
/// Spring Boot 3.x/4.x 要求 Java 17，选错 JDK 会直接 `UnsupportedClassVersionError`，
/// 在表单里提前提示比事后排查日志省事得多。
///
/// 注意框架版本取自 MANIFEST 的 `Spring-Boot-Version`（repackage 自动写入，三版都有），
/// 而不是 `Implementation-Version`——后者受 Maven `addDefaultImplementationEntries`
/// 控制、默认为 false，多数可执行 jar 里根本没有。
#[tauri::command]
pub async fn read_jar_info(jar_path: String) -> Result<JarInfo, String> {
    use crate::services::springboot_manager as sb;
    let spring_boot_version = sb::read_spring_boot_version(&jar_path);
    let min_jdk = spring_boot_version
        .as_deref()
        .and_then(sb::parse_spring_boot_major)
        .and_then(sb::min_jdk_for_spring_boot);
    Ok(JarInfo {
        version: sb::read_jar_version(&jar_path),
        spring_boot_version,
        min_jdk,
    })
}

#[tauri::command]
pub async fn read_jar_port(jar_path: String) -> Result<Option<u16>, String> {
    Ok(crate::services::springboot_manager::read_port_from_jar(
        &jar_path,
    ))
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

/// 导出结果摘要（供前端提示成功/警告）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportSummary {
    /// 实际导出（含 JAR 与应用目录）的应用数
    pub apps: usize,
    /// 打包进 zip 的文件数（不含 manifest.json）
    pub files: usize,
    /// 跳过或异常的应用说明（如 JAR 缺失）
    pub warnings: Vec<String>,
}

/// 导出整个应用目录（含 JAR、配置等，**排除日志目录**）到 zip 文件。
/// `group_names` 为 None 时导出全部应用。
#[tauri::command]
pub async fn export_springboot_config(
    app_handle: AppHandle,
    manager: State<'_, Arc<SpringBootManager>>,
    file_path: String,
    group_names: Option<Vec<String>>,
) -> Result<ExportSummary, String> {
    let all_apps = manager.export_apps();
    let groups = manager.list_groups();
    let env_vars = manager.get_global_env_vars();

    // 按分组过滤
    let apps: Vec<&SpringBootApp> = if let Some(ref names) = group_names {
        all_apps
            .iter()
            .filter(|a| {
                a.group
                    .as_deref()
                    .map_or(false, |g| names.iter().any(|n| n == g))
            })
            .collect()
    } else {
        all_apps.iter().collect()
    };

    let manifest = serde_json::json!({
        "apps": apps,
        "groups": groups,
        "global_env_vars": env_vars,
    });
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;

    let f =
        std::fs::File::create(&file_path).map_err(|e| format!("ERR_WRITE:创建文件失败: {}", e))?;
    let mut zip = zip::ZipWriter::new(f);
    let opts =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("manifest.json", opts)
        .map_err(|e| format!("ERR_ZIP:{}", e))?;
    zip.write_all(manifest_json.as_bytes())
        .map_err(|e| format!("ERR_ZIP:{}", e))?;

    let total = apps.len();
    let mut exported = 0usize;
    let mut files = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    for (i, app) in apps.iter().enumerate() {
        let _ = app_handle.emit(
            "export-progress",
            serde_json::json!({ "current": i + 1, "total": total, "name": app.name }),
        );

        // jar_path 兼容相对（springboot/<name>/app.jar）与绝对两种历史写法
        let jar = crate::utils::paths::resolve_data_path(&app.jar_path);
        if !jar.exists() {
            // 不静默跳过：JAR 缺失时明确告知，否则用户解压后只看到一份 manifest.json
            warnings.push(format!("{}: JAR 文件缺失，已跳过", app.name));
            continue;
        }
        let app_home = jar.parent().unwrap_or(&jar);
        let app_dir_name = format!("apps/{}", sanitize_name(&app.name));

        // 排除日志目录：log_path 是相对 data_dir 的路径，必须先 resolve 再 canonicalize，
        // 否则按进程 CWD 判断必然不存在 → 排除失效、日志被打包。
        // 取 log_path 的父目录（logs/）以排除整个日志目录；并加护栏避免把应用根目录整个排除。
        let log_canonical = {
            let lp = crate::utils::paths::resolve_data_path(&app.log_path);
            let dir = if lp.is_dir() {
                Some(lp)
            } else {
                lp.parent().map(|p| p.to_path_buf())
            };
            let candidate = dir.filter(|d| d.exists()).and_then(|d| d.canonicalize().ok());
            match (candidate, app_home.canonicalize()) {
                (Some(ex), Ok(root)) if ex == root || root.starts_with(&ex) => None,
                (c, _) => c,
            }
        };

        files += add_dir_to_zip(
            &mut zip,
            app_home,
            &app_dir_name,
            log_canonical.as_deref(),
            opts,
        )
        .map_err(|e| format!("ERR_ZIP:{}({}):{}", app.name, app.id, e))?;
        exported += 1;
    }

    let f = zip.finish().map_err(|e| format!("ERR_ZIP:{}", e))?;
    f.sync_all().map_err(|e| format!("ERR_ZIP:{}", e))?;
    let _ = app_handle.emit("export-progress", serde_json::json!({ "done": true }));
    Ok(ExportSummary {
        apps: exported,
        files,
        warnings,
    })
}

/// 导入结果摘要（供前端提示成功/警告）
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportSummary {
    /// 实际导入或更新的应用数
    pub apps: usize,
    /// 跳过或异常的应用说明
    pub warnings: Vec<String>,
}

/// 从 zip 文件导入应用（含 JAR 与应用目录）与全局配置
#[tauri::command]
pub async fn import_springboot_config(
    app_handle: AppHandle,
    manager: State<'_, Arc<SpringBootManager>>,
    file_path: String,
) -> Result<ImportSummary, String> {
    let _ = app_handle.emit(
        "import-progress",
        serde_json::json!({ "phase": "extracting" }),
    );
    let tmp_dir = std::env::temp_dir().join(format!(
        "opx_import_{}",
        std::time::UNIX_EPOCH
            .elapsed()
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("ERR_TMP:{}", e))?;

    let f = std::fs::File::open(&file_path).map_err(|e| format!("ERR_READ:读取文件失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(f).map_err(|e| format!("ERR_ZIP_PARSE:文件格式错误: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("ERR_ZIP:{}", e))?;
        let out_path = tmp_dir.join(sanitize_zip_path(entry.name()));
        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
            }
            let mut outfile =
                std::fs::File::create(&out_path).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| format!("ERR_EXTRACT:{}", e))?;
        }
    }

    let _ = app_handle.emit("import-progress", serde_json::json!({ "phase": "config" }));
    let manifest_content = std::fs::read_to_string(&tmp_dir.join("manifest.json"))
        .map_err(|e| format!("ERR_IMPORT:manifest.json 不存在或无法读取: {}", e))?;
    let data: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| format!("ERR_IMPORT:manifest.json 格式错误: {}", e))?;

    use serde_json::Value;
    let mut imported_count = 0usize;
    let mut warnings: Vec<String> = Vec::new();
    if let Some(apps) = data.get("apps").and_then(|v| v.as_array()) {
        let existing = manager.list_apps();
        for app_val in apps {
            let imported: SpringBootApp = serde_json::from_value(app_val.clone())
                .map_err(|e| format!("ERR_IMPORT:应用数据错误: {}", e))?;

            // 恢复到本机数据目录 <data_dir>/springboot/<name>/。
            // 不信任 manifest 里导出机的绝对路径（跨机器必然失效，会把文件写到错误位置）。
            let app_data_dir = tmp_dir.join("apps").join(sanitize_name(&imported.name));
            let local_dir = crate::utils::paths::data_dir()
                .join("springboot")
                .join(&imported.name);
            let has_jar = app_data_dir.join("app.jar").exists();
            if !has_jar {
                warnings.push(format!(
                    "{}: 导出包内未包含 JAR，仅导入了配置",
                    imported.name
                ));
            }
            if app_data_dir.exists() {
                if let Err(e) = copy_dir_all(&app_data_dir, &local_dir) {
                    warnings.push(format!("{}: 应用数据复制失败（{}）", imported.name, e));
                    continue;
                }
            }

            // log_path 归一化为本机相对路径（导出机的外部绝对路径在本机无意义）
            let log_path = {
                let rel = SpringBootManager::relativize_data_path(&imported.log_path);
                if Path::new(&rel).is_absolute() {
                    format!("springboot/{}/logs/console.log", imported.name)
                } else {
                    rel
                }
            };

            // ponytail: 按名称匹配（应用名称唯一），id 随机器不同
            let existing_app = existing.iter().find(|a| a.name == imported.name);
            let outcome = if let Some(existing) = existing_app {
                manager
                    .update_app(
                        &existing.id,
                        UpdateAppParams {
                            name: Some(imported.name.clone()),
                            jdk_installed_id: Some(imported.jdk_installed_id.clone()),
                            jvm_opts: Some(imported.jvm_opts.clone()),
                            program_args: Some(imported.program_args.clone()),
                            profile: Some(imported.profile.clone()),
                            env_vars: Some(imported.env_vars.clone()),
                            port: imported.port,
                            log_path: Some(log_path),
                            dependencies: Some(imported.dependencies.clone()),
                            auto_start: Some(imported.auto_start),
                            startup_order: Some(imported.startup_order),
                            auto_restart: Some(imported.auto_restart),
                            group: Some(imported.group.clone()),
                            jdk_type: Some(imported.jdk_type.clone()),
                            stop_timeout_secs: Some(imported.stop_timeout_secs),
                            actuator_shutdown_url: Some(
                                imported.actuator_shutdown_url.clone().unwrap_or_default(),
                            ),
                        },
                    )
                    .map_err(|e| e.to_string())
            } else {
                // create_app 会把 src 的 jar 复制到 <data_dir>/springboot/<name>/app.jar；
                // 用解压出的 jar 作 src，避免「源 = 目标」同路径复制。
                let src_jar = if has_jar {
                    app_data_dir.join("app.jar")
                } else {
                    local_dir.join("app.jar")
                };
                manager
                    .create_app(CreateAppParams {
                        name: imported.name.clone(),
                        jar_path: src_jar.to_string_lossy().to_string(),
                        jdk_installed_id: imported.jdk_installed_id.clone(),
                        jvm_opts: imported.jvm_opts.clone(),
                        program_args: imported.program_args.clone(),
                        profile: imported.profile.clone(),
                        env_vars: imported.env_vars.clone(),
                        port: imported.port,
                        log_path,
                        dependencies: imported.dependencies.clone(),
                        auto_start: imported.auto_start,
                        startup_order: imported.startup_order,
                        auto_restart: imported.auto_restart,
                        group: imported.group.clone(),
                        jdk_type: imported.jdk_type.clone(),
                        stop_timeout_secs: imported.stop_timeout_secs,
                        actuator_shutdown_url: imported.actuator_shutdown_url.clone(),
                    })
                    .map_err(|e| e.to_string())
            };
            match outcome {
                Ok(_) => imported_count += 1,
                Err(e) => warnings.push(format!("{}: 导入失败（{}）", imported.name, e)),
            }
        }
    }
    if let Some(groups) = data.get("groups").and_then(|v| v.as_array()) {
        let parsed: Vec<AppGroup> = serde_json::from_value(Value::Array(groups.clone()))
            .map_err(|e| format!("ERR_IMPORT:分组错误: {}", e))?;
        manager.save_groups(parsed).map_err(|e| e.to_string())?;
    }
    if let Some(env_vars) = data.get("global_env_vars") {
        let parsed: Vec<(String, String)> = serde_json::from_value(env_vars.clone())
            .map_err(|e| format!("ERR_IMPORT:环境变量错误: {}", e))?;
        manager
            .set_global_env_vars(parsed)
            .map_err(|e| e.to_string())?;
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
    let _ = app_handle.emit("import-progress", serde_json::json!({ "done": true }));
    Ok(ImportSummary {
        apps: imported_count,
        warnings,
    })
}

/// 将目录递归添加到 zip，跳过 excluded_dir。返回打包的文件数。
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src: &Path,
    prefix: &str,
    exclude: Option<&Path>,
    opts: zip::write::FileOptions,
) -> Result<usize, String> {
    if !src.is_dir() {
        if exclude.map_or(true, |e| !is_parent_or_self(e, src)) {
            let name = format!(
                "{}/{}",
                prefix,
                src.file_name().unwrap_or_default().to_string_lossy()
            );
            zip.start_file(&name, opts).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            std::fs::File::open(src)
                .map_err(|e| e.to_string())?
                .read_to_end(&mut buf)
                .map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
            return Ok(1);
        }
        return Ok(0);
    }

    let mut count = 0usize;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if let Some(ex) = exclude {
            if is_parent_or_self(ex, &path) {
                continue;
            }
        }
        let rel_name = entry.file_name().to_string_lossy().to_string();
        let zip_name = format!("{}/{}", prefix, rel_name);

        if path.is_dir() {
            zip.add_directory(&format!("{}/", &zip_name), opts)
                .map_err(|e| e.to_string())?;
            count += add_dir_to_zip(zip, &path, &zip_name, exclude, opts)?;
        } else {
            zip.start_file(&zip_name, opts).map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .read_to_end(&mut buf)
                .map_err(|e| e.to_string())?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
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
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
        let opts =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
        assert_ne!(
            std::fs::read(&old).unwrap(),
            std::fs::read(&backup).unwrap()
        );

        std::fs::remove_dir_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(
            crate::utils::paths::data_dir()
                .join("backups")
                .join("test-app"),
        );
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

    /// MANIFEST 的 `Spring-Boot-Version` 由 repackage 自动写入，2.x/3.x/4.x 都有，
    /// 是判断「该 jar 需要什么 JDK」的唯一可靠依据。样本取自本机真实 jar：
    /// online-sunlike-barcode(2.3.3.RELEASE) / pigx-boot(3.5.11) / SimImage(4.0.8)。
    #[test]
    fn reads_spring_boot_version_and_jdk_floor() {
        use crate::services::springboot_manager::{
            min_jdk_for_spring_boot, parse_spring_boot_major, read_spring_boot_version,
        };
        let dir = std::env::temp_dir().join(format!("opx_sbv_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for (ver, major, floor) in
            [("2.3.3.RELEASE", 2_u32, 8_u32), ("3.5.11", 3, 17), ("4.0.8", 4, 17)]
        {
            let p = dir.join(format!("sb{major}.jar"));
            fake_jar(
                &p,
                &format!("Manifest-Version: 1.0\r\nStart-Class: com.example.App\r\nSpring-Boot-Version: {ver}\r\nMain-Class: org.springframework.boot.loader.launch.JarLauncher\r\n\r\n"),
            );
            assert_eq!(
                read_spring_boot_version(p.to_str().unwrap()).as_deref(),
                Some(ver),
                "读出的框架版本"
            );
            assert_eq!(parse_spring_boot_major(ver), Some(major));
            assert_eq!(min_jdk_for_spring_boot(major), Some(floor), "Spring Boot {major}.x 的 JDK 门槛");
        }

        // 非 Spring Boot 打包的 jar 没有该字段 → None：不猜版本、不误报门槛
        let plain = dir.join("plain.jar");
        fake_jar(&plain, "Manifest-Version: 1.0\r\nImplementation-Version: 1.0\r\n\r\n");
        assert_eq!(read_spring_boot_version(plain.to_str().unwrap()), None);
        assert_eq!(parse_spring_boot_major(""), None);
        assert_eq!(min_jdk_for_spring_boot(5), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 折行的 MANIFEST 值必须拼回完整值（JAR 规范：续行以**单个空格**开头，该空格不属值）；
    /// 且拼完一个属性后，后面属性的续行不得被误拼进来。
    #[test]
    fn manifest_folded_value_is_rejoined() {
        use crate::services::springboot_manager::read_jar_manifest_field;
        let dir = std::env::temp_dir().join(format!("opx_mfold_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p = dir.join("folded.jar");
        fake_jar(
            &p,
            "Manifest-Version: 1.0\r\nImplementation-Version: 1.0.0-SNAPSHOT\r\n continued-part\r\nSpring-Boot-Version: 4.0.8\r\n\r\n",
        );
        assert_eq!(
            read_jar_manifest_field(p.to_str().unwrap(), "Implementation-Version").as_deref(),
            Some("1.0.0-SNAPSHOTcontinued-part")
        );
        // 紧跟在下一属性后的折行不应被算进上一个属性
        assert_eq!(
            read_jar_manifest_field(p.to_str().unwrap(), "Spring-Boot-Version").as_deref(),
            Some("4.0.8")
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 整合：`read_jar_info` 一次给出应用版本、框架版本与最低 JDK，三者互不串位
    #[tokio::test]
    async fn read_jar_info_combines_version_and_jdk_floor() {
        let dir = std::env::temp_dir().join(format!("opx_jinfo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p = dir.join("app.jar");
        fake_jar(
            &p,
            "Manifest-Version: 1.0\r\nImplementation-Version: 2.1.0\r\nSpring-Boot-Version: 3.5.11\r\n\r\n",
        );
        let info = super::read_jar_info(p.to_str().unwrap().to_string())
            .await
            .unwrap();
        assert_eq!(info.version.as_deref(), Some("2.1.0"));
        assert_eq!(info.spring_boot_version.as_deref(), Some("3.5.11"));
        assert_eq!(info.min_jdk, Some(17));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
