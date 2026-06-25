#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, TrayIconBuilder, Menu, MenuItem, WindowEvent};
use std::sync::Mutex;
use std::sync::Arc;

fn main() {
    let tray_menu = Menu::new()
       .add_native_item(MenuItem::Quit);

    let app_handle: Arc<Mutex<Option<tauri::AppHandle>>> = Arc::new(Mutex::new(None));

    let _tray = TrayIconBuilder::new()
        .menu(tray_menu)
        .build()
        .expect("Failed to build tray icon");

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            {
                *app_handle.lock() = Some(app.app_handle().clone());
            }
            Ok(())
        })
        .on_window_event(|event| {
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                let settings = load_settings();
                match settings.close_window_action {
                    CloseWindowAction::MinimizeToTray => {
                        api.prevent_close();
                        event.window().hide().unwrap();
                    }
                    CloseWindowAction::Exit => {
                        // do nothing, let it close
                    }
                    CloseWindowAction::BackgroundService => {
                        api.prevent_close();
                        // keep running in background
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