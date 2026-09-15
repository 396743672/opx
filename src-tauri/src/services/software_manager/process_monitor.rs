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

/// 30s 落盘采样专用实例。CPU% 是「距上次 refresh 的增量 / 时间差」——
/// 与 1s 实时采样共享同一实例时，落盘拿到的是那一瞬的瞬时值（多数时刻恰好空闲 → 全 0），
/// 独立实例才能得到真正的 30s 平均。
static PROCESS_SYS_SLOW: Lazy<Mutex<System>> = Lazy::new(|| {
    let mut s = System::new();
    s.refresh_processes(ProcessesToUpdate::All);
    Mutex::new(s)
});

/// 从当前 System 快照出 (pid, parent) 列表，供纯函数做进程树判断。
fn snapshot_tree(system: &System) -> Vec<(u32, Option<u32>)> {
    system
        .processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32(), p.parent().map(|pp| pp.as_u32())))
        .collect()
}

/// pid 是否为 root 的后代（**不含自身**）。沿 parent 链上溯，深度上限防异常环。
fn is_descendant(tree: &[(u32, Option<u32>)], pid: u32, root: u32) -> bool {
    let parent_of = |p: u32| tree.iter().find(|(q, _)| *q == p).and_then(|(_, par)| *par);
    let mut cur = parent_of(pid);
    for _ in 0..32 {
        match cur {
            Some(p) if p == root => return true,
            Some(p) => cur = parent_of(p),
            None => return false,
        }
    }
    false
}

/// 对给定 pid 列表逐进程采样：**累加自身与全部后代进程**的 CPU 与内存。
///
/// Windows 上 mysqld 等会 fork 出真正干活的子进程（被 spawn 的父进程是空壳，
/// CPU 恒 0、内存恒定），只看自身会得到一条死直线；Nginx 主+worker 同理。
/// 返回的 `pid` 仍是传入值，调用方按原 pid 索引即可。
pub fn sample_processes(pids: &[u32]) -> Vec<ProcessSample> {
    sample_with(&mut PROCESS_SYS.lock().unwrap(), pids)
}

/// 低频（30s 落盘）专用入口：走独立的 System 实例，得到窗口期平均 CPU。
pub fn sample_processes_slow(pids: &[u32]) -> Vec<ProcessSample> {
    sample_with(&mut PROCESS_SYS_SLOW.lock().unwrap(), pids)
}

fn sample_with(system: &mut System, pids: &[u32]) -> Vec<ProcessSample> {
    system.refresh_processes(ProcessesToUpdate::All);
    let tree = snapshot_tree(system);
    pids.iter()
        .filter_map(|&pid| {
            let root = system.process(Pid::from_u32(pid))?;
            let mut cpu = root.cpu_usage() as f64;
            let mut mem = root.memory();
            for (child_pid, _) in &tree {
                if *child_pid == pid || !is_descendant(&tree, *child_pid, pid) {
                    continue;
                }
                if let Some(p) = system.process(Pid::from_u32(*child_pid)) {
                    cpu += p.cpu_usage() as f64;
                    mem += p.memory();
                }
            }
            Some(ProcessSample {
                pid,
                cpu_usage: cpu,
                mem_bytes: mem,
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

    #[test]
    fn descendant_detection_follows_parent_chain() {
        // 1 -> 2 -> 3；4 独立
        let tree = [(1u32, None), (2, Some(1)), (3, Some(2)), (4, None)];
        assert!(is_descendant(&tree, 2, 1), "直接子进程是后代");
        assert!(is_descendant(&tree, 3, 1), "孙进程也是后代");
        assert!(!is_descendant(&tree, 1, 1), "自身不算后代");
        assert!(!is_descendant(&tree, 4, 1), "无关进程不算后代");
        assert!(!is_descendant(&tree, 1, 2), "反向（祖先不是后代）");
    }

    #[test]
    fn descendant_detection_survives_parent_cycle() {
        // 异常环 5 <-> 6：深度上限兜底，不死循环
        let tree = [(5u32, Some(6)), (6, Some(5))];
        assert!(!is_descendant(&tree, 5, 99));
    }

    #[test]
    fn descendant_detection_handles_missing_pid() {
        let tree = [(1u32, None)];
        assert!(!is_descendant(&tree, 777, 1), "树里没有的 pid 不算后代");
    }

    /// 真实进程树：spawn 一个子进程，验证其内存被累加进父进程采样。
    /// 这是「MySQL 父壳 + 子进程」场景的最小复现。
    #[test]
    fn sample_aggregates_child_process_memory() {
        let me = std::process::id();
        let only_self = sample_processes(&[me]);
        assert!(!only_self.is_empty());
        let self_mem = only_self[0].mem_bytes;

        // 子进程持有 ~20MB，父进程自身测量值应随之增大
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "ping -n 20 127.0.0.1 > nul"])
            .spawn()
            .expect("spawn child");
        std::thread::sleep(std::time::Duration::from_millis(800));
        let with_child = sample_processes(&[me]);
        let _ = child.kill();
        let _ = child.wait();

        assert!(!with_child.is_empty());
        assert!(
            with_child[0].mem_bytes > self_mem,
            "含子进程的采样应大于仅自身（{} vs {}）",
            with_child[0].mem_bytes,
            self_mem
        );
    }
}
