use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_usage: f64,
    pub disks: Vec<DiskInfo>,
    pub network: NetworkInfo,
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub boot_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub packets_sent: u64,
    pub packets_recv: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub timestamp: u64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}
