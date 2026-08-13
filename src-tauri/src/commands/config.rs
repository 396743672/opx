use std::fs;
use tauri::AppHandle;
use crate::models::settings::AppSettings;
use crate::utils::paths;
use crate::oplog;

/// 读取设置；文件缺失或解析失败返回默认值。
/// 路径相对 exe 所在目录（便携布局），不再使用外部 APPDATA。
#[tauri::command]
pub fn get_settings(_app: AppHandle) -> AppSettings {
    let path = paths::settings_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 保存设置（原子写：写 .tmp 再 rename）
#[tauri::command]
pub fn save_settings(_app: AppHandle, settings: AppSettings) -> Result<(), String> {
    oplog!("save_settings", "all");
    let path = paths::settings_path();
    let tmp = path.with_extension("json.tmp");
    let content =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
    // 立即刷新下载代理配置
    crate::utils::download::init_download_config(
        settings.github_proxy_url,
        settings.proxy_url,
    );
    Ok(())
}

const RUN_VALUE: &str = "OPX";

/// 开机自启注册表 Run 键（与 cc-switch 等便携应用一致）
fn run_key() -> String {
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run".to_string()
}

/// 读取当前程序是否已开机自启（注册表 Run 项含 OPX）
#[tauri::command]
pub fn get_autostart() -> bool {
    let out = std::process::Command::new("reg")
        .args(["query", &run_key(), "/v", RUN_VALUE])
        .output();
    match out {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// 设置开机自启（写/删注册表 Run 项）
#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
    let exe_path = exe.to_string_lossy().replace('/', "\\");

    if enabled {
        let quoted = format!("\"{}\"", exe_path);
        let status = std::process::Command::new("reg")
            .args(["add", &run_key(), "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &quoted, "/f"])
            .status()
            .map_err(|e| format!("执行 reg add 失败: {}", e))?;
        if !status.success() {
            return Err("设置开机自启失败".to_string());
        }
    } else {
        let _ = std::process::Command::new("reg")
            .args(["delete", &run_key(), "/v", RUN_VALUE, "/f"])
            .status();
    }
    Ok(())
}
