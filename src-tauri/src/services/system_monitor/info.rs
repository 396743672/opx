use sysinfo::{System, Disks, Networks, ProcessesToUpdate, Pid};
use crate::models::system::{SystemInfo, DiskInfo, NetworkInfo, ProcessInfo};

pub fn get_system_info(system: &mut System) -> SystemInfo {
    system.refresh_all();

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

    let networks_data = Networks::new_with_refreshed_list();
    let mut bytes_sent = 0;
    let mut bytes_recv = 0;
    // packets not available in sysinfo 0.31, set to 0
    let packets_sent = 0;
    let packets_recv = 0;
    for (_, network) in networks_data.iter() {
        bytes_sent += network.total_transmitted();
        bytes_recv += network.total_received();
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
            packets_sent,
            packets_recv,
        },
        os_name,
        os_version,
        hostname,
        boot_time,
    }
}

pub fn get_process_list(system: &mut System) -> Vec<ProcessInfo> {
    system.refresh_processes(ProcessesToUpdate::All);

    let mut processes = Vec::new();
    for (pid, process) in system.processes() {
        processes.push(ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string_lossy().to_string(),
            cpu_usage: process.cpu_usage() as f64,
            memory_usage: (process.memory() as f64) / (1024 * 1024) as f64,
            status: process.status().to_string(),
        });
    }

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes
}

pub fn kill_process(pid: u32) -> bool {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All);
    if let Some(process) = system.process(Pid::from_u32(pid)) {
        process.kill()
    } else {
        false
    }
}
