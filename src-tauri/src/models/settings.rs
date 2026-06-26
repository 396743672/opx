use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CloseWindowAction {
    MinimizeToTray,
    Exit,
    BackgroundService,
}

impl Default for CloseWindowAction {
    fn default() -> Self {
        Self::MinimizeToTray
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
    pub register_as_system_service: bool,
    pub auto_start_managed_services: bool,
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
            register_as_system_service: false,
            auto_start_managed_services: true,
        }
    }
}
