use tauri::{AppHandle, Manager};
use crate::services::process_registry;

/// 退出程序：先级联停止所有已注册子服务，再退出
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    // stop_all 内部 emit 进度事件并最终 emit stop-complete
    let _results = process_registry::stop_all(&app);
    // 无论 stop_all 是否全部成功，都退出
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