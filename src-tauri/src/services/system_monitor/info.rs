use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System, Disks, Networks};
use opx_core::models::system::{SystemInfo, DiskInfo, NetworkInfo};

/// 全局复用的 Networks 句柄，避免每次新建导致统计重置
static NETWORKS: Lazy<Mutex<Networks>> =
    Lazy::new(|| Mutex::new(Networks::new_with_refreshed_list()));

/// 构造只含 CPU 用量 + 内存的 System，并按官方要求做间隔 ≥200ms 的双次刷新
/// （sysinfo 的 CPU% 是两次测量的差值，首采无效；见 DeepWiki sysinfo §2.4）。
fn new_cpu_mem_system() -> System {
    let mut s = System::new_with_specifics(
        // 0.31 版 API 是 `new()`（master 才改名 `nothing()`，勿照抄新版文档）
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    thread::sleep(Duration::from_millis(200));
    s.refresh_cpu_usage();
    s.refresh_memory();
    s
}

/// 全局 System 实例（**实时轮询路径**，~1s 窗口）。刻意不用 `refresh_all`：
/// 它附带全量进程扫描（本机 276 进程、数百 ms），既拖慢 1s 轮询的锁持有，
/// 又会把增量窗口切得参差。CPU% 取官方 `global_cpu_usage()`（= 各核平均，
/// 已用探针实测与手写平均逐值相等）。
static SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(new_cpu_mem_system()));

/// 低频（30s 落盘）**专用**实例：独立后 recorder 的增量窗口是真实的 30s，
/// 历史曲线才是「30s 平均」语义。若与实时路径共用实例，1s 轮询会把
/// recorder 的增量窗口切割成 ~1s（窗口语义失真，与任务管理器的对照点
/// 也不再稳定）。同款模式见 `process_monitor::PROCESS_SYS_SLOW`。
static SYSTEM_SLOW: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(new_cpu_mem_system()));

/// 实时采样（~1s 窗口，`system_info` 命令 / 前端 1s 轮询用）。
pub fn sample_system() -> SystemInfo {
    let mut system = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    get_system_info(&mut system)
}

/// 低频采样（真实 30s 窗口，recorder 落盘 / 告警判定用）。
pub fn sample_system_slow() -> SystemInfo {
    let mut system = SYSTEM_SLOW.lock().unwrap_or_else(|e| e.into_inner());
    get_system_info(&mut system)
}

pub fn get_system_info(system: &mut System) -> SystemInfo {
    system.refresh_cpu_usage();
    system.refresh_memory();
    // 单独刷新网络（refresh_all 不含 Networks）
    if let Ok(mut nets) = NETWORKS.lock() {
        nets.refresh();
    }

    let cpu_usage = system.global_cpu_usage() as f64;

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
        },
        os_name,
        os_version,
        hostname,
        boot_time,
    }
}
