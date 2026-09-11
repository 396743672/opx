use serde::{Deserialize, Serialize};

/// Node 应用状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeAppStatus {
    Stopped,
    Running,
    Starting,
    Stopping,
    Error,
}

impl Default for NodeAppStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

/// Node 应用（用已装 Node.js 运行时拉起的 JS/TS 服务）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeApp {
    pub id: String,
    pub name: String,
    /// JS 入口文件绝对路径（如 /path/app.js）
    pub entry_path: String,
    /// 使用的 Node 实例 installed_id（空 = 自动选择已装 Node）
    #[serde(default)]
    pub node_installed_id: String,
    /// 额外启动参数
    #[serde(default)]
    pub args: Vec<String>,
    /// 额外环境变量
    #[serde(default)]
    pub env_vars: Vec<(String, String)>,
    /// 应用启动时自动拉起
    #[serde(default)]
    pub auto_start: bool,
    /// 自启顺序（数字越小越早）
    #[serde(default)]
    pub startup_order: u32,

    /// 进程意外退出后自动重启
    #[serde(default)]
    pub auto_restart: bool,

    // ===== 运行时字段 =====
    #[serde(default)]
    pub status: NodeAppStatus,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub log_path: String,
}

/// Node 应用创建载荷
#[derive(Debug, Clone, Deserialize)]
pub struct CreateNodeAppParams {
    pub name: String,
    pub entry_path: String,
    /// 指定 Node 实例 installed_id（空 = 自动）
    #[serde(default)]
    pub node_installed_id: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env_vars: Vec<(String, String)>,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub startup_order: u32,
    #[serde(default)]
    pub auto_restart: bool,
}

/// Node 应用更新载荷（全可选）
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateNodeAppParams {
    pub name: Option<String>,
    pub entry_path: Option<String>,
    #[serde(default)]
    pub node_installed_id: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env_vars: Option<Vec<(String, String)>>,
    #[serde(default)]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub startup_order: Option<u32>,
    #[serde(default)]
    pub auto_restart: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_app_params_default_auto_restart_false() {
        let p: CreateNodeAppParams =
            serde_json::from_str(r#"{"name":"a","entry_path":"b"}"#).expect("params parse");
        assert!(!p.auto_restart);
        let u: UpdateNodeAppParams = serde_json::from_str(r#"{}"#).expect("update parse");
        assert!(u.auto_restart.is_none());
    }
}