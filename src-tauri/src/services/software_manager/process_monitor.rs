use once_cell::sync::Lazy;
use std::sync::Mutex;
use sysinfo::{Pid, ProcessesToUpdate, System};

/// 单进程采样结果：OS 级 CPU%（0-100，f64）与物理内存字节数
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProcessSample {
    pub pid: u32,
    pub cpu_usage: f64,
    pub mem_bytes: u64,
}

/// 跨调用缓存的 System：进程 CPU% 依赖两次 refresh 的时间差，
/// 保持同一实例才能算出准确的增量 CPU%。
static PROCESS_SYS: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new();
    s.refresh_processes(ProcessesToUpdate::All);
    Mutex::new(s)
});

/// 对给定 pid 列表逐进程采样。进程已退出/不存在则跳过（不返回该 pid）。
pub fn sample_processes(pids: &[u32]) -> Vec<ProcessSample> {
    let mut system = PROCESS_SYS.lock().unwrap();
    system.refresh_processes(ProcessesToUpdate::All);
    pids.iter()
        .filter_map(|&pid| {
            let p = system.process(Pid::from_u32(pid))?;
            Some(ProcessSample {
                pid,
                cpu_usage: p.cpu_usage() as f64,
                mem_bytes: p.memory(),
            })
        })
        .collect()
}

/// 查询进程名（端口冲突占用者提示用）。进程不存在返回 None。
pub fn process_name(pid: u32) -> Option<String> {
    let mut system = PROCESS_SYS.lock().unwrap();
    system.refresh_processes(ProcessesToUpdate::All);
    system
        .process(Pid::from_u32(pid))
        .map(|p| p.name().to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_missing_pids_returns_empty() {
        // 不存在的 pid 直接返回空集，不崩溃
        let out = sample_processes(&[999_999_99u32]);
        assert!(out.is_empty());
    }

    #[test]
    fn sample_current_process_returns_non_negative() {
        // 采样当前进程自身：能取到数据且不为负
        let my_pid = std::process::id();
        let out = sample_processes(&[my_pid]);
        assert!(!out.is_empty());
        let me = &out[0];
        assert_eq!(me.pid, my_pid);
        assert!(me.cpu_usage >= 0.0);
        assert!(me.mem_bytes > 0);
    }
}
