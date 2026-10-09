use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use tauri::{AppHandle, State};

use opx_core::event::{EventSink, EventSinkExt};

use opx_core::models::software::{
    BackupMode, Catalog, CatalogEntry, ConfigFieldType, ConfigSchema, CustomInstallParams,
    CustomStartCommand, InstallParams, InstalledSoftware, JreUsageReport, LogChunk, LogSource,
    SnapshotMeta, SoftwareStatus, UninstallSafetyReport,
};
use crate::services::software_manager::config_editor::FormData;
use opx_core::services::software_manager::providers::custom_templates;
use opx_core::services::software_manager::providers::{ConfigContext, DataDirContext};
use crate::services::software_manager::{
    backup, backup_scheduler, config_editor, health_check, installer, lifecycle,
    uninstall_guard, SoftwareManager,
};
use opx_core::services::software_manager::{catalog, log_viewer, providers};
// 阶段 3 批次 3B：`start_stop.rs` 的 13 个函数已搬入 core，本文件只留三个
// `#[tauri::command]` 薄封装（参数接收 + 错误转字符串）。下列 4 个符号在本文件
// 仍有调用点，故重导出保持原路径可写；其余 9 个只在 core 内部使用，
// 不重导出以收窄壳层可见面。沿用批次 3A2 在 `lifecycle.rs` 上验证过的
// 「同名模块用 pub use 转发消歧」手法——写错路径会编译失败而非静默用错实现。
pub use opx_core::services::software_manager::start_stop::{
    collect_configured_ports, do_start_software, ensure_dependencies, graceful_stop_software,
    wait_dependency_ready,
};
use opx_core::{audited_async, oplog_begin, oplog_fail, oplog_result};

/// 把壳层的 [`AppHandle`] 转成core 的事件通道，供服务层（零tauri 依赖）使用。
///
/// ## 为什么需要这个助手
/// `Arc<TauriEventSink>` **不会**自动 coerce 成 `Arc<dyn EventSink>`
/// （unsized coercion 只对直接类型生效，不穿透 `Arc`）。若在每个调用点写
/// `Arc::new(TauriEventSink::new(app.clone()))` 再靠期望类型推断，泛型/闭包
/// 场景下极易推断失败或退化成 `Arc<TauriEventSink>`。集中在此转换一次，
/// 调用点只写 `&sink_of(&app)`。
fn sink_of(app: &AppHandle) -> Arc<dyn EventSink> {
    Arc::new(crate::event_sink::TauriEventSink::new(app.clone()))
}

