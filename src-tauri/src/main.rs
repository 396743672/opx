#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            {
                let tray = app.tray();
                // 系统托盘菜单会在后续初始化
            }
            Ok(())
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