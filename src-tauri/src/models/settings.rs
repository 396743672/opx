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
}

fn default_dns_provider() -> String {
    "cloudflare".to_string()
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
}