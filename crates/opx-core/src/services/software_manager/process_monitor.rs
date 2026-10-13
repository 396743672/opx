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

/// 从当前 System 快照出 pid → parent 索引，供纯函数做进程树判断。
/// HashMap：is_descendant 每跳查一次父进程，1Hz 采样下避免线性扫全表。
fn snapshot_tree(system: &System) -> std::collections::HashMap<u32, Option<u32>> {
    system
        .processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32(), p.parent().map(|pp| pp.as_u32())))
        .collect()
}

/// pid 是否为 root 的后代（**不含自身**）。沿 parent 链上溯，深度上限防异常环。
fn is_descendant(tree: &std::collections::HashMap<u32, Option<u32>>, pid: u32, root: u32) -> bool {
    let mut cur = tree.get(&pid).copied().flatten();
    for _ in 0..32 {
        match cur {
            Some(p) if p == root => return true,
            Some(p) => cur = tree.get(&p).copied().flatten(),
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
    sample_with(&mut PROCESS_SYS.lock().unwrap_or_else(|e| e.into_inner()), pids)
}

/// 低频（30s 落盘）专用入口：走独立的 System 实例，得到窗口期平均 CPU。
pub fn sample_processes_slow(pids: &[u32]) -> Vec<ProcessSample> {
    sample_with(&mut PROCESS_SYS_SLOW.lock().unwrap_or_else(|e| e.into_inner()), pids)
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
    let mut system = PROCESS_SYS.lock().unwrap_or_else(|e| e.into_inner());
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

    fn tree(pairs: &[(u32, Option<u32>)]) -> std::collections::HashMap<u32, Option<u32>> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn descendant_detection_follows_parent_chain() {
        // 1 -> 2 -> 3；4 独立
        let tree = tree(&[(1u32, None), (2, Some(1)), (3, Some(2)), (4, None)]);
        assert!(is_descendant(&tree, 2, 1), "直接子进程是后代");
        assert!(is_descendant(&tree, 3, 1), "孙进程也是后代");
        assert!(!is_descendant(&tree, 1, 1), "自身不算后代");
        assert!(!is_descendant(&tree, 4, 1), "无关进程不算后代");
        assert!(!is_descendant(&tree, 1, 2), "反向（祖先不是后代）");
    }

    #[test]
    fn descendant_detection_survives_parent_cycle() {
        // 异常环 5 <-> 6：深度上限兜底，不死循环
        let tree = tree(&[(5u32, Some(6)), (6, Some(5))]);
        assert!(!is_descendant(&tree, 5, 99));
    }

    #[test]
    fn descendant_detection_handles_missing_pid() {
        let tree = tree(&[(1u32, None)]);
        assert!(!is_descendant(&tree, 777, 1), "树里没有的 pid 不算后代");
    }

    /// 真实进程树：spawn 一个**持有大常驻内存**的子进程，验证其内存被累加进父进程采样。
    /// 这是「MySQL 父壳 + 子进程」场景的最小复现。
    ///
    /// 为什么不用「父进程前后两次采样对比」：测试进程自身的 WorkingSet 会被系统回收，
    /// 且并行执行的其它用例分配/释放内存，波动可达 ±20MB——足以淹没 cmd/ping 这类几 MB
    /// 子进程，使断言偶发失败（历史 flake）。改用确定性不变式：子进程持有 ~256MB，
    /// 父聚合内存 = 父自身 + 全部后代，**必然 ≥ 子进程自身内存**；若聚合失效（后代未计入），
    /// 父聚合仅父自身（~80MB）< 256MB → 断言失败。该判据对父进程波动完全免疫。
    #[test]
    fn sample_aggregates_child_process_memory() {
        // 与其它「真实 spawn 子进程」的测试串行（见 PROCESS_SPAWN_TEST_LOCK 注释）。
        let _spawn_guard = crate::services::software_manager::PROCESS_SPAWN_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let me = std::process::id();

        // 子进程：分配 ~256MB 并**实际写入**（零填充的数组映射到共享零页，不写入不占工作集），
        // 用加密 RNG 一次性填满整个数组以触发全部缺页装入，随后保持数秒。
        #[cfg(windows)]
        let mut child = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$x = [byte[]]::new(268435456); [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($x); Start-Sleep -Seconds 12",
            ])
            .spawn()
            .expect("spawn memory-holding child");
        #[cfg(not(windows))]
        let mut child = std::process::Command::new("sh")
            .args(["-c", "sleep 12"]) // 非 Windows：仅保证编译通过（该用例面向 Windows 进程树）
            .spawn()
            .expect("spawn memory-holding child");

        // 轮询等待子进程内存涨到 >100MB（powershell 启动 + 分配/写入需要时间）。
        let mut child_mem: u64 = 0;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(120));
            child_mem = sample_processes(&[child.id()])
                .first()
                .map(|s| s.mem_bytes)
                .unwrap_or(0);
            if child_mem > 100 * 1024 * 1024 {
                break;
            }
        }

        // 子进程存活期间采样父进程：聚合值应包含该后代。
        let parent_aggregate = sample_processes(&[me])
            .first()
            .map(|s| s.mem_bytes)
            .unwrap_or(0);

        let _ = child.kill();
        let _ = child.wait();

        #[cfg(windows)]
        assert!(
            child_mem > 100 * 1024 * 1024,
            "子进程应持有 >100MB 常驻内存（实测 {}）",
            child_mem
        );
        #[cfg(windows)]
        assert!(
            parent_aggregate >= child_mem,
            "父聚合内存应包含后代子进程（父聚合 {} vs 子自身 {}）",
            parent_aggregate,
            child_mem
        );
        // 非 Windows：该用例面向 Windows 进程树，仅消费变量避免 unused 告警。
        #[cfg(not(windows))]
        let _ = (parent_aggregate, child_mem);
    }
}