/// 获取可安装软件列表（catalog）
#[tauri::command]
pub async fn list_available_software(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<CatalogEntry>, String> {
    // ponytail: 以 builtin 为底，用旧缓存补齐 builtin 没有的额外 key。
    // builtin（含新增 provider）优先，避免旧缓存把新增内置软件覆盖回过期版本。
    let builtin = catalog::build_builtin_catalog();
    let cached = catalog::load_catalog_cache();
    let catalog = catalog::merge_with_builtin(builtin, cached);
    manager.set_catalog(catalog);
    Ok(manager.get_catalog().entries)
}

/// 刷新 catalog：重新拉取远程并合并
#[tauri::command]
pub async fn refresh_catalog(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
) -> Result<Vec<CatalogEntry>, String> {
    let builtin = catalog::build_builtin_catalog();

    // 从 settings 读取 mirror_url（远程 catalog.json，可选）
    let mirror_url = {
        let sp = opx_core::utils::paths::settings_path();
        if sp.exists() {
            std::fs::read_to_string(&sp)
                .ok()
                .and_then(|c| serde_json::from_str::<opx_core::models::settings::AppSettings>(&c).ok())
                .map(|s| s.mirror_url)
                .unwrap_or_else(|| "https://mirrors.aliyun.com".to_string())
        } else {
            "https://mirrors.aliyun.com".to_string()
        }
    };
    let remote_catalog = catalog::fetch_remote_catalog(&mirror_url).await;
    let mut merged = catalog::merge_catalogs(builtin, remote_catalog);

    merged.updated_at = Some(chrono::Local::now().to_rfc3339());
    manager.set_catalog(merged.clone());
    catalog::save_catalog_cache(&merged); // ponytail: 缓存到文件
    sink_of(&app).emit_ser("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}

/// 获取指定软件的远程版本列表（前端打开安装对话框时调用）
/// 返回包装后的 CatalogEntry（含远程版本），失败返回错误信息供前端展示
#[tauri::command]
pub async fn fetch_remote_versions_for(
    manager: State<'_, Arc<SoftwareManager>>,
    key: String,
) -> Result<Vec<CatalogEntry>, String> {
    // 用 spawn_blocking 调用 provider 的 sync fetch_remote_versions
    // 避免 reqwest::blocking 在 async 上下文中 panic
    let key_for_blocking = key.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let providers = providers::all_providers();
        let provider = providers
            .iter()
            .find(|p| p.key() == key_for_blocking)
            .ok_or_else(|| format!("未知软件: {}", key_for_blocking))?;
        Ok::<_, String>(provider.fetch_remote_versions())
    })
    .await
    .map_err(|e| format!("获取版本列表失败: {}", e))?;

    match result {
        Ok(Some(versions)) => {
            manager.merge_entry_versions(&key, versions.clone());
            // ponytail: 更新缓存
            catalog::save_catalog_cache(&manager.get_catalog());

            // 包装成单条 CatalogEntry 返回（前端合并展示）
            let providers = providers::all_providers();
            let provider = providers.iter().find(|p| p.key() == key);
            if let Some(p) = provider {
                let mut entry = p.catalog_entry();
                entry.versions = versions;
                Ok(vec![entry])
            } else {
                Err(format!("未知软件: {}", key))
            }
        }
        Ok(None) => {
            // 该软件不支持动态拉取（如 MySQL/RustFS/Redis），返回空
            Ok(vec![])
        }
        Err(e) => Err(e),
    }
}

/// 获取已安装软件列表
#[tauri::command]
pub async fn list_installed_software(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<InstalledSoftware>, String> {
    let catalog = manager.get_catalog();
    let mut list = manager.get_installed();
    // 为每个已装软件附加 catalog 分类与图标（自定义软件无对应 entry，保持默认）
    for sw in list.iter_mut() {
        if sw.category.is_none() || sw.icon.is_empty() {
            if let Some(e) = catalog.entries.iter().find(|e| e.key == sw.key) {
                if sw.category.is_none() {
                    sw.category = Some(e.category.clone());
                }
                if sw.icon.is_empty() {
                    sw.icon = e.icon.clone();
                }
            }
        }
    }
    Ok(list)
}

/// 安装预置软件（在线镜像）
#[tauri::command]
pub async fn install_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: InstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    oplog_begin!("install", &params.key, &params.version);
    installer::register_install_audit(&install_id, "install", &params.key, &params.version);
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    let sink = sink_of(&app);
    tauri::async_runtime::spawn(async move {
        installer::install_software(sink, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 替换式升级：停旧 → 备份旧目录为 .bak → 装新版 → 迁移数据/配置 → 合并记录。
/// 返回安装任务 id（供前端 createTask 跟踪进度），进度/完成事件复用 install-progress。
#[tauri::command]
pub async fn upgrade_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let audit_target = manager
        .find_installed(&installed_id)
        .map(|s| s.name)
        .unwrap_or_default();
    oplog_begin!("upgrade", &audit_target);
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let installed_id_for_task = installed_id.clone();
    let install_id_for_task = install_id.clone();
    // spawn 需 'static：先构造owned 的事件通道再move 进去。
    let sink = sink_of(&app);
    tauri::async_runtime::spawn(async move {
        let audit_target_for_task = audit_target.clone();
        let r = do_upgrade(
            &manager_arc,
            &sink,
            &installed_id_for_task,
            &install_id_for_task,
        )
        .await;
        oplog_result!("upgrade", &audit_target_for_task, "", r);
        if let Err(ref e) = r {
            sink.emit_ser(
                "install-progress",
                serde_json::json!({
                    "install_id": install_id_for_task,
                    "phase": "failed",
                    "error": format!("{}", e),
                    "stage": "upgrade"
                }),
            );
        }
    });
    Ok(install_id)
}

/// 替换式升级核心流程（供 upgrade_software 后台任务执行）
async fn do_upgrade(
    manager: &Arc<SoftwareManager>,
    sink: &Arc<dyn EventSink>,
    installed_id: &str,
    install_id: &str,
) -> anyhow::Result<()> {
    let software = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    // 目标版本：compute_upgrades 中该实例的 target_version（无可升级则报错）。
    // 必须按 installed_id 匹配：同一 key 可并存多个实例，按 key 取会拿到别的实例的目标版本。
    let catalog = manager.get_catalog();
    let installed_all = manager.get_installed();
    let target_version = compute_upgrades(&installed_all, &catalog)
        .into_iter()
        .find(|u| u.installed_id == installed_id)
        .and_then(|u| u.target_version)
        .ok_or_else(|| anyhow::anyhow!("{} 已是最新版本", software.name))?;

    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", software.key))?;

    // 数据/配置迁移目录（相对 install_path）+ 配置文件相对路径
    let data_dirs = provider.data_dirs(&DataDirContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    });
    let config_file_path = provider.config_file_path(&ConfigContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    });

    // 1. 若运行中（Running/Starting）→ 停止并等待
    if software.status == SoftwareStatus::Running || software.status == SoftwareStatus::Starting {
        if let Some(pid) = software.pid {
            let (ok, _) = tokio::task::spawn_blocking(move || lifecycle::stop_one(pid))
                .await
                .map_err(|e| anyhow::anyhow!("停止线程异常: {}", e))?;
            if !ok {
                return Err(anyhow::anyhow!(
                    "停止旧版本进程失败（PID {} 仍在运行）",
                    pid
                ));
            }
        }
        manager
            .update_runtime_fields(
                &software.id,
                SoftwareStatus::Stopped,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| anyhow::anyhow!("状态更新失败: {}", e))?;
        lifecycle::unregister(&software.id);
    }

    // 2. 备份源：旧安装目录（不 rename，装完压缩为 {old_ver}.bak.zip 省磁盘）
    let old_install_path = opx_core::utils::paths::resolve_install_path(&software.install_path);
    let bak_zip_path = old_install_path.with_file_name(format!(
        "{}.bak.zip",
        old_install_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
    ));

    // 2.5 把新版本父目录建好（download_and_extract 解压时落到新目录）
    let new_install_path = opx_core::utils::paths::apps_dir()
        .join(&software.key)
        .join(&target_version);
    std::fs::create_dir_all(&new_install_path)
        .map_err(|e| anyhow::anyhow!("创建新版安装目录失败: {}", e))?;

    // 3. 装新版到 <key>/<target_version>（下载→SHA 校验→解压→post_install）
    //    从 builtin 静态目录取 version_info/mirror（upgrade 目标 vs 当前目录优先匹配）
    let catalog_entry = catalog
        .entries
        .iter()
        .find(|e| e.key == software.key)
        .ok_or_else(|| anyhow::anyhow!("未知软件：{}", software.key))?;
    let version_info = catalog_entry
        .versions
        .iter()
        .find(|v| v.version == target_version)
        .ok_or_else(|| anyhow::anyhow!("{} 不支持版本 {}", catalog_entry.name, target_version))?;
    // 选可联网下载的镜像（builtin 镜像无真实 URL，download_and_extract 走 HTTP 下载）。
    // 只定「首选源」的下标，实际下载由 download_with_mirror_fallback 按序回退。
    let preferred_index = version_info
        .mirrors
        .iter()
        .position(|m| m.builtin.is_none() && m.url.starts_with("http"))
        .ok_or_else(|| anyhow::anyhow!("{} 无可用镜像源", target_version))?;

    let params = InstallParams {
        key: software.key.clone(),
        version: target_version.clone(),
        mirror_index: 0,
        set_as_default_jre: false,
    };

    // 4. 下载+解压到新目录（首选源不可达时自动回退其余可联网镜像）
    installer::download_with_mirror_fallback(
        &params,
        &new_install_path,
        sink.clone(),
        install_id,
        version_info,
        preferred_index,
    )
    .await
    .map_err(|e| {
        // 解压失败：回滚新目录（旧目录未动）
        let _ = std::fs::remove_dir_all(&new_install_path);
        e
    })?;

    // 5. 迁移用户数据/配置：从旧目录复制 data_dirs + config_file_path 到新目录
    //    迁移失败时回滚：删除已装好的新目录，旧目录不动，旧记录不动
    copy_paths_to_new(
        &old_install_path,
        &old_install_path,
        &new_install_path,
        &data_dirs,
        &config_file_path,
    )
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&new_install_path);
        e
    })?;

    // 5.5 压缩备份旧目录为 {old_ver}.bak.zip（已有同名先删），成功后删除旧目录。
    //     压缩失败回滚：删除新目录、旧目录保留（zip 未生成则无害）。
    if old_install_path.exists() {
        if bak_zip_path.exists() {
            std::fs::remove_file(&bak_zip_path)
                .map_err(|e| anyhow::anyhow!("清理旧备份失败: {}", e))?;
        }
        zip_dir(&old_install_path, &bak_zip_path)
            .map_err(|e| anyhow::anyhow!("压缩备份旧目录失败: {}", e))
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&new_install_path);
                e
            })?;
        // 删除旧目录失败不阻塞（zip 已生成，可视为备份完成；残留目录后续可手动清理）
        if let Err(e) = std::fs::remove_dir_all(&old_install_path) {
            tracing::warn!(path = %old_install_path.display(), err = %e, "删除旧安装目录失败（.bak.zip 已生成）");
        }
    }

    // 6. 记录合并：删旧记录，生成新记录（继承 config/auto_start/startup_order/port）
    let new_installed_id = uuid::Uuid::new_v4().to_string();
    let new_record = InstalledSoftware {
        id: new_installed_id.clone(),
        key: software.key.clone(),
        version: target_version.clone(),
        name: format!("{} {}", catalog_entry.name, target_version),
        install_path: format!("{}/{}", software.key, target_version),
        install_time: chrono::Utc::now().naive_utc(),
        status: SoftwareStatus::Unknown,
        port: software.port,
        config: software.config.clone(),
        is_custom: software.is_custom,
        auto_start_on_app_start: software.auto_start_on_app_start,
        startup_order: software.startup_order,
        source: software.source.clone(),
        pid: None,
        last_started_at: None,
        last_stopped_at: None,
        last_error: None,
        custom_start_command: None,
        icon: String::new(),
        category: software.category.clone(),
        depends_on: vec![],
        auto_restart: software.auto_restart,
    };

    // remove_installed 会删除旧安装目录（旧目录已压缩删除，此时路径不存在，不误删 .bak.zip 备份）
    let old_removed = manager
        .remove_installed(installed_id)
        .map_err(|e| anyhow::anyhow!("删除旧记录失败: {}", e))?;
    let _ = old_removed;

    // add_installed 失败时回滚：恢复旧记录，新目录保持已装状态、.bak.zip 保留（zip 备份不删除）
    manager.add_installed(new_record).map_err(|e| {
        let _ = manager.add_installed(old_removed);
        anyhow::anyhow!("写入新记录失败: {}", e)
    })?;

    // 7. emit completed（install_id 由前端 createTask 给定）
    sink.emit_ser(
        "install-progress",
        serde_json::json!({
            "install_id": install_id,
            "phase": "completed",
            "installed_id": new_installed_id,
        }),
    );

    Ok(())
}

/// 从源目录复制 data_dirs（目录递归）+ config_file_path 指向的文件到新目录。
/// 目标父目录不存在则创建；跳过源不存在的路径。
/// data_dirs 可能返回绝对路径（旧 install_path 内），先归一化为相对路径再映射到新目录。
fn copy_paths_to_new(
    src_path: &std::path::Path,
    old_install_path: &std::path::Path,
    new_install_path: &std::path::Path,
    data_dirs: &[std::path::PathBuf],
    config_file_path: &Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    for rel in data_dirs {
        let rel = rel.strip_prefix(old_install_path).unwrap_or(rel);
        let src = src_path.join(rel);
        if src.exists() {
            let dst = new_install_path.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow::anyhow!("创建目标目录失败 {}: {}", parent.display(), e))?;
            }
            copy_dir_recursive(&src, &dst)?;
        }
    }

    if let Some(rel) = config_file_path {
        // 与 data_dirs 一致：绝对路径（旧 install_path 内）先归一化为相对路径，否则 join 静默失效
        let rel = rel.strip_prefix(old_install_path).unwrap_or(rel);
        let src = src_path.join(rel);
        if src.exists() {
            let dst = new_install_path.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow::anyhow!("创建目标目录失败 {}: {}", parent.display(), e))?;
            }
            std::fs::copy(&src, &dst)
                .map_err(|e| anyhow::anyhow!("复制配置文件失败 {}: {}", dst.display(), e))?;
        }
    }
    Ok(())
}

/// 递归复制目录（std::fs，无第三方依赖）
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| anyhow::anyhow!("创建目录失败 {}: {}", dst.display(), e))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| anyhow::anyhow!("读取目录失败 {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| anyhow::anyhow!("读取目录项失败: {}", e))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| anyhow::anyhow!("复制文件失败 {}: {}", to.display(), e))?;
        }
    }
    Ok(())
}

