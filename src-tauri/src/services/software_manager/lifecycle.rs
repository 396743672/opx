use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Local;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisteredProcess {
    pub installed_id: String,
    pub pid: u32,
    pub name: String,
    pub key: String,
    pub kind: String,
    pub started_at: i64,
}

pub struct ProcessRegistry {
    processes: HashMap<String, RegisteredProcess>, // key = installed_id
}

impl ProcessRegistry {
    fn new() -> Self {
        Self { processes: HashMap::new() }
    }

    pub fn register(
        &mut self,
        installed_id: String,
        pid: u32,
        name: String,
        key: String,
        kind: String,
    ) {
        let entry = RegisteredProcess {
            installed_id: installed_id.clone(),
            pid,
            name,
            key,
            kind,
            started_at: Local::now().timestamp(),
        };
        self.processes.insert(installed_id, entry);
    }

    pub fn unregister(&mut self, installed_id: &str) {
        self.processes.remove(installed_id);
    }

    pub fn get(&self, installed_id: &str) -> Option<&RegisteredProcess> {
        self.processes.get(installed_id)
    }

    pub fn list(&self) -> Vec<RegisteredProcess> {
        self.processes.values().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<RegisteredProcess> {
        let v: Vec<_> = self.processes.values().cloned().collect();
        self.processes.clear();
        v
    }
}

static REGISTRY: Lazy<Mutex<ProcessRegistry>> =
    Lazy::new(|| Mutex::new(ProcessRegistry::new()));

pub fn register(installed_id: String, pid: u32, name: String, key: String, kind: String) {
    REGISTRY.lock().unwrap().register(installed_id, pid, name, key, kind);
}

pub fn unregister(installed_id: &str) {
    REGISTRY.lock().unwrap().unregister(installed_id);
}

pub fn get(installed_id: &str) -> Option<RegisteredProcess> {
    REGISTRY.lock().unwrap().get(installed_id).cloned()
}

pub fn list() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().list()
}

pub fn drain() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().drain()
}

// —— spawn 执行器 ——

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::services::software_manager::providers::{FirstRunInit, StartCommand};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 用 StartCommand 构造并 spawn 子进程
/// Windows 上设置 CREATE_NO_WINDOW flag 隐藏控制台窗口
pub fn spawn_process(cmd: StartCommand) -> anyhow::Result<Child> {
    let mut command = Command::new(&cmd.program);
    command.args(&cmd.args).current_dir(&cmd.working_dir);

    for (k, v) in &cmd.env_vars {
        command.env(k, v);
    }

    #[cfg(windows)]
    command.creation_flags(cmd.creation_flags);

    let child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(child)
}

