use crate::models::settings::AppSettings;
use crate::utils::paths;
use crate::{audited, audited_async};
use std::fs;
use tauri::AppHandle;

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
    let target = format!("{} ({})", provider, zone);
    audited_async!("test_dns_token", target, "", {
        let zone = crate::commands::website::sanitize_domain(&zone)?;
        let p = crate::services::acme::dns::provider_for_account(
            &crate::models::dns_account::DnsAccount {
                id: String::new(),
                name: String::new(),
                provider: provider.clone(),
                token: token.clone(),
                access_key_id: token.clone(),
                access_key_secret: token.clone(),
                zones: vec![],
                tested_at: None,
            },
        )
        .ok_or_else(|| format!("不支持的服务商: {}", provider))?;
        let fqdn = format!("_opx-token-test.{}", zone);
        let value = format!("opx-{}", chrono::Local::now().timestamp_millis());
        p.set_value(&fqdn, "TXT", &value)
            .await
            .map_err(|e| format!("{:#}", e))?;
        let _ = p.delete_value(&fqdn, "TXT").await; // 清理探针记录（尽力而为）
        Ok(())
    })
}

/// 保存设置（原子写：写 .tmp 再 rename）
#[tauri::command]
pub fn save_settings(_app: AppHandle, settings: AppSettings) -> Result<(), String> {
    audited!("save_settings", "all", "", {
        let path = paths::settings_path();
        let tmp = path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
        fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
        // 立即刷新下载代理配置
        crate::utils::download::init_download_config(settings.github_proxy_url, settings.proxy_url);
        Ok(())
    })
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

/// 发送测试通知：构造固定告警事件，按当前设置对启用的渠道真实发送一遍。
/// 未配置任何渠道时报错提示。
#[tauri::command]
pub async fn test_alert_webhook() -> Result<(), String> {
    let e = crate::services::system_monitor::notify::AlertEvent {
        name: "测试".to_string(),
        metric: "cpu".to_string(),
        value: 95.0,
        threshold: 90,
    };
    let s = read_settings()?;
    let mut sent = false;
    if !s.alert_webhook_url.trim().is_empty() {
        crate::services::system_monitor::notify::send_webhook(
            &s.alert_webhook_url,
            &s.alert_webhook_format,
            &s.alert_webhook_secret,
            &e,
        )
        .await
        .map_err(|err| format!("webhook 发送失败: {:#}", err))?;
        sent = true;
    }
    if s.smtp_enabled && !s.smtp_host.trim().is_empty() && !s.smtp_to.trim().is_empty() {
        crate::services::system_monitor::notify::send_mail(&s, &e)
            .await
            .map_err(|err| format!("邮件发送失败: {:#}", err))?;
        sent = true;
    }
    if !sent {
        return Err("未配置任何通知渠道（webhook URL 为空且 SMTP 未启用）".to_string());
    }
    Ok(())
}

/// 立即执行一轮 DDNS 同步（设置页「立即同步」按钮）：走与调度器同一条 `sync_once`，
/// 返回人读报告。未启用 / 未配置域名 / 无凭证时同步报错。
#[tauri::command]
pub async fn sync_ddns_now() -> Result<String, String> {
    let s = read_settings()?;
    let r = crate::services::ddns::sync_once(&s, &crate::services::ddns::ddns_account_of(&s))
        .await
        .map_err(|e| format!("{:#}", e))?;
    let mut report = format!("公网 IP: {}", r.v4);
    if let Some(v6) = r.v6 {
        report.push_str(&format!(" / IPv6: {}", v6));
    }
    if r.changes.is_empty() {
        report.push_str("；所有记录未变");
    } else {
        report.push('；');
        report.push_str(&r.changes.join("；"));
    }
    if r.failures > 0 {
        report.push_str(&format!("（{} 条失败）", r.failures));
    }
    Ok(report)
}
