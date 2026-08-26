pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // 单例：第二实例启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = set_focus_safe(&window);
            }
        }))
        .setup(|app| {
            // 日志文件监听后台线程（notify 实时推送增量）
            crate::services::software_manager::log_watcher::LogWatcher::init(app.handle().clone());

            // 便携布局：启动时主动创建所有运行目录（exe 同级）
            {
                // 初始化内置 zip manifest（resource_dir/software/manifest.json）
                // Windows 上 resource_dir() 返回 exe 目录，资源实际在 resources/ 子目录下
                let manifest_path = app.path().resource_dir().ok().and_then(|d| {
                    crate::utils::paths::resolve_builtin_resource(&d, "software/manifest.json")
                });
                if let Some(mp) = manifest_path {
                    crate::services::software_manager::providers::init_builtin_manifest(&mp);
                }

                let _ = crate::utils::paths::apps_dir();
                let _ = crate::utils::paths::config_dir();
                let _ = crate::utils::paths::data_dir();
                let _ = crate::utils::paths::tmp_dir();
                let _ = crate::utils::paths::logs_dir();
                // settings.json 不存在时写入默认值，确保便携目录有可见配置
                let sp = crate::utils::paths::settings_path();
                if !sp.exists() {
                    let default = crate::models::settings::AppSettings::default();
                    if let Ok(json) = serde_json::to_string_pretty(&default) {
                        let _ = std::fs::write(&sp, json);
                    }
                }
                // 初始化下载代理配置
                if let Ok(content) = std::fs::read_to_string(&sp) {
                    if let Ok(settings) = serde_json::from_str::<crate::models::settings::AppSettings>(&content) {
                        crate::utils::download::init_download_config(
                            settings.github_proxy_url,
                            settings.proxy_url,
                        );
                    }
                }
            }

            // 注册 SoftwareManager State（用 Arc 包装，供命令层 clone 入后台 task）
            let software_mgr = std::sync::Arc::new(
                crate::services::software_manager::SoftwareManager::new(),
            );
            app.manage(software_mgr.clone());
            app.manage(std::sync::Arc::new(
                crate::services::website_manager::WebsiteManager::new(),
            ));
            let springboot_mgr = std::sync::Arc::new(
                crate::services::springboot_manager::SpringBootManager::new(),
            );
            app.manage(springboot_mgr.clone());
            // 注册 StackManager State（携带 SoftwareManager / SpringBootManager 的 Arc）
            app.manage(std::sync::Arc::new(
                crate::services::stack_manager::StackManager::new(software_mgr, springboot_mgr),
            ));

            // 初始化审计日志（tracing + 按日 rolling），并清理 7 天前的旧日志
            // guard 必须用 Mutex 包装后 manage 到 Tauri State，
            // 否则 setup 退出时 guard drop，tracing_appender 会停止 flush
            let _audit_guard = match crate::services::software_manager::audit_log::init() {
                Ok(g) => {
                    let log_dir = crate::utils::paths::logs_dir();
                    crate::services::software_manager::audit_log::cleanup_old_logs(&log_dir, 7);
                    Some(g)
                }
                Err(e) => {
                    eprintln!("[audit_log] 初始化失败: {}", e);
                    None
                }
            };
            if let Some(g) = _audit_guard {
                app.manage(std::sync::Mutex::new(g));
            }

            // auto_start 拉起：按 startup_order 升序拉起 auto_start=true 的实例
            // 后台异步执行，不阻塞 setup；单个实例慢启动不阻塞后续
            let app_handle_for_auto = app.handle().clone();
            let manager_arc = app
                .state::<std::sync::Arc<crate::services::software_manager::SoftwareManager>>()
                .inner()
                .clone();
            // 定时备份调度（在 manager_arc 被 auto_start spawn 捕获前克隆）
            let bs_manager = manager_arc.clone();
            let bs_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                crate::services::software_manager::lifecycle::auto_start_all(
                    &manager_arc,
                    &app_handle_for_auto,
                )
                .await;
            });

            // 服务组自启：启用 auto_start 的服务组在应用启动后按序拉起
            let app_handle_for_stack_auto = app.handle().clone();
            let stack_mgr_arc = app
                .state::<std::sync::Arc<crate::services::stack_manager::StackManager>>()
                .inner()
                .clone();
            tauri::async_runtime::spawn(async move {
                stack_mgr_arc.auto_start_all(&app_handle_for_stack_auto).await;
            });

            // 定时备份调度：后台循环按配置间隔自动对实例做 Hot 快照
            tauri::async_runtime::spawn(async move {
                crate::services::software_manager::backup_scheduler::run_scheduler(bs_manager, bs_app)
                    .await;
            });

            #[cfg(desktop)]
            {
                // 托盘右键菜单（R7：动态列出运行中软件，点击即停止）
                let tray_manager: std::sync::Arc<
                    crate::services::software_manager::SoftwareManager,
                > = app
                    .state::<std::sync::Arc<crate::services::software_manager::SoftwareManager>>()
                    .inner()
                    .clone();

                let (menu, tooltip) = build_tray_menu(app.handle(), &tray_manager)?;

                let icon = app.default_window_icon().cloned();
                let mut builder = TrayIconBuilder::new()
                    .menu(&menu)
                    .show_menu_on_left_click(false);
                if let Some(img) = icon {
                    builder = builder.icon(img);
                }
                if !tooltip.is_empty() {
                    builder = builder.tooltip(&tooltip);
                }
                let tray = builder
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "quit" => {
                            let _ = app.emit("close-requested", ());
                        }
                        other => {
                            // running_{installed_id}：转发给前端执行停止
                            if let Some(id) = other.strip_prefix("running_") {
                                let _ = app.emit("tray-software-stop", id.to_string());
                            }
                        }
                    })
                    .on_tray_icon_event(move |tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            show_main_window(app);
                        }
                    })
                    .build(app)?;

                // 软件状态变化时重建托盘，保持「运行中列表 + tooltip 运行数」同步
                let tray_for_listen = tray.clone();
                let manager_for_listen = tray_manager.clone();
                app.handle().listen("software-status-changed", move |_| {
                    let handle = tray_for_listen.app_handle().clone();
                    if let Ok((menu, tooltip)) = build_tray_menu(&handle, &manager_for_listen) {
                        let _ = tray_for_listen.set_menu(Some(menu));
                        let tooltip = if tooltip.is_empty() { None } else { Some(tooltip) };
                        let _ = tray_for_listen.set_tooltip(tooltip);
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.app_handle().emit("close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::system::system_history,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::config::get_autostart,
            commands::config::set_autostart,
            commands::app::quit_app,
            commands::app::exit_app,
            commands::app::hide_main_window,
            commands::software::list_available_software,
            commands::software::refresh_catalog,
            commands::software::list_installed_software,
            commands::software::install_software,
            commands::software::upgrade_software,
            commands::software::rollback_software,
            commands::software::install_custom,
            commands::software::uninstall_software,
            commands::software::fetch_remote_versions_for,
            commands::software::check_upgrades,
            commands::software::start_software,
            commands::software::stop_software,
            commands::software::restart_software,
            commands::software::get_software_status,
            commands::software::get_config_schema,
            commands::software::read_config_form,
            commands::software::write_config_form,
            commands::software::read_config_source,
            commands::software::write_config_source,
            commands::software::check_uninstall_safety,
            commands::software::check_jre_in_use,
            commands::software::get_custom_start_command,
            commands::software::save_custom_start_command,
            commands::software::list_custom_templates,
            commands::software::save_startup_settings,
            commands::software::list_config_backups,
            commands::software::restore_config_backup,
            commands::software::get_log_sources,
            commands::software::read_log,
            commands::software::download_log,
            commands::software::export_combined_log,
            commands::software::search_all_logs,
            commands::software::watch_log_file,
            commands::software::unwatch_log_file,
            commands::software::create_snapshot,
            commands::software::list_snapshots,
            commands::software::restore_snapshot,
            commands::software::delete_snapshot,
            commands::software::reset_instance,
            commands::software::set_backup_schedule,
            commands::software::get_backup_schedule,
            commands::software::sample_process_resources,
            commands::website::list_websites,
            commands::website::save_website,
            commands::website::delete_website,
            commands::website::set_website_enabled,
            commands::website::upload_site_bundle,
            commands::website::get_site_conf,
            commands::website::set_site_conf,
            commands::website::unlock_site_conf,
            commands::website::generate_self_signed_cert,
            commands::springboot::list_springboot_apps,
            commands::springboot::create_springboot_app,
            commands::springboot::update_springboot_app,
            commands::springboot::delete_springboot_app,
            commands::springboot::start_springboot_app,
            commands::springboot::stop_springboot_app,
            commands::springboot::restart_springboot_app,
            commands::springboot::replace_springboot_jar,
            commands::springboot::replace_springboot_jar_and_restart,
            commands::springboot::get_springboot_jvm_metrics,
            commands::springboot::list_springboot_groups,
            commands::springboot::save_springboot_groups,
            commands::springboot::get_springboot_global_env_vars,
            commands::springboot::set_springboot_global_env_vars,
            commands::springboot::get_recommended_jvm_opts,
            commands::springboot::list_springboot_dependency_candidates,
            commands::springboot::read_jar_version_info,
            commands::springboot::read_jar_port,
            commands::springboot::list_springboot_log_sources,
            commands::springboot::read_springboot_log,
            commands::springboot::download_springboot_log,
            commands::springboot::export_springboot_config,
            commands::springboot::import_springboot_config,
            commands::stack::list_stacks,
            commands::stack::get_stack,
            commands::stack::create_stack,
            commands::stack::update_stack,
            commands::stack::delete_stack,
            commands::stack::start_stack,
            commands::stack::stop_stack,
            commands::stack::restart_stack,
            commands::stack::export_stack,
            commands::stack::import_stack,
        ])
        .run(tauri::generate_context!())
        .expect("error while starting tauri application");
}

fn set_focus_safe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.set_focus()
}

