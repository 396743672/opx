use once_cell::sync::Lazy;
use std::sync::Mutex;
use sysinfo::{System, Disks, Networks};
use crate::models::system::{SystemInfo, DiskInfo, NetworkInfo};

/// 全局复用的 Networks 句柄，避免每次新建导致统计重置
static NETWORKS: Lazy<Mutex<Networks>> =
    Lazy::new(|| Mutex::new(Networks::new_with_refreshed_list()));

pub fn get_system_info(system: &mut System) -> SystemInfo {
    system.refresh_all();
    // 单独刷新网络（refresh_all 不含 Networks）
    if let Ok(mut nets) = NETWORKS.lock() {
        nets.refresh();
    }

    let cpu_usage = system.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / system.cpus().len() as f32;
    let cpu_usage = cpu_usage as f64;

    let memory_used = system.used_memory();
    let memory_total = system.total_memory();
    let memory_usage = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64) * 100.0
    } else {
        0.0
    };

    let disks_data = Disks::new_with_refreshed_list();
    let mut disks = Vec::new();
    for disk in disks_data.list() {
        let total = disk.total_space();
        let available = disk.available_space();
        let used = total - available;
        let usage = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        disks.push(DiskInfo {
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total,
            used,
            usage,
        });
    }

    let mut bytes_sent = 0u64;
    let mut bytes_recv = 0u64;
    if let Ok(nets) = NETWORKS.lock() {
        for (_, network) in nets.iter() {
            bytes_sent += network.total_transmitted();
            bytes_recv += network.total_received();
        }
    }

    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let boot_time = System::boot_time();

    SystemInfo {
        cpu_usage,
        memory_used,
        memory_total,
        memory_usage,
        disks,
        network: NetworkInfo {
            bytes_sent,
            bytes_recv,
            packets_sent: 0,
            packets_recv: 0,
        },
        os_name,
        os_version,
        hostname,
        boot_time,
    }
}
