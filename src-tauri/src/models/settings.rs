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
    pub register_as_system_service: bool,
    pub auto_start_managed_services: bool,
    pub jre_default_id: Option<String>,
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
            register_as_system_service: false,
            auto_start_managed_services: true,
            jre_default_id: None,
        }
    }
}