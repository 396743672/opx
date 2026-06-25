#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, WindowEvent};
use tauri::menu::*;
use tauri::tray::*;
use std::sync::Mutex;

// 全局应用句柄存储
static APP_HANDLE: Mutex<Option<tauri::AppHandle>> = Mutex::new(None);

use crate::models::settings::CloseWindowAction;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
        #[cfg(desktop)]
        {
            *APP_HANDLE.lock() = Some(app.app_handle().clone());
        }

        // 创建托盘菜单
        let show_window = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
        let hide_window = MenuItemBuilder::with_id("hide", "隐藏窗口").build(app)?;
        let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

        let tray_menu = MenuBuilder::new(app)
            .items(&[&show_window, &hide_window])
            .add_native_item(MenuItemBuilder::separator())
            .items(&[&quit])
            .build()?;

        // 创建托盘图标
        let tray = TrayIconBuilder::new()
            .menu(&tray_menu)
            .icon(app.default_window_icon().cloned().unwrap_or_else(|| {
                // 如果没有默认图标，使用一个简单的图标作为后备
                TrayIcon::default_icon(app)
            }))
            .on_menu_event(move |app, event| match event.id().as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "hide" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
                "quit" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.close();
                    }
                    app.exit(0);
                }
                _ => (),
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        match window.is_visible() {
                            Ok(true) => let _ = window.hide(),
                            Ok(false) => {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            Err(_) => {},
                        }
                    }
                }
            })
            .build(app)?;

        // 保持托盘图标实例，防止被提前丢弃
        std::mem::forget(tray);

        // Auto start services when running as service
        if cfg!(not(feature = "dev")) {
            let app_root = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();

            let settings_path = app_root.join("config").join("settings.json");

            if let Ok(settings) = crate::utils::file::read_json::<crate::models::settings::AppSettings>(&settings_path) {
                if settings.auto_start_managed_services {
                    let _ = start_auto_started();
                }
            }
        }

        Ok(())
    })
    .on_window_event(|event| {
        if let WindowEvent::CloseRequested { api, .. } = event.event() {
            let settings = load_settings();
            match settings.close_window_action {
                CloseWindowAction::MinimizeToTray => {
                    api.prevent_close();
                    if let Err(e) = event.window().hide() {
                        eprintln!("Failed to hide window: {}", e);
                    }
                }
                CloseWindowAction::Exit => {
                    // do nothing, let it close
                }
                CloseWindowAction::BackgroundService => {
                    api.prevent_close();
                    // 这里可以添加后台运行的逻辑，比如继续运行服务
                    // 例如：启动一个后台任务，或者保持应用运行
                    tauri::async_runtime::spawn(async move {
                        // 后台运行的逻辑
                        println!("Application running in background");
                    });
                }
            }
        }
    })
    .invoke_handler(tauri::generate_handler![
        opx::commands::system::*,
        opx::commands::software::*,
        opx::commands::springboot::*,
        opx::commands::config::*,
        opx::commands::service::*,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

fn load_settings() -> crate::models::settings::AppSettings {
    let app_root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    let settings_path = app_root.join("config").join("settings.json");

    if !settings_path.exists() {
        return crate::models::settings::AppSettings::default();
    }

    crate::utils::file::read_json::<crate::models::settings::AppSettings>(&settings_path)
        .unwrap_or_default()
}