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

/// 解析 program 路径：
/// - 绝对路径直接返回
/// - 相对路径：若 working_dir/program 存在，返回绝对路径（避免 Windows
///   CreateProcessW 用父进程工作目录查找的问题）；否则返回原值，让系统从 PATH 查找
fn resolve_program_path(program: &str, working_dir: &std::path::Path) -> PathBuf {
    let p = PathBuf::from(program);
    if p.is_absolute() {
        return p;
    }
    let candidate = working_dir.join(program);
    if candidate.exists() {
        candidate
    } else {
        p
    }
}

/// 计算 stdout/stderr 重定向落盘路径：<install_path>/logs/opx-<installed_id>.log
/// 文件名用 installed_id 而非 pid：pid 在 spawn 前不可知，且 Windows 下 rename 打开中的文件会失败。
/// install_path 取 StartCommand.working_dir 解析后的绝对路径（各 provider 的 working_dir 均为 install_path）。
pub fn stdout_log_path(install_path: &std::path::Path, installed_id: &str) -> PathBuf {
    // 防御性 sanitize：installed_id 可能含 / : 等非法文件名字符，
    // 直接拼进文件名会造成路径穿越或 File::create 失败（Windows）。
    // 仅保留字母数字与 - _，其余一律替换为下划线。
    let safe_id: String = installed_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    install_path
        .join("logs")
        .join(format!("opx-{}.log", safe_id))
}

/// 用 StartCommand 构造并 spawn 子进程，将其 stdout+stderr 重定向到
/// <install_path>/logs/opx-<installed_id>.log（installed_id 在 spawn 前即可确定）。
///
/// 文件句柄通过 `Stdio::from(file)` 移交给子进程持有，父进程不保留副本，
/// 进程退出后文件可读（Windows 下 rename 打开中的文件会失败，故文件名用 installed_id）。
pub fn spawn_process(cmd: StartCommand, installed_id: &str) -> anyhow::Result<Child> {
    let program_path = resolve_program_path(&cmd.program, &cmd.working_dir);

    let mut command = Command::new(&program_path);
    command.args(&cmd.args).current_dir(&cmd.working_dir);

    for (k, v) in &cmd.env_vars {
        command.env(k, v);
    }
    // 从继承环境移除宿主注入的危险变量（如 SERVER_PORT 会污染 Spring Boot 端口）
    for name in &cmd.remove_envs {
        command.env_remove(name);
    }

    #[cfg(windows)]
    command.creation_flags(cmd.creation_flags);

    // stdout/stderr 重定向到日志文件（先于 spawn 创建目录与文件）
    let log_path = stdout_log_path(&cmd.working_dir, installed_id);
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let log_file = std::fs::File::create(&log_path)
        .map_err(|e| anyhow::anyhow!("创建日志文件失败 {}: {}", log_path.display(), e))?;
    let log_file_stderr = log_file
        .try_clone()
        .map_err(|e| anyhow::anyhow!("克隆日志文件句柄失败: {}", e))?;

    let child = command
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_stderr))
        .spawn()?;
    Ok(child)
}

/// 首次初始化命令超时阈值（秒）。
/// MySQL 在慢盘/首次生成随机数据初始化时可超过 60s，放宽到 180s 避免误杀。
const INIT_TIMEOUT_SECS: u64 = 180;

