pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // 单例：第二实例启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = set_focus_safe(&window);
            }
        }))
        .setup(|app| {
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
            }

            // 注册 SoftwareManager State（用 Arc 包装，供命令层 clone 入后台 task）
            app.manage(std::sync::Arc::new(
                crate::services::software_manager::SoftwareManager::new(),
            ));
            app.manage(std::sync::Arc::new(
                crate::services::website_manager::WebsiteManager::new(),
            ));
            app.manage(std::sync::Arc::new(
                crate::services::springboot_manager::SpringBootManager::new(),
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
            tauri::async_runtime::spawn(async move {
                crate::services::software_manager::lifecycle::auto_start_all(
                    &manager_arc,
                    &app_handle_for_auto,
                )
                .await;
            });

            #[cfg(desktop)]
            {
                // 托盘右键菜单
                let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

                let app_handle = app.handle().clone();
                let icon = app.default_window_icon().cloned();
                let mut builder = TrayIconBuilder::new().menu(&menu).show_menu_on_left_click(false);
                if let Some(img) = icon {
                    builder = builder.icon(img);
                }
                let _tray = builder
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = set_focus_safe(&window);
                            }
                        }
                        "quit" => {
                            let _ = app.emit("close-requested", ());
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(move |tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = set_focus_safe(&window);
                            }
                        }
                    })
                    .build(app)?;
                let _ = app_handle;
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
            commands::app::quit_app,
            commands::app::exit_app,
            commands::app::hide_main_window,
            commands::software::list_available_software,
            commands::software::refresh_catalog,
            commands::software::list_installed_software,
            commands::software::install_software,
            commands::software::install_custom,
            commands::software::uninstall_software,
            commands::software::fetch_remote_versions_for,
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
            commands::website::list_websites,
            commands::website::save_website,
            commands::website::delete_website,
            commands::website::set_website_enabled,
            commands::website::upload_site_bundle,
            commands::website::get_site_conf,
            commands::website::set_site_conf,
            commands::website::unlock_site_conf,
            commands::springboot::list_springboot_apps,
            commands::springboot::create_springboot_app,
            commands::springboot::update_springboot_app,
            commands::springboot::delete_springboot_app,
            commands::springboot::start_springboot_app,
            commands::springboot::stop_springboot_app,
            commands::springboot::restart_springboot_app,
            commands::springboot::replace_springboot_jar,
            commands::springboot::get_springboot_jvm_metrics,
            commands::springboot::list_springboot_groups,
            commands::springboot::save_springboot_groups,
            commands::springboot::get_recommended_jvm_opts,
            commands::springboot::list_springboot_dependency_candidates,
            commands::springboot::read_jar_version_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while starting tauri application");
}

fn set_focus_safe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.set_focus()
}
