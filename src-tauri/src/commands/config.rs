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

/// 读取 settings.json；文件缺失或内容损坏时返回默认设置（并记录告警）。
pub fn read_settings() -> Result<crate::models::settings::AppSettings, String> {
    let path = crate::utils::paths::settings_path();
    let Ok(s) = std::fs::read_to_string(&path) else {
        return Ok(Default::default());
    };
    match serde_json::from_str(&s) {
        Ok(v) => Ok(v),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "settings.json 解析失败，使用默认设置");
            Ok(Default::default())
        }
    }
}

/// 校验 DNS 服务商 Token 是否具备 DNS 写入权限（DNS-01 签发必需）。
/// 探针：在目标 zone 临时创建一条 TXT 再删除——这是唯一能区分「只读 Token」与
/// 「可写 Token」的最小验证（`/user/tokens/verify` 只验有效性、不验权限）。
#[tauri::command]
pub async fn test_dns_token(provider: String, token: String, zone: String) -> Result<(), String> {
    let zone = crate::commands::website::sanitize_domain(&zone)?;
    let p = crate::services::acme::dns::provider_for(&provider, &token)
        .ok_or_else(|| format!("不支持的服务商: {}", provider))?;
    let fqdn = format!("_opx-token-test.{}", zone);
    let value = format!("opx-{}", chrono::Local::now().timestamp_millis());
    p.create_txt(&fqdn, &value)
        .await
        .map_err(|e| format!("{:#}", e))?;
    let _ = p.delete_txt(&fqdn, &value).await; // 清理探针记录（尽力而为）
    oplog!("test_dns_token", &format!("{} ({})", provider, zone));
    Ok(())
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

fn run_key() -> winreg::RegKey {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )
        .expect("打开注册表 Run 键失败")
}

/// 读取当前程序是否已开机自启（注册表 Run 项含 OPX）
#[tauri::command]
pub fn get_autostart() -> bool {
    if !cfg!(windows) {
        return false;
    }
    run_key().get_value::<String, _>(RUN_VALUE).is_ok()
}

/// 设置开机自启（写/删注册表 Run 项，直连 WinAPI 无子进程）
#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
    let exe_path = exe.to_string_lossy().replace('/', "\\");
    let key = run_key();

    if enabled {
        let quoted = format!("\"{}\"", exe_path);
        key.set_value(RUN_VALUE, &quoted)
            .map_err(|e| format!("写入注册表失败: {}", e))?;
    } else {
        let _ = key.delete_value(RUN_VALUE);
    }
    Ok(())
}