/// 递归压缩目录为 zip 压缩包（Deflated）。zip 内条目为相对路径，统一 `/` 分隔。
/// 空目录跳过（备份目录树由文件承载，空目录无保留价值）。
fn zip_dir(src: &std::path::Path, dst_zip: &std::path::Path) -> std::io::Result<()> {
    let file = std::fs::File::create(dst_zip)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path == src || path.is_dir() {
            continue;
        }
        let rel = path.strip_prefix(src).unwrap_or(path);
        let name = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(&name, opts)?;
        std::io::copy(&mut std::fs::File::open(path)?, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

/// 解压 zip 到 dst_dir（完整目录树）。跳过目录条目；覆盖写（以解压文件为准）。
fn unzip_to(zip_path: &std::path::Path, dst_dir: &std::path::Path) -> std::io::Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let Some(rel) = file.enclosed_name() else {
            continue;
        };
        let outpath = dst_dir.join(rel);
        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&outpath)?;
        std::io::copy(&mut file, &mut out)?;
    }
    Ok(())
}

/// 安装用户上传的自定义压缩包
#[tauri::command]
pub async fn install_custom(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: CustomInstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    oplog_begin!("install_custom", &params.name);
    installer::register_install_audit(&install_id, "install_custom", &params.name, "");
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    let sink = sink_of(&app);
    tauri::async_runtime::spawn(async move {
        installer::install_custom(sink, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 卸载已安装软件
///
/// 流程：
/// 1. 复查卸载安全性（防止前端绕过 check_uninstall_safety 直接调用）
/// 2. 强杀兜底（防意外残留 PID）
/// 3. 从 installed.json 移除记录 + 删除安装目录
/// 4. emit "software-uninstalled" 事件
#[tauri::command]
pub async fn uninstall_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
) -> Result<bool, String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = software.name.clone();
    let detail = String::new();

    audited_async!("uninstall", target, detail, {
        let report =
            uninstall_guard::check_uninstall_safety(&software).map_err(|e| e.to_string())?;
        if !report.safe {
            let reasons = report
                .blockers
                .iter()
                .map(|b| b.kind.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("卸载被阻止：{}", reasons));
        }

        // 强杀兜底（防意外残留 PID）
        if let Some(pid) = software.pid {
            if health_check::is_process_alive(pid) {
                let _ = lifecycle::stop_one(pid);
            }
        }
        lifecycle::unregister(&installed_id);

        // 删除记录 + 安装目录。
        // ponytail: remove_installed 内含 remove_dir_all 删整个安装目录（可能数百 MB），
        // 同步执行会阻塞 async worker → 前端 await 挂起、卸载框不关。移入 spawn_blocking。
        let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
        let id_for_remove = installed_id.clone();
        tokio::task::spawn_blocking(move || manager_arc.remove_installed(&id_for_remove))
            .await
            .map_err(|e| format!("卸载线程异常: {}", e))?
            .map_err(|e| e.to_string())?;

        sink_of(&app).emit_ser("software-uninstalled", &installed_id);
        Ok(true)
    })
}

// ===== 启停命令（任务 10.1）=====

/// 启动软件
///
/// 流程：状态转换校验 → PID 残留校验 → 异步 spawn 启动任务
/// （构造 StartCommand → 首次初始化 → spawn 子进程 → 注册 PID →
///   emit Starting → 异步健康检查 → emit Running/Error）
#[tauri::command]
pub async fn start_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
    init_password: Option<String>,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let audit_target = software.name.clone();
    let audit_detail = format!("{} ({})", software.version, software.id);
    oplog_begin!("start", &audit_target, &audit_detail);

    // PID 残留校验：旧 PID 仍存活则拒绝启动（纯读，先于任何状态变更）
    if let Some(pid) = software.pid {
        if health_check::is_process_alive(pid) {
            let msg = format!("进程 {} 仍在运行，请先停止", pid);
            oplog_fail!("start", &audit_target, &audit_detail, &msg);
            return Err(msg);
        }
    }

    // P1-2 启动竞态修复：原子「校验 + 占位 Starting」。
    // 历史上校验在命令入口、置 Starting 在 spawn 后的异步任务里（do_start_software
    // 之前还要跑依赖编排 + 首次初始化，最长 180s），间隙内并发第二次 start 可通过
    // 校验 → 双进程/端口冲突。现在写锁内一次完成，重复启动被 Starting 状态拒绝。
    if let Err(e) = manager.try_reserve_start(&installed_id) {
        oplog_fail!("start", &audit_target, &audit_detail, &e);
        return Err(e.to_string());
    }

    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let installed_id_for_task = installed_id.clone();
    // spawn 需 'static：先构造 owned 的事件通道再 move 进去。
    let sink = sink_of(&app);
    let audit_target_task = audit_target.clone();
    let audit_detail_task = audit_detail.clone();

    // 异步执行启动流程，命令本身立即返回
    tauri::async_runtime::spawn(async move {
        // 依赖编排：先按拓扑序拉起未运行的依赖，再启动自身
        if let Err(e) = ensure_dependencies(&manager_arc, &sink, &installed_id_for_task).await
        {
            let msg = format!("依赖编排失败：{}", e);
            oplog_fail!("start", &audit_target_task, &audit_detail_task, &msg);
            tracing::warn!(error = %e, installed_id = %installed_id_for_task, "依赖编排失败");
            let _ = manager_arc.update_runtime_fields(
                &installed_id_for_task,
                SoftwareStatus::Error,
                None,
                None,
                None,
                Some(msg.clone()),
            );
            lifecycle::emit_status_changed(
                &sink,
                &installed_id_for_task,
                SoftwareStatus::Error,
                None,
                Some(msg.clone()),
            );
            return;
        }
        let result = do_start_software(
            &manager_arc,
            &sink,
            &installed_id_for_task,
            init_password,
        )
        .await;
        oplog_result!("start", &audit_target_task, &audit_detail_task, result);
        if let Err(e) = result {
            let _ = manager_arc.update_runtime_fields(
                &installed_id_for_task,
                SoftwareStatus::Error,
                None,
                None,
                None,
                Some(format!("启动失败：{}", e)),
            );
            lifecycle::emit_status_changed(
                &sink,
                &installed_id_for_task,
                SoftwareStatus::Error,
                None,
                Some(format!("启动失败：{}", e)),
            );
            tracing::error!(
                error = %e,
                installed_id = %installed_id_for_task,
                "start_software failed",
            );
        }
    });

    Ok(())
}

/// 更新软件的依赖清单（需已安装的软件 id；自动去重、剔除自引用与不存在项）。
#[tauri::command]
pub async fn update_software_deps(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    depends_on: Vec<String>,
) -> Result<InstalledSoftware, String> {
    manager
        .update_dependencies(&installed_id, depends_on)
        .map_err(|e| e.to_string())?;
    manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))
}

/// 解析某软件的依赖拓扑（分层 + 环检测），供前端依赖图/启动预览。返回被拉起的依赖顺序。
#[tauri::command]
pub async fn resolve_software_deps(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<serde_json::Value, String> {
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let software = manager_arc
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let deps_ids: Vec<String> = software.depends_on.clone();
    let deps = |n: &str| -> Vec<String> {
        manager_arc
            .find_installed(n)
            .map(|sw| sw.depends_on)
            .unwrap_or_default()
    };
    let tiebreak: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    // 闭包展开
    let mut visited: Vec<String> = Vec::new();
    let mut stack: Vec<String> = vec![installed_id.clone()];
    while let Some(id) = stack.pop() {
        if visited.contains(&id) {
            continue;
        }
        visited.push(id.clone());
        for dep in deps(&id) {
            stack.push(dep);
        }
    }
    let plan = opx_core::utils::topo::topo_layers(&visited, deps, &tiebreak);
    let missing: Vec<String> = visited
        .iter()
        .filter(|id| id.as_str() != installed_id && manager_arc.find_installed(id).is_none())
        .cloned()
        .collect();
    Ok(serde_json::json!({
        "layers": plan.layers,
        "cycle": plan.cycle,
        "missing": missing,
        "direct": deps_ids,
    }))
}

/// 单个配置端口的诊断状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortStatus {
    pub port: u16,
    /// listening=本软件正常监听 / conflict=被其他进程占用 / not-listening=未监听 / unknown=已监听但宿主未知
    pub state: String,
    pub owner_pid: Option<u32>,
    pub owner_name: Option<String>,
}

