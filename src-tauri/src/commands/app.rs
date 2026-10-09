use tauri::{AppHandle, Manager};
use crate::oplog;

/// 触发退出流程：级联停止所有已注册子服务（逐个 emit stop-progress，完成后 emit stop-complete）。
/// ponytail: async + spawn_blocking，避免同步命令阻塞主线程（多服务逐个 taskkill 可能长达数秒）。
#[tauri::command]
pub async fn quit_app(app: AppHandle) -> Result<(), String> {
    oplog!("quit_app", "all");
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // spawn_blocking 要求 'static，故在闭包内部构造 sink（AppHandle 本身已是 owned）。
        let sink: std::sync::Arc<dyn crate::event_sink::EventSink> =
            std::sync::Arc::new(crate::event_sink::TauriEventSink::new(app2.clone()));
        crate::services::software_manager::lifecycle::stop_all_on_exit(&sink);
    });
    Ok(())
}

/// 真正退出程序（前端在 stop-complete 后调用）
#[tauri::command]
pub fn exit_app(app: AppHandle) -> Result<(), String> {
    oplog!("exit_app", "all");
    app.exit(0);
    Ok(())
}

/// 隐藏主窗口（收起到系统托盘）
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .hide()
            .map_err(|e| format!("隐藏窗口失败: {}", e))?;
    }
    Ok(())
}