/// 显示并聚焦主窗口（托盘恢复用）。Windows 前台锁可能让 set_focus 被忽略
/// （后台进程无法抢前台），用「置顶→取消」强制把窗口提到最前（社区通用做法）。
fn show_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        eprintln!("[tray] show_main_window: main window not found");
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = set_focus_safe(&window);
    let _ = window.set_always_on_top(true);
    let _ = window.set_always_on_top(false);
}

/// 构建含运行中软件列表的托盘菜单，并返回 tooltip 文本。
/// 菜单项：显示窗口 / (分隔) / 运行中软件(点击停止) / (分隔) / 退出。
#[cfg(desktop)]
fn build_tray_menu(
    app: &AppHandle,
    manager: &std::sync::Arc<crate::services::software_manager::SoftwareManager>,
) -> tauri::Result<(Menu<tauri::Wry>, String)> {
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let (running, tooltip) = running_softwares(&manager.get_installed());

    // 用 owned Box 持有全部菜单项，再取引用构造成异构图项数组（解决异构生命周期借用）
    let mut owned: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    if !running.is_empty() {
        owned.push(Box::new(PredefinedMenuItem::separator(app)?));
        for (id, name) in &running {
            owned.push(Box::new(MenuItem::with_id(
                app,
                format!("running_{}", id),
                name.clone(),
                true,
                None::<&str>,
            )?));
        }
    }
    owned.push(Box::new(PredefinedMenuItem::separator(app)?));
    owned.push(Box::new(quit_item));

    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> =
        owned.iter().map(|b| b.as_ref() as &dyn IsMenuItem<tauri::Wry>).collect();
    let menu = Menu::with_items(app, &refs)?;
    Ok((menu, tooltip))
}