/// 某软件的端口图谱报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortReport {
    /// 配置声明的端口及诊断
    pub configured: Vec<PortStatus>,
    /// 本软件进程实际监听的所有端口（含配置外的）
    pub listening: Vec<u16>,
}

/// 采集某软件的端口监听与冲突诊断（仅运行中实例有意义）。
#[tauri::command]
pub async fn get_software_port_report(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<PortReport, String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;

    // 1. 配置端口（自定义软件走 custom_start_command，无 provider）
    let providers_list = providers::all_providers();
    let provider = providers_list.iter().find(|p| p.key() == software.key);
    let configured_ports = collect_configured_ports(&software, provider.map(|p| &**p));

    // 2. 实际监听（命令失败降级为空集）
    let entries = crate::services::software_manager::netutils::listen_entries();
    let my_pid = software.pid;
    let mut listening: Vec<u16> = entries
        .iter()
        .filter(|e| my_pid.map_or(false, |p| e.pid == Some(p)))
        .map(|e| e.port)
        .collect();
    listening.sort_unstable();
    listening.dedup();

    // 3. 配置端口 × 监听集 → 诊断
    let mut configured = Vec::new();
    for port in configured_ports {
        let owner = entries.iter().find(|e| e.port == port);
        let (state, owner_pid, owner_name) = match owner {
            None => ("not-listening", None, None),
            Some(e) => match (my_pid, e.pid) {
                (Some(mine), Some(op)) if mine == op => ("listening", Some(op), None),
                (_, Some(op)) => (
                    "conflict",
                    Some(op),
                    crate::services::software_manager::process_monitor::process_name(op),
                ),
                (_, None) => ("unknown", None, None),
            },
        };
        configured.push(PortStatus {
            port,
            state: state.to_string(),
            owner_pid,
            owner_name,
        });
    }

    Ok(PortReport {
        configured,
        listening,
    })
}

/// 停止软件
#[tauri::command]
pub async fn stop_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
) -> Result<bool, String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = software.name.clone();
    let detail = format!("{} ({})", software.version, software.id);

    audited_async!("stop", target, detail, {
        lifecycle::validate_stop_transition(software.status.clone()).map_err(|e| e.to_string())?;
        let sink = sink_of(&app);

        // 无 PID（如初始化失败卡住时）：直接设为 Stopped 返回
        let pid = match software.pid {
            Some(p) => p,
            None => {
                manager
                    .update_runtime_fields(
                        &installed_id,
                        SoftwareStatus::Stopped,
                        None,
                        None,
                        Some(Local::now().naive_local()),
                        None,
                    )
                    .map_err(|e| e.to_string())?;
                lifecycle::unregister(&installed_id);
                lifecycle::emit_status_changed(
                    &sink,
                    &installed_id,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                );
                return Ok(true);
            }
        };

        // 更新状态为 Stopping
        manager
            .update_runtime_fields(
                &installed_id,
                SoftwareStatus::Stopping,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
        lifecycle::emit_status_changed(&sink, &installed_id, SoftwareStatus::Stopping, None, None);

        let pid_for_status = pid;
        // P1-3：优先语义化优雅停止（provider 关闭命令），失败/超时回退强杀；
        // 外层 30s 超时给优雅停止命令（最长 15s）留足余量。
        let stop_sw = software.clone();
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::task::spawn_blocking(move || graceful_stop_software(&stop_sw, pid)),
        )
        .await;

        let graceful = match result {
            Ok(Ok((true, _))) => true,
            other => {
                tracing::warn!(installed_id = %installed_id, pid = pid_for_status,
                stop_result = ?other, "stop_one incomplete/unexpected");
                false
            }
        };

        let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
        let installed_id_clone = installed_id.clone();

        // 停止成功：置 Stopped。停止失败（进程仍存活）：如实反馈 Error，保留 pid 供下次 stop，
        // 避免 UI 假报"已停止"导致进程残留占用端口。
        if graceful {
            manager_arc
                .update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                    Some(Local::now().naive_local()),
                    None,
                )
                .map_err(|e| e.to_string())?;
            lifecycle::unregister(&installed_id_clone);
            lifecycle::emit_status_changed(
                &sink,
                &installed_id_clone,
                SoftwareStatus::Stopped,
                None,
                None,
            );
            tracing::info!(installed_id = %installed_id_clone, pid = pid_for_status, "software stopped");
            Ok(graceful)
        } else {
            let msg = format!(
                "进程 PID {} 未能停止，可能仍在运行并占用端口，请重试或手动结束该进程。",
                pid_for_status
            );
            manager_arc
                .update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    Some(pid_for_status),
                    None,
                    None,
                    Some(msg.clone()),
                )
                .map_err(|e| e.to_string())?;
            lifecycle::emit_status_changed(
                &sink,
                &installed_id_clone,
                SoftwareStatus::Error,
                Some(pid_for_status),
                Some(msg.clone()),
            );
            tracing::error!(installed_id = %installed_id_clone, pid = pid_for_status, "software stop failed, process may still hold ports");
            Err(msg)
        }
    })
}

/// 重启软件
#[tauri::command]
pub async fn restart_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
    init_password: Option<String>,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = software.name.clone();
    let detail = format!("{} ({})", software.version, software.id);

    audited_async!("restart", target, detail, {
        let should_stop = software.status == SoftwareStatus::Running
            || software.status == SoftwareStatus::Starting;
        if should_stop {
            let pid = software.pid.ok_or_else(|| "无 PID".to_string())?;
            // P1-3：重启前的停止也走语义化优雅停止，避免强杀数据库
            let restart_sw = software.clone();
            let _ = tokio::task::spawn_blocking(move || graceful_stop_software(&restart_sw, pid))
                .await
                .map_err(|e| format!("停止失败: {}", e))?;
            manager
                .update_runtime_fields(
                    &installed_id,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(|e| e.to_string())?;
            lifecycle::unregister(&installed_id);
        }

        // 再启动
        let sink = sink_of(&app);
        let installed_id_clone = installed_id.clone();
        let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
        if let Err(e) =
            do_start_software(&manager_arc, &sink, &installed_id_clone, init_password).await
        {
            // 启动失败（如端口占用）：置为 Error 并 emit，让管理页显示失败原因
            let msg = format!("启动失败：{}", e);
            let _ = manager_arc.update_runtime_fields(
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                None,
                None,
                Some(msg.clone()),
            );
            lifecycle::emit_status_changed(
                &sink,
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                Some(msg.clone()),
            );
            return Err(msg);
        }
        Ok(())
    })
}

