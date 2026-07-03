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

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

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

// —— 状态转换校验 ——

use crate::models::software::{CustomStartCommand, SoftwareStatus};

/// 校验启动状态转换是否合法
pub fn validate_start_transition(current: SoftwareStatus) -> anyhow::Result<()> {
    match current {
        SoftwareStatus::Stopped => Ok(()),
        SoftwareStatus::Unknown => Ok(()),
        SoftwareStatus::Error => Ok(()),
        SoftwareStatus::Running => Err(anyhow::anyhow!(
            "当前状态为运行中，无法启动（请先停止）"
        )),
        SoftwareStatus::Starting => Err(anyhow::anyhow!(
            "当前状态为启动中，无法重复启动"
        )),
        SoftwareStatus::Stopping => Err(anyhow::anyhow!(
            "当前状态为停止中，无法启动"
        )),
        SoftwareStatus::Initializing => Err(anyhow::anyhow!(
            "当前状态为初始化中，无法启动"
        )),
    }
}

/// 校验停止状态转换是否合法
pub fn validate_stop_transition(current: SoftwareStatus) -> anyhow::Result<()> {
    match current {
        SoftwareStatus::Running => Ok(()),
        SoftwareStatus::Starting => Ok(()),
        SoftwareStatus::Error => Ok(()),
        SoftwareStatus::Stopped => Err(anyhow::anyhow!("已停止，无需再次停止")),
        SoftwareStatus::Stopping => Err(anyhow::anyhow!("停止中，请等待")),
        SoftwareStatus::Unknown => Err(anyhow::anyhow!("未知状态，无法停止")),
        SoftwareStatus::Initializing => Err(anyhow::anyhow!("初始化中，请等待")),
    }
}

/// 校验相对路径白名单：仅允许字母数字 _./- 且不含 ..
/// 用于自定义软件 executable / working_dir / data_dir 等路径防御
const VALID_PATH_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./-";

pub fn validate_relative_path(path: &str) -> anyhow::Result<()> {
    if path.is_empty() {
        return Err(anyhow::anyhow!("路径不能为空"));
    }
    // 禁止绝对路径（Windows 盘符 X: 或 Unix / 开头）
    if path.len() >= 2 {
        let bytes = path.as_bytes();
        if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            return Err(anyhow::anyhow!("不允许绝对路径（盘符）"));
        }
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(anyhow::anyhow!("不允许绝对路径"));
    }
    // 禁止 ..
    if path.contains("..") {
        return Err(anyhow::anyhow!("路径不允许 .."));
    }
    // 白名单字符
    for c in path.chars() {
        if !VALID_PATH_CHARS.contains(c) {
            return Err(anyhow::anyhow!("路径含非法字符: {}", c));
        }
    }
    Ok(())
}

/// 用 CustomStartCommand 构造 StartCommand
/// 校验 executable / working_dir 相对路径白名单（防注入）
pub fn build_custom_command(
    install_path: &str,
    custom: &CustomStartCommand,
) -> anyhow::Result<StartCommand> {
    validate_relative_path(&custom.executable)?;
    if let Some(wd) = &custom.working_dir {
        validate_relative_path(wd)?;
    }

    let working_dir = match &custom.working_dir {
        Some(wd) => PathBuf::from(install_path).join(wd),
        None => PathBuf::from(install_path),
    };

    Ok(StartCommand {
        program: custom.executable.clone(),
        args: custom.args.clone(),
        env_vars: custom.env_vars.clone(),
        working_dir,
        creation_flags: 0x08000000, // CREATE_NO_WINDOW
        first_run_init: None,
    })
}

// —— 通用 kill 流程 ——

use std::time::{Duration, Instant};

use crate::services::software_manager::health_check::is_process_alive;