/// 从已安装列表筛出运行中软件，返回 (id, name) 列表与 tooltip 文本。
/// 分离为纯函数以便单测验证筛选与 tooltip 逻辑。
fn running_softwares(
    installed: &[crate::models::software::InstalledSoftware],
) -> (Vec<(String, String)>, String) {
    use crate::models::software::SoftwareStatus;
    let running: Vec<_> = installed
        .iter()
        .filter(|s| s.status == SoftwareStatus::Running)
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();
    let tooltip = if running.is_empty() {
        String::new()
    } else {
        format!("运行中：{} 个软件", running.len())
    };
    (running, tooltip)
}

#[cfg(test)]
mod tests {
    use crate::models::software::{InstalledSoftware, SoftwareStatus};

    fn sample(status: SoftwareStatus) -> InstalledSoftware {
        InstalledSoftware {
            id: "id-1".into(),
            key: "k".into(),
            version: "1.0".into(),
            name: "测试软件".into(),
            install_path: "p".into(),
            install_time: chrono::NaiveDateTime::default(),
            status,
            port: 0,
            config: serde_json::Value::Null,
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: crate::models::software::InstallSource::Builtin { version: "1.0".into() },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
            icon: String::new(),
            category: None,
        }
    }

    #[test]
    fn running_softwares_filters_and_tooltip() {
        let list = vec![
            sample(SoftwareStatus::Running),
            sample(SoftwareStatus::Stopped),
            sample(SoftwareStatus::Error),
        ];
        let (running, tooltip) = super::running_softwares(&list);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].1, "测试软件");
        assert_eq!(tooltip, "运行中：1 个软件");
    }

    #[test]
    fn running_softwares_empty_tooltip() {
        let list = vec![sample(SoftwareStatus::Stopped)];
        let (running, tooltip) = super::running_softwares(&list);
        assert!(running.is_empty());
        assert!(tooltip.is_empty());
    }
}