/// 查询单个软件状态
#[tauri::command]
pub async fn get_software_status(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<SoftwareStatus, String> {
    let sw = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    Ok(sw.status)
}

// ===== 配置编辑命令（任务 10.2）=====

/// 获取指定软件的表单 schema
/// 自定义软件返回 None（无统一表单）
#[tauri::command]
pub async fn get_config_schema(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<Option<ConfigSchema>, String> {
    let software = load_software_for_id(&installed_id)?;
    if software.is_custom {
        return Ok(None);
    }
    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
    let mut schema = match provider.config_schema() {
        Some(s) => s,
        None => return Ok(None),
    };

    // JDK 选择字段填充：凡 config_schema 含 key=="jdk" 的 Select 字段（nacos/kafka/elasticsearch 等 Java 中间件），
    // 动态列出已装 JDK/JRE 作为 Select options。provider 的 config_schema 是静态的，options 为空，需在此处填充。
    let has_jdk_field = schema.fields.iter().any(|f| f.key == "jdk");
    if has_jdk_field {
        fill_jdk_options(&mut schema, manager.inner(), provider.min_jdk_version());
    }
    // 注：nacos 的 mysql_* 连接字段是否显示，交由 schema.field_rules（visible_when storage=mysql）
    // 在前端按表单实时值切换，避免此处按已保存 config 静态剥离导致改 storage 后字段不出现。

    Ok(Some(schema))
}

/// 解析 JDK/JRE 版本串的主版本号：
/// - 以 "1." 开头取第二段（"1.8.0" → 8）
/// - 否则取首段（"17.0.12" → 17、"21" → 21）
fn parse_jdk_major(version: &str) -> Option<u32> {
    let v = version.trim();
    if let Some(rest) = v.strip_prefix("1.") {
        rest.split('.').next().and_then(|s| s.parse::<u32>().ok())
    } else {
        v.split(['.', '-'])
            .next()
            .and_then(|s| s.parse::<u32>().ok())
    }
}

/// 把已装 JDK/JRE 填充进 schema 中 key=="jdk" 的 Select 字段：
/// - options 存 installed_id（稳定标识，便携版路径变动不影响）
/// - labels 存 "name (version) [JDK/JRE]"（与 SpringBoot 应用选择一致）
/// 供 Nacos 等需选 JDK 的软件复用。找不到 JDK 时保留空 options（前端显示"未安装 JDK"）。
/// `min_jdk_version` 用于过滤低于软件最低要求的 JDK/JRE（如 Kafka 隐藏 <11、ES 隐藏 <17）。
fn fill_jdk_options(
    schema: &mut ConfigSchema,
    manager: &SoftwareManager,
    min_jdk_version: Option<u32>,
) {
    let jdks: Vec<InstalledSoftware> = manager
        .get_installed()
        .into_iter()
        .filter(|s| s.key == "jdk" || s.key == "jre")
        .filter(|s| match min_jdk_version {
            Some(min) => parse_jdk_major(&s.version)
                .map(|mj| mj >= min)
                .unwrap_or(false),
            None => true,
        })
        .collect();
    if jdks.is_empty() {
        return;
    }
    let ids: Vec<String> = jdks.iter().map(|s| s.id.clone()).collect();
    let labels: Vec<String> = jdks
        .iter()
        .map(|s| {
            format!(
                "{} ({}) {}",
                s.name,
                s.version,
                if s.key == "jdk" { "[JDK]" } else { "[JRE]" }
            )
        })
        .collect();
    for field in &mut schema.fields {
        if field.key == "jdk" {
            if let ConfigFieldType::Select {
                options,
                labels: lbls,
                ..
            } = &mut field.field_type
            {
                *options = ids.clone();
                *lbls = labels.clone();
                // 默认选中第一个 JDK（若默认值为空）
                if field
                    .default_value
                    .as_str()
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
                {
                    field.default_value = serde_json::json!(ids[0]);
                }
            }
        }
    }
}

/// 读表单数据：优先从 installed.json 的 config 字段读（权威来源），
/// 缺失字段用 schema default_value 兜底
#[tauri::command]
pub async fn read_config_form(installed_id: String) -> Result<FormData, String> {
    let software = load_software_for_id(&installed_id)?;
    if software.is_custom {
        return Err("自定义软件无表单 schema".to_string());
    }
    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
    let schema = provider
        .config_schema()
        .ok_or_else(|| "该软件无表单 schema".to_string())?;

    // 从 installed.json 的 config 字段读，缺失用 default_value
    let mut form = FormData::new();
    for field in &schema.fields {
        let mut value = software
            .config
            .get(&field.key)
            .cloned()
            .unwrap_or_else(|| field.default_value.clone());
        // 归一化 innodb_buffer_pool_size：前端 Number 输入不认 "128M" 字符串，
        // 把已存储的带 M/G 后缀值转为纯数字，避免 v-model.number 得 NaN
        if field.key == "innodb_buffer_pool_size" {
            if let Some(s) = value.as_str() {
                let stripped = s.trim_end_matches('M').trim_end_matches('G');
                if let Ok(n) = stripped.parse::<i64>() {
                    value = serde_json::json!(n);
                }
            }
        }
        form.insert(field.key.clone(), value);
    }
    Ok(form)
}

/// 写表单数据：把表单字段写回配置文件，并同步更新 installed.json 的 config
#[tauri::command]
pub async fn write_config_form(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    mut data: FormData,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = format!("{} ({})", software.name, installed_id);
    let detail = String::new();

    audited_async!("config_form", target, detail, {
        if software.is_custom {
            return Err("自定义软件无表单 schema".to_string());
        }
        let providers_list = providers::all_providers();
        let provider = providers_list
            .iter()
            .find(|p| p.key() == software.key)
            .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
        let schema = provider
            .config_schema()
            .ok_or_else(|| "该软件无表单 schema".to_string())?;

        // 归一化 innodb_buffer_pool_size：前端 Number 输入传纯数字（如 512），
        // MySQL 要求带 M/G 单位后缀；若值不含单位则自动追加 "M"
        if let Some(v) = data.get("innodb_buffer_pool_size") {
            let needs_suffix = match v {
                serde_json::Value::Number(_) => true,
                serde_json::Value::String(s) => !s.ends_with('M') && !s.ends_with('G'),
                _ => false,
            };
            if needs_suffix {
                let num_str = match v {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => String::new(),
                };
                if !num_str.is_empty() {
                    data.insert(
                        "innodb_buffer_pool_size".to_string(),
                        serde_json::Value::String(format!("{}M", num_str)),
                    );
                }
            }
        }

        let cctx = ConfigContext {
            install_path: software.install_path.clone(),
            version: software.version.clone(),
            config: software.config.clone(),
        };
        // MinIO/RustFS 无配置文件（config_file_path 返回 None），此时仅更新 installed.json 的 config 字段
        if let Some(file_path) = provider.config_file_path(&cctx) {
            let full_path = std::path::Path::new(&software.install_path).join(file_path);
            config_editor::write_form_to_config(&full_path, &schema, &data)
                .map_err(|e| e.to_string())?;
        }

        // 同步 config 到 installed.json
        let mut new_config = software.config.clone();
        if let Some(obj) = new_config.as_object_mut() {
            for (k, v) in &data {
                // 跳过 ephemeral 字段（如 MySQL 初始化密码），绝不写入 installed.json（不落盘）
                if schema.ephemeral_keys.iter().any(|ek| ek == k) {
                    continue;
                }
                obj.insert(k.clone(), v.clone());
            }
        }
        manager
            .update_config(&installed_id, new_config)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// 读配置文件源码（整个文件内容）
#[tauri::command]
pub async fn read_config_source(installed_id: String) -> Result<String, String> {
    let software = load_software_for_id(&installed_id)?;
    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
    let cctx = ConfigContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    };
    let file_path = provider
        .config_file_path(&cctx)
        .ok_or_else(|| "该软件无配置文件".to_string())?;
    let full_path = std::path::Path::new(&software.install_path).join(file_path);
    config_editor::read_config_source(&full_path).map_err(|e| e.to_string())
}

/// 写配置文件源码（整个文件内容，含备份 + 原子写）
#[tauri::command]
pub async fn write_config_source(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    content: String,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = format!("{} ({})", software.name, installed_id);
    let detail = String::new();

    audited_async!("config_source", target, detail, {
        let providers_list = providers::all_providers();
        let provider = providers_list
            .iter()
            .find(|p| p.key() == software.key)
            .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
        let cctx = ConfigContext {
            install_path: software.install_path.clone(),
            version: software.version.clone(),
            config: software.config.clone(),
        };
        let file_path = provider
            .config_file_path(&cctx)
            .ok_or_else(|| "该软件无配置文件".to_string())?;
        let full_path = std::path::Path::new(&software.install_path).join(file_path);
        config_editor::write_config_source(&full_path, &content).map_err(|e| e.to_string())
    })
}

/// 辅助：按 installed_id 从 installed.json 读单条记录（不依赖 State）
/// 适用于不需要修改 installed.json 的只读命令（如 get_config_schema、read_config_form）
fn load_software_for_id(installed_id: &str) -> Result<InstalledSoftware, String> {
    let path = opx_core::utils::paths::config_dir().join("installed.json");
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let list: opx_core::models::software::InstalledSoftwareList =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut sw = list
        .software
        .into_iter()
        .find(|s| s.id == installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    sw.install_path = opx_core::utils::paths::resolve_install_path(&sw.install_path)
        .to_string_lossy()
        .to_string();
    Ok(sw)
}

// ===== 卸载校验 + 自定义启动命令 + 启动设置命令（任务 10.3）=====

/// 检查卸载是否安全
/// - 运行中/启动中/停止中/初始化中 → 阻止
/// - JRE 且是默认或被依赖 → 阻止
#[tauri::command]
pub async fn check_uninstall_safety(installed_id: String) -> Result<UninstallSafetyReport, String> {
    let software = load_software_for_id(&installed_id)?;
    uninstall_guard::check_uninstall_safety(&software).map_err(|e| e.to_string())
}

/// 检查 JRE 是否被使用（默认 JRE / 被 SpringBoot 应用依赖）
#[tauri::command]
pub async fn check_jre_in_use(jre_installed_id: String) -> Result<JreUsageReport, String> {
    uninstall_guard::check_jre_in_use(&jre_installed_id).map_err(|e| e.to_string())
}

/// 获取自定义软件的启动命令配置
/// 标准软件返回 None
#[tauri::command]
pub async fn get_custom_start_command(
    installed_id: String,
) -> Result<Option<CustomStartCommand>, String> {
    let software = load_software_for_id(&installed_id)?;
    Ok(software.custom_start_command)
}

/// 保存自定义软件的启动命令
/// 复用 SoftwareManager::set_custom_start_command（含写锁 + 持久化）
#[tauri::command]
pub async fn save_custom_start_command(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    cmd: CustomStartCommand,
) -> Result<(), String> {
    let name = manager
        .find_installed(&installed_id)
        .map(|s| s.name)
        .unwrap_or_default();
    let target = format!("{} ({})", name, installed_id);
    audited_async!("save_start_command", target, "", {
        manager
            .set_custom_start_command(&installed_id, cmd)
            .map_err(|e| e.to_string())
    })
}

/// 列出内置自定义模板
/// 返回简化 JSON（前端按 id 选择模板后填表）
#[tauri::command]
pub async fn list_custom_templates() -> Result<Vec<serde_json::Value>, String> {
    let templates = custom_templates::builtin_templates();
    Ok(templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name_i18n": t.name_i18n,
                "executable": t.executable,
                "args": t.args,
                "config_file_relative": t.config_file_relative,
            })
        })
        .collect())
}

/// 保存启动设置（auto_start + startup_order + auto_restart）
#[tauri::command]
pub async fn save_startup_settings(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    auto_start: bool,
    order: u32,
    auto_restart: bool,
) -> Result<(), String> {
    let name = manager
        .find_installed(&installed_id)
        .map(|s| s.name)
        .unwrap_or_default();
    let target = format!("{} ({})", name, installed_id);
    audited_async!("save_startup", target, "", {
        manager
            .update_startup_settings(&installed_id, auto_start, order, auto_restart)
            .map_err(|e| e.to_string())
    })
}

/// 列出配置文件的备份列表
#[tauri::command]
pub async fn list_config_backups(installed_id: String) -> Result<Vec<serde_json::Value>, String> {
    let software = load_software_for_id(&installed_id)?;
    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
    let cctx = ConfigContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    };
    let file_path = provider
        .config_file_path(&cctx)
        .ok_or_else(|| "该软件无配置文件".to_string())?;
    let full_path = std::path::Path::new(&software.install_path).join(file_path);
    let backup_dir = full_path.parent().unwrap().join("backups");
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }
    let filename = full_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config");
    let suffix = format!("_{}", filename);
    let mut entries: Vec<_> = std::fs::read_dir(&backup_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.ends_with(&suffix))
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let backups: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let meta = e.metadata().ok();
            serde_json::json!({
                "name": e.file_name().to_string_lossy(),
                "size": meta.as_ref().map(|m| m.len()).unwrap_or(0),
                "modified": meta.and_then(|m| m.modified().ok())
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                    .unwrap_or(0),
            })
        })
        .collect();
    Ok(backups)
}

