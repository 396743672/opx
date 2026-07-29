use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use tauri::{AppHandle, Emitter, State};

use crate::models::software::{
    CatalogEntry, ConfigSchema, CustomInstallParams, CustomStartCommand, InstallParams,
    InstalledSoftware, JreUsageReport, SoftwareStatus, UninstallSafetyReport,
};
use crate::oplog;
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
    // ponytail: 优先用缓存，缓存不存在时用内置
    let catalog = catalog::load_catalog_cache().unwrap_or_else(|| catalog::build_builtin_catalog());
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
    catalog::save_catalog_cache(&merged); // ponytail: 缓存到文件
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
    oplog!("install", &params.key, &params.version);
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
    oplog!("install_custom", &params.name);
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
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    oplog!("uninstall", &software.name);
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
    init_password: Option<String>,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    oplog!("start", &software.name);

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
        let result = do_start_software(&manager_arc, &app_handle, &installed_id_for_task, init_password).await;
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

/// 收集软件启动将监听的端口，用于启动前占用校验。
/// 标准软件：取 config_schema 中 field_type=Port 的字段，从 config 读端口值（缺失回退字段默认值）。
/// 自定义软件：从 custom_start_command 的健康检查规格推导（Tcp 端口 / Http url 端口）。
fn collect_configured_ports(
    software: &InstalledSoftware,
    provider: &dyn providers::SoftwareProvider,
) -> Vec<u16> {
    use crate::models::software::{ConfigFieldType, CustomHealthSpec};
    let mut ports = Vec::new();
    if software.is_custom {
        if let Some(c) = &software.custom_start_command {
            match &c.health_check {
                CustomHealthSpec::Tcp { port } => ports.push(*port),
                CustomHealthSpec::Http { url, .. } => {
                    if let Some(p) = reqwest::Url::parse(url)
                        .ok()
                        .and_then(|u| u.port_or_known_default())
                    {
                        ports.push(p);
                    }
                }
                CustomHealthSpec::None => {}
            }
        }
    } else if let Some(schema) = provider.config_schema() {
        for field in &schema.fields {
            if matches!(field.field_type, ConfigFieldType::Port) {
                let v = software.config.get(&field.key).unwrap_or(&field.default_value);
                if let Some(p) = v.as_u64() {
                    if (1..=65535).contains(&p) {
                        ports.push(p as u16);
                    }
                }
            }
        }
    }
    ports
}

