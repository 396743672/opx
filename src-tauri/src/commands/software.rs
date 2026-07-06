use std::sync::Arc;

use chrono::Local;
use tauri::{AppHandle, Emitter, State};

use crate::models::software::{
    CatalogEntry, ConfigSchema, CustomInstallParams, CustomStartCommand, InstallParams,
    InstalledSoftware, JreUsageReport, SoftwareStatus, UninstallSafetyReport,
};
use crate::services::software_manager::{
    catalog, config_editor, health_check, installer, lifecycle, providers, uninstall_guard,
    SoftwareManager,
};
use crate::services::software_manager::config_editor::FormData;
use crate::services::software_manager::providers::custom_templates;
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
            // 关键：把远程版本合并进 manager 的 catalog，
            // 否则安装时 install_software 从 catalog 查不到网络版本，报"不支持版本"
            manager.merge_entry_versions(&key, versions.clone());

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
    // 复查卸载安全性
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let report = uninstall_guard::check_uninstall_safety(&software)
        .map_err(|e| e.to_string())?;
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

    // 删除记录 + 安装目录
    manager
        .remove_installed(&installed_id)
        .map_err(|e| e.to_string())?;

    let _ = app.emit("software-uninstalled", &installed_id);
    Ok(true)
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
        // 检查 data 目录是否已存在且非空（如之前初始化失败残留）
        // 非空则跳过初始化（假设已初始化，避免 --initialize-insecure 因目录非空失败）
        let data_dir = fri.init_command.working_dir.join("data");
        let data_already_initialized = data_dir.exists()
            && std::fs::read_dir(&data_dir)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
        if !initialized && data_already_initialized {
            tracing::info!(
                installed_id = %installed_id,
                "data dir non-empty, skipping first_run_init"
            );
            // 标记 initialized = true
            let mut new_config = software.config.clone();
            if let Some(obj) = new_config.as_object_mut() {
                obj.insert("initialized".to_string(), serde_json::json!(true));
            } else {
                new_config = serde_json::json!({ "initialized": true });
            }
            manager.update_config(installed_id, new_config)?;
        } else if !initialized {
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
            let init_result = tokio::task::spawn_blocking(move || lifecycle::run_first_run_init(&fri))
                .await
                .map_err(|e| anyhow::anyhow!("初始化任务 join 失败: {}", e));

            // 初始化失败时恢复状态为 Error（否则会卡在 Initializing 无法卸载/重启）
            let output = match init_result {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    let msg = format!("初始化失败：{}", e);
                    manager.update_runtime_fields(
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        None,
                        None,
                        Some(msg.clone()),
                    )?;
                    lifecycle::emit_status_changed(
                        app,
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        Some(msg),
                    );
                    return Err(e);
                }
                Err(e) => {
                    let err = anyhow::anyhow!("初始化任务 join 失败: {}", e);
                    manager.update_runtime_fields(
                        installed_id,
                        SoftwareStatus::Error,
                        None,
                        None,
                        None,
                        Some(format!("{}", err)),
                    )?;
                    return Err(err);
                }
            };
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

    // spawn 子进程（打印完整命令方便诊断启动问题）
    tracing::info!(
        installed_id = %installed_id,
        program = %cmd.program,
        args = ?cmd.args,
        working_dir = %cmd.working_dir.display(),
        env_vars = ?cmd.env_vars,
        "spawning software"
    );

    let child = lifecycle::spawn_process(cmd)?;
    let pid = child.id();

    // 更新状态为 Starting，清除旧的 last_error（避免启动成功后仍显示旧错误）
    manager.update_runtime_fields(
        installed_id,
        SoftwareStatus::Starting,
        Some(pid),
        Some(Local::now().naive_local()),
        None,
        None,
    )?;
    let _ = manager.clear_last_error(installed_id);

    // 注册到 lifecycle
    let kind = format!("{:?}", provider.catalog_entry().category);
    lifecycle::register(
        installed_id.to_string(),
        pid,
        format!("{} {}", software.name, software.version),
        software.key.clone(),
        kind,
    );

    // emit 时 error 显式传 None（清除前端旧错误）
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
    let pid_for_check = pid;
    tokio::spawn(async move {
        // 健康检查前先检查进程是否存活（避免进程崩溃后误报"健康检查超时"）
        let pid_alive = tokio::task::spawn_blocking(move || {
            health_check::is_process_alive(pid_for_check)
        })
        .await
        .unwrap_or(false);
        if !pid_alive {
            let _ = manager_clone.update_runtime_fields(
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                None,
                None,
                Some("进程意外退出（启动后立即崩溃，请检查端口冲突或 data 目录权限）".to_string()),
            );
            lifecycle::emit_status_changed(
                &app_clone,
                &installed_id_clone,
                SoftwareStatus::Error,
                None,
                Some("进程意外退出".to_string()),
            );
            lifecycle::unregister(&installed_id_clone);
            tracing::error!(
                installed_id = %installed_id_clone,
                pid = pid_for_check,
                "process exited immediately after spawn"
            );
            return;
        }
        // 60 次 × 1s = 最多 60s，给慢启动软件（如 MinIO/RustFS）足够 ready 时间
        let result = health_check::run_health_check(&spec, pid_alive, 60, 1000).await;
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
                &app,
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

/// 读表单数据：优先从 installed.json 的 config 字段读（权威来源），
/// 缺失字段用 schema default_value 兜底
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

    // 从 installed.json 的 config 字段读，缺失用 default_value
    let mut form = FormData::new();
    for field in &schema.fields {
        let value = software
            .config
            .get(&field.key)
            .cloned()
            .unwrap_or_else(|| field.default_value.clone());
        form.insert(field.key.clone(), value);
    }
    Ok(form)
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

// ===== 卸载校验 + 自定义启动命令 + 启动设置命令（任务 10.3）=====

/// 检查卸载是否安全
/// - 运行中/启动中/停止中/初始化中 → 阻止
/// - JRE 且是默认或被依赖 → 阻止
#[tauri::command]
pub async fn check_uninstall_safety(
    installed_id: String,
) -> Result<UninstallSafetyReport, String> {
    let software = load_software_for_id(&installed_id)?;
    uninstall_guard::check_uninstall_safety(&software).map_err(|e| e.to_string())
}

/// 检查 JRE 是否被使用（默认 JRE / 被 SpringBoot 应用依赖）
#[tauri::command]
pub async fn check_jre_in_use(
    jre_installed_id: String,
) -> Result<JreUsageReport, String> {
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
    manager
        .set_custom_start_command(&installed_id, cmd)
        .map_err(|e| e.to_string())
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

/// 保存启动设置（auto_start + startup_order）
#[tauri::command]
pub async fn save_startup_settings(
    manager: State<'_, Arc<SoftwareManager>>,
    installed_id: String,
    auto_start: bool,
    order: u32,
) -> Result<(), String> {
    manager
        .update_startup_settings(&installed_id, auto_start, order)
        .map_err(|e| e.to_string())
}
