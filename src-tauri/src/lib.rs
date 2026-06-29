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
        // 单例：第二实例启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = set_focus_safe(&window);
            }
        }))
        .setup(|app| {
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
            commands::system::process_list,
            commands::system::kill_process,
            commands::system::system_history,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::app::quit_app,
            commands::app::exit_app,
            commands::app::hide_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while starting tauri application");
}

fn set_focus_safe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.set_focus()
}
