use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub available_versions: Vec<String>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub id: String,
    pub key: String,
    pub name: String,
    pub version: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallParams {
    pub key: String,
    pub version: String,
    pub install_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftwareList {
    pub software: Vec<InstalledSoftware>,
}

impl Default for InstalledSoftwareList {
    fn default() -> Self {
        Self { software: Vec::new() }
    }
}