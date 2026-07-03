use std::sync::Arc;

use chrono::Local;
use tauri::{AppHandle, Emitter, State};

use crate::models::software::{
    CatalogEntry, ConfigSchema, CustomInstallParams, InstallParams, InstalledSoftware,
    SoftwareStatus,
};
use crate::services::software_manager::{
    catalog, config_editor, health_check, installer, lifecycle, providers, SoftwareManager,
};
use crate::services::software_manager::config_editor::FormData;
use crate::services::software_manager::providers::{ConfigContext, HealthContext, StartContext};

/// 获取可安装软件列表（catalog）
#[tauri::command]
pub async fn list_available_software(
    manager: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<CatalogEntry>, String> {
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
        let sp = crate::utils::paths::settings_path();
        if sp.exists() {
            std::fs::read_to_string(&sp)
                .ok()
                .and_then(|c| serde_json::from_str::<crate::models::settings::AppSettings>(&c).ok())
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
    let _ = app.emit("catalog-refreshed", merged.entries.clone());
    Ok(merged.entries)
}

/// 获取指定软件的远程版本列表（前端打开安装对话框时调用）
/// 返回包装后的 CatalogEntry（含远程版本），失败返回错误信息供前端展示
#[tauri::command]
pub async fn fetch_remote_versions_for(
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
            // 包装成单条 CatalogEntry 返回（前端合并到 catalog）
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
            // 该软件不支持动态拉取（如 MySQL/MinIO/RustFS），返回空
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
    Ok(manager.get_installed())
}

/// 安装预置软件（在线镜像）
#[tauri::command]
pub async fn install_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: InstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    tauri::async_runtime::spawn(async move {
        installer::install_software(app, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 安装用户上传的自定义压缩包
#[tauri::command]
pub async fn install_custom(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    params: CustomInstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let install_id_for_task = install_id.clone();
    tauri::async_runtime::spawn(async move {
        installer::install_custom(app, manager_arc, params, install_id_for_task).await;
    });
    Ok(install_id)
}

/// 卸载已安装软件
#[tauri::command]
pub async fn uninstall_software(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
) -> Result<bool, String> {
    manager
        .remove_installed(&installed_id)
        .map(|_| true)
        .map_err(|e| e.to_string())
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
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;

    lifecycle::validate_start_transition(software.status).map_err(|e| e.to_string())?;

    // PID 残留校验：旧 PID 仍存活则拒绝启动
    if let Some(pid) = software.pid {
        if health_check::is_process_alive(pid) {
            return Err(format!("进程 {} 仍在运行，请先停止", pid));
        }
    }

    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let installed_id_for_task = installed_id.clone();
    let app_handle = app.clone();

    // 异步执行启动流程，命令本身立即返回
    tauri::async_runtime::spawn(async move {
        let result = do_start_software(&manager_arc, &app_handle, &installed_id_for_task).await;
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
                &app_handle,
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

/// 启动软件内部实现（供 start_software / restart_software / auto_start 复用）
pub async fn do_start_software(
    manager: &Arc<SoftwareManager>,
    app: &AppHandle,
    installed_id: &str,
) -> anyhow::Result<()> {
    let software = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", software.key))?;

    let start_ctx = StartContext {
        installed_id: software.id.clone(),
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
        custom_start_command: software.custom_start_command.clone(),
    };

    // 构造 StartCommand（自定义软件走 build_custom_command，否则用 provider）
    let mut cmd = if software.is_custom {
        let custom = software
            .custom_start_command
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("自定义软件未配置启动命令"))?;
        lifecycle::build_custom_command(&software.install_path, custom)?
    } else {
        provider.start_command(&start_ctx)?
    };

    // 首次初始化（如 mysqld --initialize-insecure）
    // 用 take() 取出所有权，避免后续 spawn_process(cmd) 时 cmd 仍被借用
    if let Some(fri) = cmd.first_run_init.take() {
        let initialized = software
            .config
            .get("initialized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !initialized {
            manager.update_runtime_fields(
                installed_id,
                SoftwareStatus::Initializing,
                None,
                None,
                None,
                None,
            )?;
            lifecycle::emit_status_changed(
                app,
                installed_id,
                SoftwareStatus::Initializing,
                None,
                None,
            );

            // 用 spawn_blocking 包裹阻塞的 output() 调用
            // fri 的所有权移入闭包，闭包内 &fri 借用闭包自身拥有的数据，满足 'static
            let output = tokio::task::spawn_blocking(move || lifecycle::run_first_run_init(&fri))
                .await
                .map_err(|e| anyhow::anyhow!("初始化任务 join 失败: {}", e))??;
            let _ = output; // 暂不使用 stderr 输出（如 MySQL 临时密码），保留接口

            // 标记 initialized = true
            let mut new_config = software.config.clone();
            if let Some(obj) = new_config.as_object_mut() {
                obj.insert("initialized".to_string(), serde_json::json!(true));
            } else {
                new_config = serde_json::json!({ "initialized": true });
            }
            manager.update_config(installed_id, new_config)?;
        }
    }

    // spawn 子进程
    let child = lifecycle::spawn_process(cmd)?;
    let pid = child.id();

    // 更新状态为 Starting
    manager.update_runtime_fields(
        installed_id,
        SoftwareStatus::Starting,
        Some(pid),
        Some(Local::now().naive_local()),
        None,
        None,
    )?;

    // 注册到 lifecycle
    let kind = format!("{:?}", provider.catalog_entry().category);
    lifecycle::register(
        installed_id.to_string(),
        pid,
        format!("{} {}", software.name, software.version),
        software.key.clone(),
        kind,
    );

    lifecycle::emit_status_changed(app, installed_id, SoftwareStatus::Starting, Some(pid), None);
    tracing::info!(installed_id = %installed_id, pid = pid, "start_software spawned");

    // 异步健康检查（30 次 × 1s 间隔，最多 30s）
    let hctx = HealthContext {
        installed_id: installed_id.to_string(),
        install_path: software.install_path.clone(),
        port: software.port,
        config: software.config.clone(),
    };
    let spec = if software.is_custom {
        // 自定义软件的健康检查从 custom_start_command 推导
        match &software.custom_start_command {
            Some(c) => match &c.health_check {
                crate::models::software::CustomHealthSpec::None => {
                    crate::models::software::HealthCheckSpec::ProcessOnly
                }
                crate::models::software::CustomHealthSpec::Tcp { port } => {
                    crate::models::software::HealthCheckSpec::Tcp {
                        port: *port,
                        timeout_ms: 1000,
                    }
                }
                crate::models::software::CustomHealthSpec::Http { url, expected_status } => {
                    crate::models::software::HealthCheckSpec::Http {
                        url: url.clone(),
                        expected_status: *expected_status,
                        timeout_ms: 1000,
                    }
                }
            },
            None => crate::models::software::HealthCheckSpec::ProcessOnly,
        }
    } else {
        provider.health_check(&hctx)
    };

    let manager_clone = manager.clone();
    let app_clone = app.clone();
    let installed_id_clone = installed_id.to_string();
    tokio::spawn(async move {
        // 注意：这里 pid_alive=true 是初始假设；后续若发现进程已退出，
        // 由 health_check 返回 ProcessExited 时再处理
        let result = health_check::run_health_check(&spec, true, 30, 1000).await;
        match result {
            health_check::HealthCheckResult::Healthy => {
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Running,
                    Some(pid),
                    None,
                    None,
                    None,
                );
                lifecycle::emit_status_changed(
                    &app_clone,
                    &installed_id_clone,
                    SoftwareStatus::Running,
                    Some(pid),
                    None,
                );
                tracing::info!(installed_id = %installed_id_clone, "software healthy");
            }
            health_check::HealthCheckResult::Timeout => {
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    Some(pid),
                    None,
                    None,
                    Some("健康检查超时".to_string()),
                );
                lifecycle::emit_status_changed(
                    &app_clone,
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    Some(pid),
                    Some("健康检查超时".to_string()),
                );
            }
            health_check::HealthCheckResult::ProcessExited => {
                let _ = manager_clone.update_runtime_fields(
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    None,
                    None,
                    Some("进程意外退出".to_string()),
                );
                lifecycle::emit_status_changed(
                    &app_clone,
                    &installed_id_clone,
                    SoftwareStatus::Error,
                    None,
                    Some("进程意外退出".to_string()),
                );
                lifecycle::unregister(&installed_id_clone);
            }
        }
    });

    Ok(())
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

    lifecycle::validate_stop_transition(software.status).map_err(|e| e.to_string())?;

    let pid = software
        .pid
        .ok_or_else(|| "无 PID 记录，可能已停止".to_string())?;

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
    lifecycle::emit_status_changed(&app, &installed_id, SoftwareStatus::Stopping, None, None);

    // spawn_blocking 执行 stop_one（含 5s 优雅等待 + 强杀）
    let result = tokio::task::spawn_blocking(move || lifecycle::stop_one(pid))
        .await
        .map_err(|e| format!("停止任务失败: {}", e))?;

    let (success, _status) = result;
    let graceful = success; // 简化：成功即视为优雅停止

    // 更新状态为 Stopped
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    let app_clone = app.clone();
    let installed_id_clone = installed_id.clone();
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
        &app_clone,
        &installed_id_clone,
        SoftwareStatus::Stopped,
        None,
        None,
    );

    tracing::info!(installed_id = %installed_id_clone, pid = pid, "software stopped");
    Ok(graceful)
}

/// 重启软件
#[tauri::command]
pub async fn restart_software(
    manager: State<'_, Arc<SoftwareManager>>,
    app: AppHandle,
    installed_id: String,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;

    let should_stop = software.status == SoftwareStatus::Running
        || software.status == SoftwareStatus::Starting;
    if should_stop {
        let pid = software.pid.ok_or_else(|| "无 PID".to_string())?;
        let _ = tokio::task::spawn_blocking(move || lifecycle::stop_one(pid))
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
    let app_clone = app.clone();
    let installed_id_clone = installed_id.clone();
    let manager_arc: Arc<SoftwareManager> = manager.inner().clone();
    do_start_software(&manager_arc, &app_clone, &installed_id_clone)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
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
    Ok(provider.config_schema())
}

/// 读表单数据：根据 schema 从配置文件中提取字段值
#[tauri::command]
pub async fn read_config_form(
    installed_id: String,
) -> Result<FormData, String> {
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
    let cctx = ConfigContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    };
    let file_path = provider
        .config_file_path(&cctx)
        .ok_or_else(|| "该软件无配置文件".to_string())?;
    let full_path = std::path::Path::new(&software.install_path).join(file_path);
    config_editor::read_config_as_form(&full_path, &schema).map_err(|e| e.to_string())
}

/// 写表单数据：把表单字段写回配置文件，并同步更新 installed.json 的 config
#[tauri::command]
pub async fn write_config_form(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    data: FormData,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
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
    let cctx = ConfigContext {
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
    };
    let file_path = provider
        .config_file_path(&cctx)
        .ok_or_else(|| "该软件无配置文件".to_string())?;
    let full_path = std::path::Path::new(&software.install_path).join(file_path);
    config_editor::write_form_to_config(&full_path, &schema, &data).map_err(|e| e.to_string())?;

    // 同步 config 到 installed.json（MinIO/RustFS 无文件，仅更新 config 字段）
    let mut new_config = software.config.clone();
    if let Some(obj) = new_config.as_object_mut() {
        for (k, v) in &data {
            obj.insert(k.clone(), v.clone());
        }
    }
    manager
        .update_config(&installed_id, new_config)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读配置文件源码（整个文件内容）
#[tauri::command]
pub async fn read_config_source(
    installed_id: String,
) -> Result<String, String> {
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
}

/// 辅助：按 installed_id 从 installed.json 读单条记录（不依赖 State）
/// 适用于不需要修改 installed.json 的只读命令（如 get_config_schema、read_config_form）
fn load_software_for_id(
    installed_id: &str,
) -> Result<InstalledSoftware, String> {
    let path = crate::utils::paths::config_dir().join("installed.json");
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let list: crate::models::software::InstalledSoftwareList =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;
    list.software
        .into_iter()
        .find(|s| s.id == installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))
}
