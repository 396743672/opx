use tauri::{AppHandle, Manager};
use crate::services::process_registry;

/// 触发退出流程：级联停止所有已注册子服务（逐个 emit stop-progress，完成后 emit stop-complete）。
/// 不在此处直接退出——前端监听 stop-complete 后展示「已安全退出」再调用 exit_app 真正退出，
/// 以确保停止进度对话框能完整渲染。
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    // 软件管理模块：停止所有运行中的实例（含 5s 优雅等待 + 强杀兜底）
    crate::services::software_manager::lifecycle::stop_all_on_exit();
    let _results = process_registry::stop_all(&app);
    Ok(())
}

/// 真正退出程序（前端在 stop-complete 后调用）
#[tauri::command]
pub fn exit_app(app: AppHandle) -> Result<(), String> {
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