/// 执行首次初始化命令（同步等待，最多 60s）
/// 用于 MySQL --initialize-insecure 等场景
pub fn run_first_run_init(fri: &FirstRunInit) -> anyhow::Result<()> {
    let init = &fri.init_command;
    let mut command = Command::new(&init.program);
    command.args(&init.args).current_dir(&init.working_dir);

    for (k, v) in &init.env_vars {
        command.env(k, v);
    }

    #[cfg(windows)]
    command.creation_flags(init.creation_flags);

    // 用 output() 等待完成并捕获 stdout/stderr（用于抓临时密码）
    let output = command.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "初始化命令失败（code={}）：{}",
            output.status,
            stderr
        ));
    }

    // 若有 temp_secret_output，可在此处抓取（任务 6.3 实现）
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::software_manager::providers::{FirstRunInit, StartCommand};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn register_and_get_returns_entry() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-1".to_string(),
            12345,
            "MySQL 8.4.10".to_string(),
            "mysql".to_string(),
            "Database".to_string(),
        );
        let entry = reg.get("uuid-1").unwrap();
        assert_eq!(entry.pid, 12345);
        assert_eq!(entry.key, "mysql");
    }

    #[test]
    fn unregister_removes_entry() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-2".to_string(), 111, "Test".to_string(),
            "redis".to_string(), "Cache".to_string(),
        );
        assert!(reg.get("uuid-2").is_some());
        reg.unregister("uuid-2");
        assert!(reg.get("uuid-2").is_none());
    }

    #[test]
    fn register_overwrites_same_id() {
        let mut reg = ProcessRegistry::new();
        reg.register(
            "uuid-3".to_string(), 100, "Old".to_string(),
            "mysql".to_string(), "Database".to_string(),
        );
        reg.register(
            "uuid-3".to_string(), 200, "New".to_string(),
            "mysql".to_string(), "Database".to_string(),
        );
        let entry = reg.get("uuid-3").unwrap();
        assert_eq!(entry.pid, 200);
        assert_eq!(entry.name, "New");
    }

    #[test]
    fn list_returns_all_entries() {
        let mut reg = ProcessRegistry::new();
        reg.register("a".to_string(), 1, "A".to_string(), "k".to_string(), "K".to_string());
        reg.register("b".to_string(), 2, "B".to_string(), "k".to_string(), "K".to_string());
        let list = reg.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn drain_clears_and_returns_all() {
        let mut reg = ProcessRegistry::new();
        reg.register("a".to_string(), 1, "A".to_string(), "k".to_string(), "K".to_string());
        reg.register("b".to_string(), 2, "B".to_string(), "k".to_string(), "K".to_string());
        let drained = reg.drain();
        assert_eq!(drained.len(), 2);
        assert!(reg.list().is_empty());
    }

    #[test]
    fn get_returns_none_for_unknown_id() {
        let reg = ProcessRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn spawn_command_returns_pid_on_success() {
        // Windows: cmd.exe /c exit 0；Unix: /bin/true
        let program = if cfg!(windows) { "cmd.exe" } else { "/bin/true" };
        let args: Vec<String> = if cfg!(windows) {
            vec!["/c".to_string(), "exit".to_string(), "0".to_string()]
        } else {
            vec![]
        };
        let cmd = StartCommand {
            program: program.to_string(),
            args,
            env_vars: BTreeMap::new(),
            working_dir: PathBuf::from("."),
            creation_flags: 0x08000000,
            first_run_init: None,
        };
        let result = spawn_process(cmd);
        assert!(result.is_ok());
        let child = result.unwrap();
        assert!(child.id() > 0);
    }

    #[test]
    fn spawn_command_fails_for_nonexistent_program() {
        let cmd = StartCommand {
            program: "nonexistent-program-xyz-99999.exe".to_string(),
            args: vec![],
            env_vars: BTreeMap::new(),
            working_dir: PathBuf::from("."),
            creation_flags: 0x08000000,
            first_run_init: None,
        };
        let result = spawn_process(cmd);
        assert!(result.is_err());
    }

    #[test]
    fn run_first_run_init_executes_init_command() {
        let program = if cfg!(windows) { "cmd.exe" } else { "/bin/true" };
        let args: Vec<String> = if cfg!(windows) {
            vec!["/c".to_string(), "echo".to_string(), "init".to_string()]
        } else {
            vec![]
        };
        let init_cmd = StartCommand {
            program: program.to_string(),
            args,
            env_vars: BTreeMap::new(),
            working_dir: PathBuf::from("."),
            creation_flags: 0x08000000,
            first_run_init: None,
        };
        let fri = FirstRunInit {
            init_command: init_cmd,
            temp_secret_output: None,
        };
        let result = run_first_run_init(&fri);
        assert!(result.is_ok());
    }

    #[test]
    fn run_first_run_init_fails_on_nonexistent_program() {
        let init_cmd = StartCommand {
            program: "nonexistent-init-xyz.exe".to_string(),
            args: vec![],
            env_vars: BTreeMap::new(),
            working_dir: PathBuf::from("."),
            creation_flags: 0x08000000,
            first_run_init: None,
        };
        let fri = FirstRunInit {
            init_command: init_cmd,
            temp_secret_output: None,
        };
        let result = run_first_run_init(&fri);
        assert!(result.is_err());
    }
}