/// 执行首次初始化命令（同步等待，最多 `INIT_TIMEOUT_SECS` 秒）
/// 用于 MySQL --initialize-insecure 等场景
///
/// stdout/stderr 设为 `Stdio::null()`（见 RC1 修复）：避免在轮询期间不读管道而
/// 造成管道缓冲死锁，也避免把 mysqld 的真实报错闷在管道里。诊断由 provider 的
/// --log-error 落文件。因此返回的 `Output` 中 stdout/stderr 为空。
/// `temp_secret_output` 仍由调用方（software.rs）按需处理。
pub fn run_first_run_init(fri: &FirstRunInit) -> anyhow::Result<std::process::Output> {
    let init = &fri.init_command;
    let program_path = resolve_program_path(&init.program, &init.working_dir);
    let mut command = Command::new(&program_path);
    command.args(&init.args).current_dir(&init.working_dir);

    for (k, v) in &init.env_vars {
        command.env(k, v);
    }
    for name in &init.remove_envs {
        command.env_remove(name);
    }

    #[cfg(windows)]
    command.creation_flags(init.creation_flags);

    // 反模式修正（F1/RC1）：原先 piped stdout+stderr 但轮询期间从不读取，会造成管道
    // 缓冲死锁、且把 mysqld 真实报错闷在管道。改为 Stdio::null()，诊断统一由 provider
    // 的 --log-error 落文件（见 mysql.rs init 命令）。--initialize-insecure 无临时密码
    // 需求，null 管道不影响后续逻辑。
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let start = Instant::now();
    let timeout = Duration::from_secs(INIT_TIMEOUT_SECS);
    loop {
        match child.try_wait()? {
            Some(_) => {
                // 进程已退出（stdout/stderr 为 null，无管道需回收）；
                // child.wait() 取最终状态（std 已缓存 status，重复调用安全）。
                let status = child.wait()?;
                if !status.success() {
                    return Err(anyhow::anyhow!("初始化命令失败（code={}）", status));
                }
                // stdout/stderr 为 null，组装空 Output 以保持函数返回类型不变
                return Ok(std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return Err(anyhow::anyhow!(
                        "初始化命令超时（{}s），已终止子进程",
                        INIT_TIMEOUT_SECS
                    ));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// 首次初始化前清理可能残留的半初始化 data 目录（RC3 修复的一部分）。
///
/// 行为契约：
/// - `data_dir` 不存在 → 返回 `Ok(false)`（首次运行，无需清理）。
/// - `data_dir` 存在且为空目录 → 返回 `Ok(false)`（无需清理）。
/// - `data_dir` 存在且为非空目录 → `remove_dir_all` 后 `create_dir_all` 重建，
///   返回 `Ok(true)`（已清空重建）。
/// - `data_dir` 存在但不是目录（如误用成文件/安装根）→ 返回 `Err`，避免误删/误用。
///
/// 调用方必须保证 `data_dir` 仅为 MySQL 的数据子目录（install_path/data），
/// 而非安装根目录本身；删除前通过 `is_dir()` 二次校验，避免误删安装根。
pub(crate) fn wipe_data_dir_if_nonempty(data_dir: &std::path::Path) -> anyhow::Result<bool> {
    // 不存在 → 无需清理
    if !data_dir.exists() {
        return Ok(false);
    }
    // 存在但不是目录：可能是误用（路径指向安装根或文件），先拦截再判空，
    // 否则 read_dir 会对文件返回 Err 被吞掉，绕过此护栏。
    if !data_dir.is_dir() {
        return Err(anyhow::anyhow!(
            "data 路径存在但不是目录，无法安全清空: {}",
            data_dir.display()
        ));
    }
    // 空目录 → 无需清理
    let nonempty = std::fs::read_dir(data_dir)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if !nonempty {
        return Ok(false);
    }
    std::fs::remove_dir_all(data_dir)?;
    std::fs::create_dir_all(data_dir)?;
    Ok(true)
}

/// 一键重置：对每个数据目录重建空态（保留目录本身，清空其下所有内容）。
///
/// 护栏：
/// - 目录必须位于 `install_path` 下（拒绝安装根/意外路径，防误删）
/// - 存在但不是目录（如误用成文件/安装根）→ 拦截返回 `Err`
///
/// 复用 `wipe_data_dir_if_nonempty` 的「非空才删、删后重建」语义。
pub(crate) fn reset_data_dirs(dirs: &[PathBuf], install_path: &std::path::Path) -> anyhow::Result<()> {
    for dir in dirs {
        // 护栏：必须在 install_path 下，避免误删安装根或系统目录
        if !dir.starts_with(install_path) {
            return Err(anyhow::anyhow!(
                "数据目录不在安装目录下，拒绝重置: {}",
                dir.display()
            ));
        }
        // 存在但不是目录：可能是误用（路径指向安装根或文件），先拦截避免误删
        if dir.exists() && !dir.is_dir() {
            return Err(anyhow::anyhow!(
                "数据路径存在但不是目录，无法安全重置: {}",
                dir.display()
            ));
        }
        wipe_data_dir_if_nonempty(dir)?;
    }
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
        // Initializing 允许停止（初始化失败卡住时需要能停止恢复）
        SoftwareStatus::Initializing => Ok(()),
        SoftwareStatus::Stopped => Err(anyhow::anyhow!("已停止，无需再次停止")),
        SoftwareStatus::Stopping => Err(anyhow::anyhow!("停止中，请等待")),
        SoftwareStatus::Unknown => Err(anyhow::anyhow!("未知状态，无法停止")),
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
        remove_envs: Vec::new(),
        creation_flags: 0x08000000, // CREATE_NO_WINDOW
        first_run_init: None,
    })
}

// —— 通用 kill 流程 ——

use std::time::{Duration, Instant};

use crate::services::software_manager::health_check::is_process_alive;

/// 停止单个进程：优雅停止→等 5s→强杀
/// 返回 (是否成功, 状态字符串: "stopped" | "killed" | "failed" | "invalid")
pub fn stop_one(pid: u32) -> (bool, String) {
    // 防御 PID 0（Unix kill 0 会杀整个进程组）
    if pid == 0 {
        return (false, "invalid".to_string());
    }
    if !is_process_alive(pid) {
        return (true, "stopped".to_string());
    }

    // 优雅停止。Windows 侧要认清它的能力边界：
    // `taskkill` 不带 `/F` 发的是 WM_CLOSE，**只对进程自己拥有窗口的程序有效**；
    // 被管软件（mysql / redis / nginx 等）都是控制台程序，窗口属于 conhost.exe，
    // 实测会被直接拒绝（「只能强行终止这个进程(带 /F 选项)」）。
    // 故只给它一次很短的机会（GUI 程序响应 WM_CLOSE 通常只要几百毫秒），不再空等 5 秒。
    // 真要按软件语义优雅停止，得用各自的关闭命令
    // （mysqladmin shutdown / redis-cli shutdown / nginx -s quit），属后续项。
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T"]);
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW).output();
    }
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }

    // 等待自行退出：Unix 的 SIGTERM 是应用能真正响应的信号，给足清理时间；
    // Windows 上面已说明几乎没有生效可能，故只留一个短窗口
    let grace = if cfg!(windows) {
        Duration::from_millis(1500)
    } else {
        Duration::from_secs(5)
    };
    let start = Instant::now();
    while start.elapsed() < grace {
        if !is_process_alive(pid) {
            return (true, "stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // 超时强杀
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        use std::os::windows::process::CommandExt;
        let _ = cmd.creation_flags(CREATE_NO_WINDOW).output();
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
    if let Err(e) = app.emit("software-status-changed", event) {
        tracing::warn!(error = %e, installed_id = %installed_id, "emit software-status-changed 失败");
    }
}

// —— 启动钩子（auto_start）与退出钩子（stop_all）——

use std::sync::Arc;

use crate::models::software::InstalledSoftware;
use crate::services::software_manager::SoftwareManager;

/// 应用启动时按 startup_order 拉起 auto_start=true 的实例
///
/// 同 startup_order 的实例会被分组并发拉起（不等单个完成，仅 sleep 500ms 间隔）；
/// 不同 startup_order 的批次之间也只 sleep 500ms。
/// 应在 Tauri setup hook 中通过 `tauri::async_runtime::spawn` 调用。
pub async fn auto_start_all(manager: &Arc<SoftwareManager>, app: &tauri::AppHandle) {
    let auto_list = manager.list_auto_start();
    if auto_list.is_empty() {
        return;
    }
    tracing::info!(count = auto_list.len(), "auto_start_on_boot");

    // 同 startup_order 分组：碰到不同的 order 时先 drain 当前 pending 批次
    let mut last_order: u32 = 0;
    let mut pending: Vec<InstalledSoftware> = Vec::new();

    for sw in auto_list {
        if !pending.is_empty() && sw.startup_order != last_order {
            for s in pending.drain(..) {
                spawn_start(manager.clone(), app.clone(), s.id).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
        last_order = sw.startup_order;
        pending.push(sw);
    }
    // 处理剩余
    for s in pending {
        spawn_start(manager.clone(), app.clone(), s.id).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// spawn 单个 auto_start 任务（fire-and-forget）
/// 不等待 do_start_software 完成，避免单个慢启动阻塞后续实例
async fn spawn_start(
    manager: Arc<SoftwareManager>,
    app: tauri::AppHandle,
    installed_id: String,
) {
    let manager_clone = manager.clone();
    let app_clone = app.clone();
    let id_clone = installed_id.clone();
    tokio::spawn(async move {
        let result =
            crate::commands::software::do_start_software(&manager_clone, &app_clone, &id_clone, None)
                .await;
        if let Err(e) = result {
            tracing::error!(error = %e, installed_id = %id_clone, "auto_start failed");
        }
    });
}

/// 应用退出时停止所有运行中的进程（软件 + SpringBoot），逐个 emit 进度并最终 emit stop-complete。
/// 同步调用，每个进程含 5s 优雅等待 + 强杀。
pub fn stop_all_on_exit(app: &AppHandle) {
    let procs = drain();
    if procs.is_empty() {
        let _ = app.emit("stop-complete", ());
        return;
    }
    let total = procs.len();
    tracing::info!(count = total, "stop_all_on_exit");
    for (i, p) in procs.iter().enumerate() {
        let (success, status) = stop_one(p.pid);
        let _ = app.emit(
            "stop-progress",
            serde_json::json!({
                "current": i + 1,
                "total": total,
                "name": p.name,
                "status": status,
            }),
        );
        tracing::info!(
            installed_id = %p.installed_id, pid = p.pid,
            success = success, status = %status, "stopped on exit"
        );
    }
    let _ = app.emit("stop-complete", ());
}

#[cfg(test)]
mod tests {
    use super::stdout_log_path;
    use std::path::Path;

    #[test]
    fn test_stdout_log_path_shape() {
        let id = "9f1c2b3a-4d5e-6f70-8a9b-0c1d2e3f4a5b";
        let p = stdout_log_path(Path::new("/opt/opx/inst1"), id);
        let fname = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(fname, format!("opx-{}.log", id));
        // 路径应包含 logs 目录组件（跨平台）
        let has_logs = p.components().any(|c| c.as_os_str() == "logs");
        assert!(has_logs, "stdout 落盘路径应包含 logs 目录组件");
    }

    /// 预期：installed_id 若含非法文件名字符（/ : 等），落盘文件名必须被 sanitize，
    /// 否则 Windows 上 File::create 会失败、或造成路径穿越。
    /// 当前实现未 sanitize => 该断言预期失败（installed_id 实际为 UUID，故为防御性 P2）。
    #[test]
    fn test_stdout_log_path_sanitizes_illegal_chars() {
        let p = stdout_log_path(Path::new("/opt/opx/inst1"), "a/b:c");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            !name.contains('/') && !name.contains('\\') && !name.contains(':'),
            "stdout_log_path 必须对 installed_id 做文件名 sanitize，当前文件名: {}",
            name
        );
    }
}
