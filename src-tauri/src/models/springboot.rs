use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringApp {
    pub id: String,
    pub name: String,
    pub jar_path: String,
    pub version: String,
    pub env: String,
    pub port: u16,
    pub jvm_opts: String,
    pub args: String,
    pub status: AppStatus,
    pub log_path: String,
    pub start_time: Option<NaiveDateTime>,
    pub backup_enabled: bool,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Stopping,
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
pub struct SpringAppList {
    pub applications: Vec<SpringApp>,
    pub groups: Vec<AppGroup>,
}

impl Default for SpringAppList {
    fn default() -> Self {
        Self {
            applications: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl Default for AppStatus {
    fn default() -> Self {
        AppStatus::Stopped
    }
}