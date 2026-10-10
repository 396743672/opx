use opx_core::models::settings::AppSettings;
use opx_core::utils::paths;
use opx_core::audited;
use std::fs;

/// 读取设置；文件缺失或解析失败返回默认值。
/// 路径相对 exe 所在目录（便携布局），不再使用外部 APPDATA。
#[tauri::command]
pub fn get_settings() -> AppSettings {
    let path = paths::settings_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 读取 settings.json；文件缺失或内容损坏时返回默认设置（并记录告警）。
///
/// 实现已下沉到 `opx-core::utils::settings::read_settings`（纯文件读取，零宿主依赖），
/// 此处 `pub use` 重导出以保持既有 5 处调用方（`renew_scheduler` / `ddns/scheduler` /
/// `backup_scheduler` / `notify` / `recorder`）零改动。
pub use opx_core::utils::settings::read_settings;

/// 保存设置（原子写：写 .tmp 再 rename）
#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    audited!("save_settings", "all", "", {
        let path = paths::settings_path();
        let tmp = path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
        fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
        opx_core::utils::http::set_global_proxy(&settings.proxy_url);
        // 立即刷新下载代理配置
        opx_core::utils::download::init_download_config(settings.github_proxy_url, settings.proxy_url);
        Ok(())
    })
}

/// 读取当前程序是否已开机自启（跨平台：Linux XDG / macOS LaunchAgent / Windows 注册表）。
#[tauri::command]
pub fn get_autostart() -> bool {
    crate::services::autostart::current().is_enabled()
}

/// 设置开机自启（跨平台**用户级**，无需提权；被管软件不在此范围——由 opx 启动后按各自
/// `auto_start_on_app_start` 配置拉起，见 `services/startup_bootstrap.rs`）。
#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let backend = crate::services::autostart::current();
    if !backend.is_supported() {
        return Ok(());
    }
    if enabled {
        let exe = std::env::current_exe().map_err(|e| format!("获取程序路径失败: {}", e))?;
        backend.enable(&exe).map_err(|e| e.to_string())
    } else {
        backend.disable().map_err(|e| e.to_string())
    }
}

/// 读取当前 Web 访问令牌（设置页展示 + 「打开浏览器」拼 `#token=` URL 用）。
///
/// 桌面专属：HTTP 分发器按 `desktop_only` 注册（409）——令牌明文不得经
/// HTTP 响应回传，重置/查看只能在桌面端进行（Web 端重置后无法把新令牌
/// 送回浏览器，等于把所有人锁在门外）。
#[tauri::command]
pub fn get_web_token() -> Result<String, String> {
    opx_http::token::load_or_generate().map_err(|e| format!("读取 token 失败: {e}"))
}

/// 重置 Web 访问令牌：重生成 → 受限写盘 → 触发 server 重启（若在跑）。
/// 旧会话的 Bearer 立即失效（重启后的 AppState 持新 token），前端 401 后
/// 由 TokenGate 引导重新输入。返回新令牌供设置页展示/拼 URL。
#[tauri::command]
pub async fn reset_web_token(app: tauri::AppHandle) -> Result<String, String> {
    let t = opx_http::token::generate();
    opx_http::token::store(&t).map_err(|e| format!("令牌写盘失败: {e}"))?;
    // 强制对齐：applied 中的旧 token 与新 token 必然不同 → supervisor 语义
    // 下 sync 会重启 server；此处直接同步调用拿到确定性的重启结果。
    crate::services::web_server::sync(&app)?;
    Ok(t)
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
