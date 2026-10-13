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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub timestamp: u64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

/// 持久化指标历史（30s 粒度，保留 7 天）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsHistory {
    #[serde(default)]
    pub system: Vec<HistoryPoint>,
    /// pid（字符串）-> 该进程的样本
    #[serde(default)]
    pub processes: std::collections::HashMap<String, Vec<HistoryPoint>>,
}