/// 从备份还原配置文件（覆盖当前配置，原始文件另存为新备份）
#[tauri::command]
pub async fn restore_config_backup(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    backup_name: String,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = format!("{} ({})", software.name, installed_id);
    let detail = String::new();

    audited_async!("restore_backup", target, detail, {
        let providers_list = providers::all_providers();
        let provider = providers_list
            .iter()
            .find(|p| p.key() == software.key)
            .ok_or_else(|| format!("未找到 provider: {}", software.key))?;
        let cctx = ConfigContext {
            install_path: software.install_path.clone(),
            version: software.version.clone(),
            config: software.config.clone(),
        };
        let file_path = provider
            .config_file_path(&cctx)
            .ok_or_else(|| "该软件无配置文件".to_string())?;
        let full_path = std::path::Path::new(&software.install_path).join(file_path);
        let backup_dir = full_path.parent().unwrap().join("backups");
        let backup_path = backup_dir.join(&backup_name);

        // 当前配置先备份，再还原
        config_editor::backup_config(&full_path).map_err(|e| e.to_string())?;
        std::fs::copy(&backup_path, &full_path).map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// 采样进程的 CPU / 内存使用
#[tauri::command]
pub fn sample_process_resources(
    pids: Vec<u32>,
) -> Result<Vec<crate::services::software_manager::process_monitor::ProcessSample>, String> {
    Ok(crate::services::software_manager::process_monitor::sample_processes(&pids))
}

// ===== C 扩展：日志查看器 + 备份/恢复 命令（任务 T2 / T3）=====

/// 获取某实例的日志来源列表（StdoutRedirect / ProviderFile）
#[tauri::command]
pub async fn get_log_sources(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<Vec<LogSource>, String> {
    let sw = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    log_viewer::list_log_sources(&sw).map_err(|e| e.to_string())
}

/// 读取日志（tail / 增量 / 历史分页 + 关键字/正则/级别过滤）
///
/// - offset = None → tail 末尾 limit 行（默认 2000）
/// - offset = Some(o), before = false → 从字节 o 向前（朝 EOF）读取增量
/// - offset = Some(o), before = true  → 读取字节 o 之前（朝文件头）的 limit 行（历史分页）
#[tauri::command]
pub async fn read_log(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    source_index: usize,
    archive_index: Option<usize>,
    offset: Option<u64>,
    before: Option<bool>,
    limit: Option<u64>,
    keyword: Option<String>,
    regex: bool,
    level: Option<String>,
) -> Result<LogChunk, String> {
    let limit = limit.map(|l| l as usize).unwrap_or(2000);
    let before = before.unwrap_or(false);
    let archive_index = archive_index.unwrap_or(0);
    let sw = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    log_viewer::read_log(
        &sw,
        source_index,
        offset,
        before,
        limit,
        keyword.as_deref(),
        regex,
        level.as_deref(),
        archive_index,
    )
    .map_err(|e| e.to_string())
}

/// 下载（拷贝）指定日志源到用户选择的路径
#[tauri::command]
pub async fn download_log(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    source_index: usize,
    dest_path: String,
) -> Result<(), String> {
    let sw = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let sources = log_viewer::list_log_sources(&sw).map_err(|e| e.to_string())?;
    let source = sources
        .get(source_index)
        .ok_or_else(|| format!("日志源索引越界: {}", source_index))?;
    log_viewer::download_log(&source.path, &dest_path).map_err(|e| e.to_string())
}

/// 合并导出：当前日志源 + 历史归档（旧→新）拼接为单一文件
#[tauri::command]
pub async fn export_combined_log(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    source_index: usize,
    dest_path: String,
) -> Result<(), String> {
    let sw = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let sources = log_viewer::list_log_sources(&sw).map_err(|e| e.to_string())?;
    let source = sources
        .get(source_index)
        .ok_or_else(|| format!("日志源索引越界: {}", source_index))?;
    log_viewer::export_combined_source(
        std::path::Path::new(&source.path),
        &source.archives,
        &dest_path,
    )
    .map_err(|e| e.to_string())
}

/// 全局日志关键字搜索（所有已装软件的日志源 + 归档）
#[tauri::command]
pub async fn search_all_logs(
    manager: State<'_, Arc<SoftwareManager>>,
    keyword: String,
    per_source_limit: Option<usize>,
    total_limit: Option<usize>,
) -> Result<Vec<opx_core::models::software::LogHit>, String> {
    Ok(log_viewer::search_all(
        &manager.get_installed(),
        &keyword,
        per_source_limit.unwrap_or(50),
        total_limit.unwrap_or(200),
    ))
}

/// 注册日志文件监听（notify），返回 watcher id（前端用于匹配事件）
#[tauri::command]
pub fn watch_log_file(path: String) -> Result<u64, String> {
    crate::services::software_manager::log_watcher::LogWatcher::register(&path)
        .ok_or_else(|| "日志监听启动失败".to_string())
}

/// 注销日志文件监听
#[tauri::command]
pub fn unwatch_log_file(id: u64) {
    crate::services::software_manager::log_watcher::LogWatcher::unregister(id);
}

/// 创建快照（压缩 data_dirs → <app_data>/backups/<id>/<ts>.zip，并写 manifest）
#[tauri::command]
pub async fn create_snapshot(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
    mode: BackupMode,
    name: Option<String>,
    note: Option<String>,
) -> Result<SnapshotMeta, String> {
    backup::create_snapshot(
        &manager,
        &sink_of(&app),
        &installed_id,
        mode,
        name,
        note,
        super::config::read_settings().unwrap_or_default().snapshot_keep.max(1) as usize,
    )
    .map_err(|e| e.to_string())
}

/// 列出某实例的全部快照
#[tauri::command]
pub async fn list_snapshots(installed_id: String) -> Result<Vec<SnapshotMeta>, String> {
    backup::list_snapshots(&installed_id).map_err(|e| e.to_string())
}

/// 恢复快照（运行态需先停服；跨大版本需 force 确认）
#[tauri::command]
pub async fn restore_snapshot(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    snapshot_id: String,
    force: bool,
) -> Result<(), String> {
    backup::restore_snapshot(&manager, &installed_id, &snapshot_id, force)
        .map_err(|e| e.to_string())
}

/// 删除快照（删 zip + 更新 manifest）
#[tauri::command]
pub async fn delete_snapshot(installed_id: String, snapshot_id: String) -> Result<(), String> {
    backup::delete_snapshot(
        &installed_id,
        &snapshot_id,
        super::config::read_settings().unwrap_or_default().snapshot_keep.max(1) as usize,
    )
    .map_err(|e| e.to_string())
}

/// 设置某实例的定时备份间隔（分钟；0 关闭）。返回全部配置。
#[tauri::command]
pub async fn set_backup_schedule(
    installed_id: String,
    minutes: u64,
) -> Result<std::collections::HashMap<String, u64>, String> {
    backup_scheduler::set_schedule(&installed_id, minutes).map_err(|e| e.to_string())
}

/// 查询某实例的定时备份间隔（分钟；0 表示未启用）
#[tauri::command]
pub async fn get_backup_schedule(installed_id: String) -> Result<u64, String> {
    Ok(backup_scheduler::get_schedule(&installed_id))
}

/// 一键重置（对每个 data_dir 重建空态，含护栏）
#[tauri::command]
pub async fn reset_instance(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<(), String> {
    // 审计目标：软件名 + 版本(id)；查不到则退回 id（不阻断重置本身，由 reset 内部报错）
    let (target, detail) = manager
        .find_installed(&installed_id)
        .map(|s| (s.name.clone(), format!("{} ({})", s.version, s.id)))
        .unwrap_or_else(|| (installed_id.clone(), installed_id.clone()));

    audited_async!("reset", target, detail, {
        backup::reset_instance(&manager, &installed_id).map_err(|e| e.to_string())
    })
}

/// 数字分段版本比较：5.7.44 < 8.0.36；7.4.9 < 7.10.0；
/// 任一段含非数字时退化为字符串比较（v1 < v2）。
///
/// 实现已下沉到 `opx-core::utils::semver::compare_versions`——升级检测与各 provider
/// 的远程版本排序必须共用同一份语义，搬进 core 后 `providers` 也能直接引用而
/// 无需反向依赖壳层 commands。此处 `pub use` 重导出以保持既有调用方零改动。
pub use opx_core::utils::semver::compare_versions;

/// 计算已装软件的升级目标：catalog 中高于当前版本的最高版本
fn compute_upgrades(
    installed: &[InstalledSoftware],
    catalog: &Catalog,
) -> Vec<opx_core::models::software::UpgradeInfo> {
    use opx_core::models::software::UpgradeInfo;
    let mut out = Vec::new();
    for sw in installed {
        if sw.is_custom || sw.category == Some(opx_core::models::software::SoftwareCategory::Runtime) {
            continue;
        }
        let Some(entry) = catalog.entries.iter().find(|e| e.key == sw.key) else {
            continue;
        };
        let target = entry
            .versions
            .iter()
            .map(|v| v.version.as_str())
            .filter(|v| compare_versions(v, &sw.version) == std::cmp::Ordering::Greater)
            .max_by(|x, y| compare_versions(x, y))
            .map(|s| s.to_string());
        out.push(UpgradeInfo {
            installed_id: sw.id.clone(),
            key: sw.key.clone(),
            name: entry.name.clone(),
            current_version: sw.version.clone(),
            target_version: target,
            rollback_to: scan_rollback_backup(&sw.install_path),
        });
    }
    out
}

/// 升级检测：基于当前 catalog（含内置 + 远程合并缓存）纯本地对比，立即返回
#[tauri::command]
pub fn check_upgrades(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Vec<opx_core::models::software::UpgradeInfo> {
    let catalog = manager.get_catalog();
    let installed = manager.get_installed();
    compute_upgrades(&installed, &catalog)
}

/// 扫描 install_path（apps/<key>/<cur>）所在 <key> 目录下的 *.bak.zip 文件，
/// 返回去 `.bak.zip` 后缀的文件名作为回滚目标版本；无备份返回 None。
/// 供 check_upgrades 填充 rollback_to（升级后同 key 会保留 <old_ver>.bak.zip）。
fn scan_rollback_backup(install_path: &str) -> Option<String> {
    let dir = opx_core::utils::paths::resolve_install_path(install_path);
    let key_dir = dir.parent()?;
    std::fs::read_dir(key_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            if !e.path().is_file() {
                return None;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".bak.zip").map(|s| s.to_string())
        })
}

/// 一键回滚：把升级时保留的 <old_ver>.bak.zip 解压恢复为旧版本目录，
/// 当前版本目录直接删除。同 key 记录保持一条，字段（version/name/install_path）回退到旧版。
#[tauri::command]
pub async fn rollback_software(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = software.name.clone();
    let detail = String::new();

    audited_async!("rollback", target, detail, {
        // 1. 解析 key、当前版本目录与同 key 的 .bak.zip 备份
        let key = software.key.clone();
        let key_dir = opx_core::utils::paths::apps_dir().join(&key);
        let old_install_path = opx_core::utils::paths::resolve_install_path(&software.install_path);

        let mut baks: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&key_dir)
            .map_err(|e| format!("读取目录失败: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".bak.zip")
                    .map(|v| (v.to_string(), e.path()))
            })
            .collect();
        baks.sort_by(|a, b| crate::commands::software::compare_versions(&a.0, &b.0));
        let (old_ver, bak_zip_path) = baks.pop().ok_or_else(|| "无可用回滚备份".to_string())?;

        // 2. 运行中/启动中 → 停止并等待
        if software.status == SoftwareStatus::Running || software.status == SoftwareStatus::Starting
        {
            let pid = software
                .pid
                .ok_or_else(|| "进程状态为运行中但无 PID，无法停止".to_string())?;
            let (ok, _) = tokio::task::spawn_blocking(move || lifecycle::stop_one(pid))
                .await
                .map_err(|e| format!("停止线程异常: {}", e))?;
            if !ok {
                return Err(format!(
                    "停止当前版本进程失败（PID {} 仍在运行），请先手动停止",
                    pid
                ));
            }
            manager
                .update_runtime_fields(
                    &software.id,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                    None,
                    None,
                )
                .map_err(|e| format!("状态更新失败: {}", e))?;
            lifecycle::unregister(&software.id);
        }

        // 3. 删当前版本目录 → 解压 .bak.zip 恢复旧版本目录
        let restore_path = key_dir.join(&old_ver);
        if old_install_path.exists() {
            std::fs::remove_dir_all(&old_install_path)
                .map_err(|e| format!("删除当前版本目录失败: {}", e))?;
        }
        if restore_path.exists() {
            std::fs::remove_dir_all(&restore_path)
                .map_err(|e| format!("清理旧版本目录失败: {}", e))?;
        }
        unzip_to(&bak_zip_path, &restore_path).map_err(|e| format!("解压恢复备份失败: {}", e))?;

        // 回滚后旧备份使命结束：删除 .bak.zip，避免残留导致同一版本可无限回滚
        let _ = std::fs::remove_file(&bak_zip_path);

        // 4. 更新记录：version/name/install_path 回退到旧版，其余字段保持
        let catalog = manager.get_catalog();
        let catalog_name = catalog
            .entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| software.name.clone());
        let mut new_record = software.clone();
        new_record.version = old_ver.clone();
        new_record.name = format!("{} {}", catalog_name, old_ver);
        new_record.install_path = format!("{}/{}", key, old_ver);
        new_record.status = SoftwareStatus::Unknown;
        new_record.pid = None;
        new_record.last_error = None;

        let removed = manager
            .remove_installed(&installed_id)
            .map_err(|e| format!("删除旧记录失败: {}", e))?;
        manager.add_installed(new_record).map_err(|e| {
            let _ = manager.add_installed(removed);
            format!("写入新记录失败: {}", e)
        })?;

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    // `post_init_already_done` / `probe_reports_done` 的单测已随函数迁入
    // `opx_core::services::software_manager::start_stop`（批次 3B）——
    // 单测与被测函数同处一个 crate，故此处不再引用。
    use super::compare_versions;
    use std::cmp::Ordering;

    use opx_core::models::software::{
        Catalog, CatalogEntry, CatalogVersion, InstallSource, InstalledSoftware, SoftwareCategory,
        SoftwareStatus,
    };

    fn dummy_installed(key: &str, version: &str) -> InstalledSoftware {
        InstalledSoftware {
            id: format!("{}-{version}", key),
            key: key.into(),
            version: version.into(),
            name: format!("{key} {version}"),
            install_path: format!("{key}/{version}"),
            install_time: chrono::Utc::now().naive_utc(),
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: "m".into(),
                url: "http://x".into(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
            icon: String::new(),
            category: None,
            depends_on: vec![],
            auto_restart: false,
        }
    }

    fn dummy_catalog(key: &str, versions: &[&str]) -> Catalog {
        Catalog {
            updated_at: None,
            entries: vec![CatalogEntry {
                key: key.into(),
                name: key.to_uppercase(),
                description: String::new(),
                description_i18n: None,
                category: SoftwareCategory::Database,
                icon: String::new(),
                versions: versions
                    .iter()
                    .map(|v| CatalogVersion {
                        version: v.to_string(),
                        mirrors: vec![],
                        archive: opx_core::models::software::ArchiveInfo {
                            format: opx_core::models::software::ArchiveFormat::Zip,
                            size: None,
                            sha256: None,
                        },
                    })
                    .collect(),
                default_version: versions[0].to_string(),
            }],
        }
    }

    #[test]
    fn compute_upgrades_picks_highest() {
        let installed = vec![dummy_installed("mysql", "5.7.44")];
        let catalog = dummy_catalog("mysql", &["5.7.44", "8.0.36", "8.4.0"]);
        let ups = super::compute_upgrades(&installed, &catalog);
        assert_eq!(ups.len(), 1);
        assert_eq!(ups[0].target_version.as_deref(), Some("8.4.0"));
    }

    /// 回归：同一 key 并存多实例时，升级状态必须按实例判定。
    /// 曾按 key 聚合，导致旧实例的"可升级"标记到已是最新版的实例上。
    #[test]
    fn compute_upgrades_is_per_instance_not_per_key() {
        let installed = vec![
            dummy_installed("minio", "RELEASE.2026-09-16T00-00-00Z"),
            dummy_installed("minio", "RELEASE.2025-04-22"),
        ];
        let catalog = dummy_catalog(
            "minio",
            &["RELEASE.2026-09-16T00-00-00Z", "RELEASE.2025-04-22"],
        );
        let ups = super::compute_upgrades(&installed, &catalog);
        assert_eq!(ups.len(), 2);
        assert_eq!(ups[0].installed_id, "minio-RELEASE.2026-09-16T00-00-00Z");
        assert_eq!(ups[1].installed_id, "minio-RELEASE.2025-04-22");

        assert_eq!(
            ups[0].target_version, None,
            "已是最新版的实例不应被标记为可升级"
        );
        assert_eq!(
            ups[1].target_version.as_deref(),
            Some("RELEASE.2026-09-16T00-00-00Z"),
            "旧版实例应可升级到新版"
        );
    }

    #[test]
    fn compute_upgrades_none_when_latest() {
        let installed = vec![dummy_installed("mysql", "8.4.0")];
        let catalog = dummy_catalog("mysql", &["5.7.44", "8.0.36", "8.4.0"]);
        let ups = super::compute_upgrades(&installed, &catalog);
        assert_eq!(ups[0].target_version, None);
    }

    #[test]
    fn version_cross_major() {
        assert_eq!(compare_versions("5.7.44", "8.0.36"), Ordering::Less);
    }
    #[test]
    fn version_equal() {
        assert_eq!(compare_versions("8.0.36", "8.0.36"), Ordering::Equal);
    }
    #[test]
    fn version_prefix_longer_is_greater() {
        assert_eq!(compare_versions("1.31.2", "1.31"), Ordering::Greater);
    }
    #[test]
    fn version_numeric_segment() {
        assert_eq!(compare_versions("7.4.9", "7.10.0"), Ordering::Less);
    }
    #[test]
    fn version_non_numeric_falls_back_to_string() {
        assert_eq!(compare_versions("v1", "v2"), Ordering::Less);
    }

    #[test]
    fn copy_paths_migrates_relative_and_absolute_data_dirs() {
        use std::path::PathBuf;
        // 构造临时目录：old/data/keep.txt、old/conf/nginx.conf
        let tmp = std::env::temp_dir().join(format!("opx_upg_test_{}", uuid::Uuid::new_v4()));
        let old = tmp.join("old");
        let bak = tmp.join("old.bak");
        let new = tmp.join("new");
        std::fs::create_dir_all(old.join("data")).unwrap();
        std::fs::create_dir_all(old.join("conf")).unwrap();
        std::fs::write(old.join("data").join("keep.txt"), b"x").unwrap();
        std::fs::write(old.join("conf").join("nginx.conf"), b"server {}").unwrap();
        std::fs::rename(&old, &bak).unwrap();

        // 混合：相对 data/、绝对 {old}/data（模拟默认 provider 返回绝对路径）
        let data_dirs = vec![PathBuf::from("data"), old.join("data")];
        let config = Some(PathBuf::from("conf/nginx.conf"));
        super::copy_paths_to_new(&bak, &old, &new, &data_dirs, &config).unwrap();

        assert!(new.join("data").join("keep.txt").exists());
        assert!(new.join("conf").join("nginx.conf").exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn copy_paths_migrates_absolute_config_file_path() {
        use std::path::PathBuf;
        // 绝对路径 config_file_path（模拟 nginx 返回 {install_path}/conf/nginx.conf）
        let tmp = std::env::temp_dir().join(format!("opx_upg_test_{}", uuid::Uuid::new_v4()));
        let old = tmp.join("old");
        let bak = tmp.join("old.bak");
        let new = tmp.join("new");
        let old_install_path = tmp.join("apps").join("nginx");
        std::fs::create_dir_all(old.join("conf")).unwrap();
        std::fs::write(old.join("conf").join("nginx.conf"), b"server {}").unwrap();
        std::fs::rename(&old, &bak).unwrap();

        let data_dirs: Vec<PathBuf> = vec![];
        // 绝对路径 config：{install_path}/conf/nginx.conf，需归一化后迁到新目录 conf/nginx.conf
        let config = Some(old_install_path.join("conf").join("nginx.conf"));
        super::copy_paths_to_new(&bak, &old_install_path, &new, &data_dirs, &config).unwrap();

        assert!(new.join("conf").join("nginx.conf").exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn zip_dir_unzip_to_roundtrip() {
        // 临时目录写 2 层文件 → 压缩 → 解压到另一临时目录 → 断言内容与相对路径一致
        let tmp = std::env::temp_dir().join(format!("opx_zip_rt_{}", uuid::Uuid::new_v4()));
        let src = tmp.join("src");
        let dst = tmp.join("dst");
        std::fs::create_dir_all(src.join("sub/dir")).unwrap();
        std::fs::write(src.join("root.txt"), b"root").unwrap();
        std::fs::write(src.join("sub/dir/deep.txt"), b"deep middleware").unwrap();
        std::fs::write(src.join("sub/a.txt"), b"a").unwrap();

        let zip_path = tmp.join("backup.bak.zip");
        super::zip_dir(&src, &zip_path).unwrap();

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        // 断言相对路径（/ 分隔，无绝对路径/.. ）
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"root.txt".to_string()));
        assert!(names.contains(&"sub/dir/deep.txt".to_string()));
        assert!(names.contains(&"sub/a.txt".to_string()));
        assert!(names
            .iter()
            .all(|n| !n.starts_with('/') && !n.contains("..")));

        super::unzip_to(&zip_path, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("root.txt")).unwrap(), b"root");
        assert_eq!(
            std::fs::read(dst.join("sub/dir/deep.txt")).unwrap(),
            b"deep middleware"
        );
        assert_eq!(std::fs::read(dst.join("sub/a.txt")).unwrap(), b"a");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// 模拟软件目录：<apps_dir>/<key>/<cur> + <key>/<old>.bak.zip
    fn make_apps_layout(tmp: &std::path::Path, key: &str, cur: &str, old: &str) {
        let apps = tmp.join("apps");
        let cur_dir = apps.join(key).join(cur);
        std::fs::create_dir_all(&cur_dir).unwrap();
        std::fs::write(cur_dir.join("f.txt"), b"x").unwrap();
        // 旧版本备份 zip：内容即一个文件
        let src = tmp.join("oldsrc");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("old.txt"), b"old").unwrap();
        super::zip_dir(&src, &apps.join(key).join(format!("{}.bak.zip", old))).unwrap();
    }

    #[test]
    fn scan_rollback_backup_sees_bak_zip() {
        let tmp = std::env::temp_dir().join(format!("opx_scan_rt_{}", uuid::Uuid::new_v4()));
        make_apps_layout(&tmp, "mysql", "8.0.36", "5.7.44");
        let install_path = tmp.join("apps/mysql/8.0.36").to_string_lossy().into_owned();
        // paths::resolve_install_path 对绝对路径直接返回
        assert_eq!(
            super::scan_rollback_backup(&install_path).as_deref(),
            Some("5.7.44")
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
