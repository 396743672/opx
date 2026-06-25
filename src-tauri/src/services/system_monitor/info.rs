use sysinfo::{System, SystemExt, ProcessorExt, DiskExt, NetworkExt};
use crate::models::system::{SystemInfo, DiskInfo, NetworkInfo, ProcessInfo};

pub fn get_system_info(system: &mut System) -> SystemInfo {
    system.refresh_all();

    let cpu_usage = system.global_processor_info().cpu_usage();

    let memory_used = system.used_memory();
    let memory_total = system.total_memory();
    let memory_usage = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64) * 100.0
    } else {
        0.0
    };

    let mut disks = Vec::new();
    for disk in system.disks() {
        let total = disk.total_size();
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

    let mut bytes_sent = 0;
    let mut bytes_recv = 0;
    let mut packets_sent = 0;
    let mut packets_recv = 0;
    for (_, network) in system.networks() {
        bytes_sent += network.transmitted();
        bytes_recv += network.received();
        packets_sent += network.transmitted_packets();
        packets_recv += network.received_packets();
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
    system.refresh_processes();

    let mut processes = Vec::new();
    for (pid, process) in system.processes() {
        processes.push(ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string(),
            cpu_usage: process.cpu_usage(),
            memory_usage: process.memory_usage(),
            status: process.status().to_string(),
        });
    }

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes
}

pub fn kill_process(pid: u32) -> bool {
    use sysinfo::ProcessExt;
    let mut system = System::new();
    system.refresh_processes();
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        process.kill()
    } else {
        false
    }
}