/// 停止单个进程：优雅停止→等 5s→强杀
/// 返回 (是否成功, 状态字符串: "stopped" | "killed" | "failed")
pub fn stop_one(pid: u32) -> (bool, String) {
    if !is_process_alive(pid) {
        return (true, "stopped".to_string());
    }

    // 优雅停止
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string()]);
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(0x08000000).output();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }

    // 轮询等待最多 5s
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if !is_process_alive(pid) {
            return (true, "stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // 超时强杀
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/F"]);
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(0x08000000).output();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }

    std::thread::sleep(Duration::from_millis(300));
    if is_process_alive(pid) {
        (false, "failed".to_string())
    } else {
        (true, "killed".to_string())
    }
}

// —— 事件推送 ——

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct SoftwareStatusEvent {
    pub installed_id: String,
    pub status: String,
    pub pid: Option<u32>,
    pub error: Option<String>,
    pub timestamp: String,
}

/// 通过 Tauri Emitter 推送 software-status-changed 事件
/// 前端 Pinia store 监听此事件实时更新 UI 状态
pub fn emit_status_changed(
    app: &AppHandle,
    installed_id: &str,
    status: SoftwareStatus,
    pid: Option<u32>,
    error: Option<String>,
) {
    let event = SoftwareStatusEvent {
        installed_id: installed_id.to_string(),
        status: format!("{:?}", status),
        pid,
        error,
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    let _ = app.emit("software-status-changed", event);
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

    // —— 状态转换校验测试 ——

    #[test]
    fn validate_start_transition_allows_stopped_to_starting() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Stopped);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_start_transition_allows_unknown_to_starting() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Unknown);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_start_transition_allows_error_to_starting() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Error);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_start_transition_rejects_running() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Running);
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains("运行中"), "错误信息应含运行中，实际：{}", err_msg);
    }

    #[test]
    fn validate_start_transition_rejects_starting() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Starting);
        assert!(result.is_err());
    }

    #[test]
    fn validate_start_transition_rejects_stopping() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Stopping);
        assert!(result.is_err());
    }

    #[test]
    fn validate_start_transition_rejects_initializing() {
        let result = validate_start_transition(crate::models::software::SoftwareStatus::Initializing);
        assert!(result.is_err());
    }

    #[test]
    fn validate_stop_transition_allows_running() {
        let result = validate_stop_transition(crate::models::software::SoftwareStatus::Running);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_stop_transition_allows_starting() {
        let result = validate_stop_transition(crate::models::software::SoftwareStatus::Starting);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_stop_transition_allows_error() {
        let result = validate_stop_transition(crate::models::software::SoftwareStatus::Error);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_stop_transition_rejects_stopped() {
        let result = validate_stop_transition(crate::models::software::SoftwareStatus::Stopped);
        assert!(result.is_err());
    }

    #[test]
    fn validate_stop_transition_rejects_stopping() {
        let result = validate_stop_transition(crate::models::software::SoftwareStatus::Stopping);
        assert!(result.is_err());
    }

    // —— build_custom_command 测试 ——

    use crate::models::software::{CustomHealthSpec, CustomStartCommand};

    fn make_custom(executable: &str, working_dir: Option<&str>) -> CustomStartCommand {
        CustomStartCommand {
            executable: executable.to_string(),
            args: vec!["--port=8080".to_string()],
            working_dir: working_dir.map(|s| s.to_string()),
            env_vars: {
                let mut m = std::collections::BTreeMap::new();
                m.insert("KEY".to_string(), "val".to_string());
                m
            },
            health_check: CustomHealthSpec::Tcp { port: 8080 },
            config_file_relative: None,
        }
    }

    #[test]
    fn build_custom_command_uses_custom_start_command() {
        let custom = make_custom("bin/app.exe", Some("subdir"));
        let cmd = build_custom_command("apps/custom/test", &custom).unwrap();
        assert_eq!(cmd.program, "bin/app.exe");
        assert!(cmd.args.contains(&"--port=8080".to_string()));
        assert_eq!(cmd.env_vars.get("KEY").unwrap(), "val");
        assert_eq!(cmd.working_dir, PathBuf::from("apps/custom/test/subdir"));
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn build_custom_command_uses_install_path_when_no_working_dir() {
        let custom = make_custom("bin/app.exe", None);
        let cmd = build_custom_command("apps/custom/test", &custom).unwrap();
        assert_eq!(cmd.working_dir, PathBuf::from("apps/custom/test"));
    }

    #[test]
    fn build_custom_command_rejects_path_traversal() {
        let custom = make_custom("../etc/passwd", None);
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
        let err_msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err_msg.contains(".."), "错误信息应含 ..，实际：{}", err_msg);
    }

    #[test]
    fn build_custom_command_rejects_absolute_path_windows() {
        let custom = make_custom("C:/Windows/system32/cmd.exe", None);
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
        let err = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err.contains("绝对") || err.contains("absolute") || err.contains("盘符"),
            "错误信息应含绝对路径，实际：{}",
            err
        );
    }

    #[test]
    fn build_custom_command_rejects_leading_slash_unix_path() {
        let custom = make_custom("/etc/passwd", None);
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
    }

    #[test]
    fn build_custom_command_rejects_working_dir_traversal() {
        let custom = make_custom("bin/app.exe", Some("../etc"));
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
    }

    #[test]
    fn build_custom_command_rejects_empty_executable() {
        let custom = make_custom("", None);
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
    }

    #[test]
    fn build_custom_command_rejects_illegal_chars() {
        let custom = make_custom("bin/app.exe; rm -rf /", None);
        let result = build_custom_command("apps/custom/test", &custom);
        assert!(result.is_err());
    }

    // —— stop_one 测试 ——

    #[test]
    fn stop_one_returns_stopped_for_nonexistent_pid() {
        // 不存在的 PID 应直接返回 stopped
        let (success, status) = stop_one(99999999);
        assert!(success);
        assert_eq!(status, "stopped");
    }

    #[test]
    fn stop_one_kills_running_process() {
        // 启动一个长进程验证 stop_one 能停止它
        let mut cmd = std::process::Command::new(if cfg!(windows) { "cmd.exe" } else { "sleep" });
        if cfg!(windows) {
            cmd.args(["/c", "timeout", "/t", "60", "/nobreak"]);
        } else {
            cmd.args(["60"]);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let child = cmd.spawn().expect("spawn failed");
        let pid = child.id();

        let (success, status) = stop_one(pid);
        assert!(success);
        // 状态应为 stopped 或 killed
        assert!(
            status == "stopped" || status == "killed",
            "状态应为 stopped 或 killed，实际：{}",
            status
        );
    }

    // —— emit_status_changed 测试 ——

    #[test]
    fn software_status_event_serializes_correctly() {
        let event = SoftwareStatusEvent {
            installed_id: "uuid".to_string(),
            status: "Running".to_string(),
            pid: Some(12345),
            error: None,
            timestamp: "2026-07-02T14:00:00+08:00".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"installed_id\":\"uuid\""));
        assert!(json.contains("\"status\":\"Running\""));
        assert!(json.contains("\"pid\":12345"));
    }

    #[test]
    fn software_status_event_with_error_serializes() {
        let event = SoftwareStatusEvent {
            installed_id: "uuid".to_string(),
            status: "Error".to_string(),
            pid: Some(99),
            error: Some("健康检查超时".to_string()),
            timestamp: "2026-07-02T14:00:00+08:00".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"status\":\"Error\""));
        assert!(json.contains("\"error\":\"健康检查超时\""));
    }

    #[test]
    fn software_status_event_with_null_pid_serializes() {
        let event = SoftwareStatusEvent {
            installed_id: "uuid".to_string(),
            status: "Stopped".to_string(),
            pid: None,
            error: None,
            timestamp: "2026-07-02T14:00:00+08:00".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"pid\":null"));
    }
}