/// 启动软件内部实现（供 start_software / restart_software / auto_start 复用）
pub async fn do_start_software(
    manager: &Arc<SoftwareManager>,
    app: &AppHandle,
    installed_id: &str,
    init_password: Option<String>,
) -> anyhow::Result<()> {
    let software = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    let providers_list = providers::all_providers();
    let provider = providers_list
        .iter()
        .find(|p| p.key() == software.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", software.key))?;

    // 启动前端口占用校验：逐个检查配置中声明的端口是否已被占用，被占用则拒绝启动。
    // 放在此处（重启流程已先停旧进程）可避免把软件自身占用的端口误判为冲突。
    for port in collect_configured_ports(&software, &**provider) {
        if !health_check::is_port_free(port) {
            return Err(anyhow::anyhow!(
                "端口 {} 已被占用，无法启动。请修改配置端口或停止占用该端口的程序后重试。",
                port
            ));
        }
    }

    let start_ctx = StartContext {
        installed_id: software.id.clone(),
        install_path: software.install_path.clone(),
        version: software.version.clone(),
        config: software.config.clone(),
        custom_start_command: software.custom_start_command.clone(),
        init_password: init_password.clone(),
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

    // 捕获首次初始化通过 --init-file 注入的临时 SQL 文件路径（MySQL 设置 root 密码用）。
    // RC4 修复：不在 do_start_software 返回时立即删除（InitSqlGuard 竞态），
    // 而是延迟到健康检查完成后删除——此时 mysqld 已启动并必然读取过 --init-file。
    // spawn_process 失败时在此处立即清理。
    let init_sql_path: Option<PathBuf> = cmd
        .args
        .iter()
        .find_map(|a| a.strip_prefix("--init-file="))
        .map(std::path::PathBuf::from);

    // 首次初始化（如 mysqld --initialize-insecure）
    // 用 take() 取出所有权，避免后续 spawn_process(cmd) 时 cmd 仍被借用
    if let Some(fri) = cmd.first_run_init.take() {
        let initialized = software
            .config
            .get("initialized")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let data_dir = fri.init_command.working_dir.join("data");
        if initialized {
            // 已成功初始化过（config.initialized == true），直接跳过
            tracing::info!(
                installed_id = %installed_id,
                data_dir = %data_dir.display(),
                "already initialized, skipping first_run_init"
            );
        } else {
            // 未初始化：若 data 目录非空（上次 init 超时/kill 残留的半初始化文件，
            // 即 RC3 链式放大），先彻底清空再重新初始化，避免用损坏的 data 目录
            // 直接拉起 mysqld 导致卡死/崩溃。
            let wiped = lifecycle::wipe_data_dir_if_nonempty(&data_dir)?;
            if wiped {
                tracing::warn!(
                    installed_id = %installed_id,
                    data_dir = %data_dir.display(),
                    "data dir non-empty but not initialized; wiped before re-init (RC3)"
                );
            }
            // 继续执行下方初始化流程
            // 打印初始化命令方便诊断
            tracing::info!(
                installed_id = %installed_id,
                program = %fri.init_command.program,
                args = ?fri.init_command.args,
                working_dir = %fri.init_command.working_dir.display(),
                "running first_run_init"
            );

            // 确保 data 目录存在（MySQL 要求 datadir 存在）
            let _ = std::fs::create_dir_all(&data_dir);
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

    let child = match lifecycle::spawn_process(cmd) {
        Ok(child) => child,
        Err(e) => {
            if let Some(ref p) = init_sql_path {
                let _ = std::fs::remove_file(p);
            }
            return Err(e.into());
        }
    };
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
    let init_sql_path_for_cleanup = init_sql_path.clone();
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
            if let Some(ref p) = init_sql_path_for_cleanup {
                let _ = std::fs::remove_file(p);
            }
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
        // RC4: 健康检查完成后清理 init SQL 文件。
        // 此时 mysqld 已经历完整启动序列（或已退出），无论结果如何，
        // --init-file 都已被读取（或不再需要），可以安全删除明文密码文件。
        if let Some(ref p) = init_sql_path_for_cleanup {
            let _ = std::fs::remove_file(p);
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
    oplog!("stop", &software.name);

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

    // spawn_blocking 执行 stop_one（含 5s 优雅等待 + 强杀），加 15s 超时兜底
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::task::spawn_blocking(move || lifecycle::stop_one(pid)),
    ).await;

    let graceful = match result {
        Ok(Ok((true, _))) => true,
        other => {
            tracing::warn!(installed_id = %installed_id, pid = pid,
                stop_result = ?other, "stop_one incomplete/unexpected, forcing Stopped");
            false
        }
    };

    // 无论 stop_one 结果如何，确保状态更新为 Stopped
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
    init_password: Option<String>,
) -> Result<(), String> {
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    oplog!("restart", &software.name);

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
    if let Err(e) = do_start_software(&manager_arc, &app_clone, &installed_id_clone, init_password).await {
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
            &app_clone,
            &installed_id_clone,
            SoftwareStatus::Error,
            None,
            Some(msg.clone()),
        );
        return Err(msg);
    }
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
    oplog!("config_form", &format!("{} ({})", software.name, installed_id));
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
        config_editor::write_form_to_config(&full_path, &schema, &data).map_err(|e| e.to_string())?;
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
    oplog!("config_source", &format!("{} ({})", software.name, installed_id));
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
    let mut sw = list
        .software
        .into_iter()
        .find(|s| s.id == installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    sw.install_path = crate::utils::paths::resolve_install_path(&sw.install_path)
        .to_string_lossy()
        .to_string();
    Ok(sw)
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
    let name = manager.find_installed(&installed_id).map(|s| s.name).unwrap_or_default();
    oplog!("save_start_command", &format!("{} ({})", name, installed_id));
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
    let name = manager.find_installed(&installed_id).map(|s| s.name).unwrap_or_default();
    oplog!("save_startup", &format!("{} ({})", name, installed_id));
    manager
        .update_startup_settings(&installed_id, auto_start, order)
        .map_err(|e| e.to_string())
}

/// 列出配置文件的备份列表
#[tauri::command]
pub async fn list_config_backups(
    installed_id: String,
) -> Result<Vec<serde_json::Value>, String> {
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
    let filename = full_path.file_name().and_then(|n| n.to_str()).unwrap_or("config");
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
    oplog!("restore_backup", &format!("{} ({})", software.name, installed_id));
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
}
