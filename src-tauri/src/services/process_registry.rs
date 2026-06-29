use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System, ProcessesToUpdate};
use tauri::{AppHandle, Emitter};

/// 已注册的子进程
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisteredProcess {
    pub id: i64,
    pub pid: u32,
    pub name: String,
    pub kind: String,
    pub started_at: i64,
}

/// 单个进程停止结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopResult {
    pub pid: u32,
    pub name: String,
    pub success: bool,
}

/// 进度事件 payload
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopProgressEvent {
    pub current: usize,
    pub total: usize,
    pub name: String,
    pub status: String, // "stopped" | "killed" | "failed"
}

pub struct ProcessRegistry {
    next_id: i64,
    processes: HashMap<i64, RegisteredProcess>,
}

impl ProcessRegistry {
    fn new() -> Self {
        Self {
            next_id: 1,
            processes: HashMap::new(),
        }
    }

    pub fn register(&mut self, pid: u32, name: String, kind: String) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let started_at = chrono::Local::now().timestamp();
        self.processes.insert(
            id,
            RegisteredProcess {
                id,
                pid,
                name,
                kind,
                started_at,
            },
        );
        id
    }

    pub fn unregister(&mut self, id: i64) {
        self.processes.remove(&id);
    }

    pub fn list(&self) -> Vec<RegisteredProcess> {
        self.processes.values().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<RegisteredProcess> {
        let v: Vec<RegisteredProcess> = self.processes.values().cloned().collect();
        self.processes.clear();
        v
    }
}

static REGISTRY: Lazy<Mutex<ProcessRegistry>> =
    Lazy::new(|| Mutex::new(ProcessRegistry::new()));

/// 注册一个子进程，返回 id
pub fn register(pid: u32, name: String, kind: String) -> i64 {
    REGISTRY.lock().unwrap().register(pid, name, kind)
}

/// 注销
pub fn unregister(id: i64) {
    REGISTRY.lock().unwrap().unregister(id);
}

/// 列出所有已注册进程
pub fn list() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().list()
}

/// 进程是否存活
fn is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All);
    sys.process(Pid::from_u32(pid)).is_some()
}

/// 停止单个进程：优雅停止→等5s→强杀。返回 (是否成功, 状态字符串)
fn stop_one(pid: u32) -> (bool, String) {
    if !is_alive(pid) {
        return (true, "stopped".to_string());
    }
    // 优雅停止
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .output();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();

    // 轮询等待最多 5s
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if !is_alive(pid) {
            return (true, "stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // 超时强杀
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output();

    std::thread::sleep(Duration::from_millis(300));
    if is_alive(pid) {
        (false, "failed".to_string())
    } else {
        (true, "killed".to_string())
    }
}

/// 停止所有已注册子进程，逐个 emit 进度，完成后 emit stop-complete
pub fn stop_all(app: &AppHandle) -> Vec<StopResult> {
    let procs = REGISTRY.lock().unwrap().drain();
    let total = procs.len();
    let mut results = Vec::new();

    for (i, p) in procs.iter().enumerate() {
        let (success, status) = stop_one(p.pid);
        let _ = app.emit(
            "stop-progress",
            StopProgressEvent {
                current: i + 1,
                total,
                name: p.name.clone(),
                status: status.clone(),
            },
        );
        results.push(StopResult {
            pid: p.pid,
            name: p.name.clone(),
            success,
        });
    }

    let _ = app.emit("stop-complete", ());
    results
}
