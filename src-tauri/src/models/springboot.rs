use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Stopping,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringBootApp {
    pub id: String,
    pub name: String,
    pub jar_path: String,
    pub version: String,
    pub jdk_installed_id: String,
    /// 自动生成的优化 JVM 参数 + 用户手动修改后的合并结果
    pub jvm_opts: Vec<String>,
    pub program_args: Vec<String>,
    pub profile: String,
    pub env_vars: Vec<(String, String)>,
    pub status: AppStatus,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub log_path: String,
    pub start_time: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    /// 依赖的已安装软件 ID 列表（如 mysql/redis 的 installed_id）
    pub dependencies: Vec<String>,
    pub auto_start: bool,
    pub startup_order: u32,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub health_check_timeout_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmInfo {
    pub heap_used: u64,
    pub heap_max: u64,
    pub non_heap_used: u64,
    pub thread_count: usize,
    pub gc_count: u64,
    pub gc_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringBootStore {
    pub applications: Vec<SpringBootApp>,
    pub groups: Vec<AppGroup>,
}

impl Default for SpringBootStore {
    fn default() -> Self {
        Self {
            applications: Vec::new(),
            groups: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceResult {
    pub backup_path: String,
    pub old_version: String,
    pub new_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAppParams {
    pub jar_path: String,
    pub name: String,
    pub jdk_installed_id: String,
    pub jvm_opts: Vec<String>,
    pub program_args: Vec<String>,
    pub profile: String,
    pub env_vars: Vec<(String, String)>,
    pub port: Option<u16>,
    pub log_path: String,
    pub dependencies: Vec<String>,
    pub auto_start: bool,
    pub startup_order: u32,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub health_check_timeout_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAppParams {
    pub name: Option<String>,
    pub jdk_installed_id: Option<String>,
    pub jvm_opts: Option<Vec<String>>,
    pub program_args: Option<Vec<String>>,
    pub profile: Option<String>,
    pub env_vars: Option<Vec<(String, String)>>,
    pub port: Option<u16>,
    pub log_path: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub auto_start: Option<bool>,
    pub startup_order: Option<u32>,
    pub auto_restart: Option<bool>,
    pub group: Option<Option<String>>,
    pub health_check_timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmOptsTemplate {
    pub xms_mb: u64,
    pub xmx_mb: u64,
    pub metaspace_mb: u64,
    pub gc_type: String,
    pub extra_flags: Vec<String>,
}
