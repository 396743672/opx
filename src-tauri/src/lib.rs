pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                let _tray = tauri::tray::TrayIconBuilder::new().build(app);
                // 系统托盘菜单会在后续初始化
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::system::process_list,
            commands::system::kill_process,
            commands::system::system_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
