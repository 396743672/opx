use serde::{Deserialize, Serialize};

/// 关闭窗口时的默认行为
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CloseWindowAction {
    /// 收起到系统托盘，进程存活
    CloseToTray,
    /// 退出程序（级联停止子服务后结束）
    Exit,
}

impl Default for CloseWindowAction {
    fn default() -> Self {
        Self::CloseToTray
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub sidebar_collapsed: bool,
    pub software_root: String,
    pub config_root: String,
    pub mirror_url: String,
    pub auto_check_update: bool,
    pub close_window_action: CloseWindowAction,
    /// 关闭窗口时是否每次弹窗询问
    pub ask_on_close: bool,
    pub jre_default_id: Option<String>,
    /// GitHub 下载加速代理（如 https://ghfast.top），空=不使用
    pub github_proxy_url: String,
    /// 全局下载代理（如 http://127.0.0.1:7890），空=直连
    pub proxy_url: String,
    /// DNS 服务商（DNS-01 用），v1 支持 "cloudflare"
    #[serde(default = "default_dns_provider")]
    pub dns_provider: String,
    /// DNS 服务商 API Token（明文存储，UI 需提示风险）
    #[serde(default)]
    pub cloudflare_api_token: String,
    /// 使用 Let's Encrypt 测试环境（staging）
    #[serde(default)]
    pub acme_use_staging: bool,
    /// 告警阈值（百分比）。默认 90。
    #[serde(default = "default_ninety")]
    pub alert_system_cpu: u32,
    #[serde(default = "default_ninety")]
    pub alert_system_mem: u32,
    #[serde(default = "default_ninety")]
    pub alert_process_cpu: u32,
    #[serde(default = "default_ninety")]
    pub alert_process_mem: u32,
    // --- 告警通知（webhook + SMTP）---
    /// 告警 webhook URL，空 = 不发送
    #[serde(default)]
    pub alert_webhook_url: String,
    /// "json" | "dingtalk" | "wecom" | "feishu"
    #[serde(default = "default_webhook_format")]
    pub alert_webhook_format: String,
    /// 钉钉加签 secret（仅 dingtalk 生效，空 = 不加签）
    #[serde(default)]
    pub alert_webhook_secret: String,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_user: String,
    /// SMTP 授权码（明文存储，UI 提示风险）
    #[serde(default)]
    pub smtp_pass: String,
    /// 收件人，逗号分隔多个
    #[serde(default)]
    pub smtp_to: String,
    /// SMTP 总开关：false 时即使填了配置也不发
    #[serde(default)]
    pub smtp_enabled: bool,
}

fn default_dns_provider() -> String {
    "cloudflare".to_string()
}

fn default_ninety() -> u32 {
    90
}

fn default_webhook_format() -> String {
    "json".to_string()
}

fn default_smtp_port() -> u16 {
    465
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "auto".to_string(),
            language: "zh-CN".to_string(),
            sidebar_collapsed: false,
            software_root: "apps".to_string(),
            config_root: "config".to_string(),
            mirror_url: "https://mirrors.aliyun.com".to_string(),
            auto_check_update: true,
            close_window_action: CloseWindowAction::default(),
            ask_on_close: true,
            jre_default_id: None,
            github_proxy_url: "https://ghfast.top".to_string(),
            proxy_url: String::new(),
            dns_provider: default_dns_provider(),
            cloudflare_api_token: String::new(),
            acme_use_staging: false,
            alert_system_cpu: default_ninety(),
            alert_system_mem: default_ninety(),
            alert_process_cpu: default_ninety(),
            alert_process_mem: default_ninety(),
            alert_webhook_url: String::new(),
            alert_webhook_format: default_webhook_format(),
            alert_webhook_secret: String::new(),
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            smtp_user: String::new(),
            smtp_pass: String::new(),
            smtp_to: String::new(),
            smtp_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_deserialize_legacy_json_without_acme_fields() {
        // 真实旧 settings.json 的形状（不含新增三字段）
        let json = r#"{
            "theme":"auto","language":"zh-CN","sidebar_collapsed":false,
            "software_root":"apps","config_root":"config","mirror_url":"https://mirrors.aliyun.com",
            "auto_check_update":true,"close_window_action":"CloseToTray","ask_on_close":true,
            "jre_default_id":null,"github_proxy_url":"","proxy_url":""
        }"#;
        let s: AppSettings = serde_json::from_str(json).expect("legacy settings must load");
        assert_eq!(s.dns_provider, "cloudflare");
        assert!(s.cloudflare_api_token.is_empty());
        assert!(!s.acme_use_staging);
    }

    #[test]
    fn alert_thresholds_default_to_90_on_legacy_json() {
        let json = r#"{
            "theme":"auto","language":"zh-CN","sidebar_collapsed":false,
            "software_root":"apps","config_root":"config","mirror_url":"https://mirrors.aliyun.com",
            "auto_check_update":true,"close_window_action":"CloseToTray","ask_on_close":true,
            "jre_default_id":null,"github_proxy_url":"","proxy_url":""
        }"#;
        let s: AppSettings = serde_json::from_str(json).expect("legacy settings must load");
        assert_eq!(s.alert_system_cpu, 90);
        assert_eq!(s.alert_system_mem, 90);
        assert_eq!(s.alert_process_cpu, 90);
        assert_eq!(s.alert_process_mem, 90);
    }

    #[test]
    fn notify_fields_default_on_legacy_json() {
        let json = r#"{
            "theme":"auto","language":"zh-CN","sidebar_collapsed":false,
            "software_root":"apps","config_root":"config","mirror_url":"",
            "auto_check_update":true,"close_window_action":"CloseToTray","ask_on_close":true,
            "jre_default_id":null,"github_proxy_url":"","proxy_url":""
        }"#;
        let s: AppSettings = serde_json::from_str(json).expect("legacy settings must load");
        assert_eq!(s.alert_webhook_url, "");
        assert_eq!(s.alert_webhook_format, "json");
        assert_eq!(s.alert_webhook_secret, "");
        assert_eq!(s.smtp_host, "");
        assert_eq!(s.smtp_port, 465);
        assert_eq!(s.smtp_user, "");
        assert_eq!(s.smtp_pass, "");
        assert_eq!(s.smtp_to, "");
        assert!(!s.smtp_enabled);
    }
}
