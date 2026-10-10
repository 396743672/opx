//! CPU 采样探针：复现 info.rs 的调用模式，对照任务管理器验证 sysinfo 读数。
//! 运行：cargo run --example cpu_probe -p opx
//! 期间请保持机器负载状态与任务管理器读数对照。

use std::thread;
use std::time::Duration;
use sysinfo::System;

fn main() {
    let mut s = System::new();
    s.refresh_all();
    thread::sleep(Duration::from_millis(200)); // MINIMUM_CPU_UPDATE_INTERVAL
    s.refresh_all();

    println!("逻辑核数: {}", s.cpus().len());
    println!("--- 与任务管理器对照 10 轮（每轮 1s，同 info.rs 模式：refresh_all + 六核平均）---");
    for i in 0..10 {
        thread::sleep(Duration::from_secs(1));
        s.refresh_all();
        let n = s.cpus().len() as f32;
        let avg = s.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / n;
        let per_core: Vec<String> = s
            .cpus()
            .iter()
            .map(|c| format!("{:.0}", c.cpu_usage()))
            .collect();
        println!(
            "[{}] 手写平均={:5.1}  global_cpu_usage={:5.1}  每核=[{}]",
            i,
            avg,
            s.global_cpu_usage(),
            per_core.join(", ")
        );
    }

    println!("--- 对照组：仅 refresh_cpu_usage（官方推荐姿势）再跑 5 轮 ---");
    for i in 0..5 {
        thread::sleep(Duration::from_secs(1));
        s.refresh_cpu_usage();
        let n = s.cpus().len() as f32;
        let avg = s.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / n;
        println!(
            "[{}] 手写平均={:5.1}  global_cpu_usage={:5.1}",
            i,
            avg,
            s.global_cpu_usage()
        );
    }
}
