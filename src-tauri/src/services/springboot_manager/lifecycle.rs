use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::SoftwareManager;
use crate::utils::process::hidden;

// 注入通道只在 Windows 需要（Unix 的 SIGTERM 本身就是 JVM 能响应的信号）
#[cfg(windows)]
use super::agent;

// 进程判活委托给 health_check 的单 PID 刷新版本：停止流程按秒轮询，
// 全量 `refresh_processes(All)` 会把整个等待期变成反复枚举全系统进程。
pub(crate) fn is_pid_alive(pid: u32) -> bool {
    crate::services::software_manager::health_check::is_process_alive(pid)
}

/// 启动 Spring Boot 应用
pub async fn start_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    app_handle: &AppHandle,
) -> Result<(), String> {
    let app = springboot_mgr.find_app(app_id).map_err(|e| e.to_string())?;

    if matches!(app.status, AppStatus::Running | AppStatus::Starting) {
        return Err("应用已在运行中".to_string());
    }

    // 前置依赖验证
    if !app.dependencies.is_empty() {
        let installed = software_mgr.get_installed();
        for dep_id in &app.dependencies {
            if let Some(dep) = installed.iter().find(|s| s.id == *dep_id) {
                if dep.status != SoftwareStatus::Running {
                    return Err(format!("前置依赖 [{}] 未运行，请先启动后再试", dep.name));
                }
            }
        }
    }

    springboot_mgr
        .update_status(app_id, AppStatus::Starting, None, None)
        .map_err(|e| e.to_string())?;
    let _ = app_handle.emit(
        "springboot-status-changed",
        (app_id.to_string(), "Starting", None::<u32>, None::<String>),
    );

    // 获取 JDK 路径
    let jdk = software_mgr
        .find_installed(&app.jdk_installed_id)
        .ok_or("所选 JDK 未找到，请重新选择")?;
    let java_home = std::path::Path::new(&jdk.install_path);
    let java_bin = if cfg!(windows) {
        let p = java_home.join("bin").join("java.exe");
        if p.exists() {
            p
        } else {
            return Err("java.exe 未找到".to_string());
        }
    } else {
        let p = java_home.join("bin").join("java");
        if p.exists() {
            p
        } else {
            return Err("java 未找到".to_string());
        }
    };

    // ponytail: 日志目录（JAR 同级 logs/）
    let log_dir = std::path::Path::new(&app.jar_path).parent().unwrap().join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // 构建命令
    let mut cmd = hidden(&java_bin);
    for opt in &app.jvm_opts {
        cmd.arg(opt);
    }
    // ponytail: 设工作目录 = JAR 所在目录，Spring Boot 相对路径日志写到正确位置
    if let Some(parent) = std::path::Path::new(&app.jar_path).parent() {
        cmd.current_dir(parent);
    }
    cmd.arg("-jar").arg(&app.jar_path);
    if let Some(p) = app.port { cmd.arg(format!("--server.port={}", p)); }
    if !app.profile.is_empty() { cmd.arg(format!("--spring.profiles.active={}", app.profile)); }
    for arg in &app.program_args { cmd.arg(arg); }
    // 注入环境变量：全局 → 分组 → 应用（同名覆盖）
    for (k, v) in springboot_mgr.get_global_env_vars() { cmd.env(k, v); }
    if let Some(ref group) = app.group {
        for (k, v) in springboot_mgr.get_group_env_vars(group) { cmd.env(k, v); }
    }
    for (k, v) in &app.env_vars { cmd.env(k, v); }

    // ponytail: stdout/stderr → console.log，Spring Boot 自己的日志（info.log 等）不受影响
    let console = std::fs::File::create(log_dir.join("console.log"))
        .map_err(|e| format!("无法创建控制台日志: {}", e))?;
    let console_clone = console.try_clone()
        .map_err(|e| format!("无法克隆句柄: {}", e))?;
    cmd.stdout(std::process::Stdio::from(console));
    cmd.stderr(std::process::Stdio::from(console_clone));

    let child = cmd.spawn().map_err(|e| format!("启动失败: {}", e))?;
    let pid = child.id();

    // 注册到全局进程注册表
    lifecycle::register(app_id.to_string(), pid, app.name.clone(), "springboot".to_string(), "springboot".to_string());

    let console_path = log_dir.join("console.log");
    // ponytail: 每 5s 读日志尾部 4KB 检测 "Started "，不等固定超时；PID 死则秒报
    let mut started = app.port.is_none();
    for _ in 0..60 {
        if !is_pid_alive(pid) {
            springboot_mgr
                .update_status(app_id, AppStatus::Error, Some(pid), Some("进程意外退出".to_string()))
                .ok();
            let _ = app_handle.emit(
                "springboot-status-changed",
                (app_id.to_string(), "Error", Some(pid), Some("进程意外退出".to_string())),
            );
            return Err("应用启动失败：进程意外退出，请检查 console.log 或 JAR 配置".to_string());
        }
        // 检测日志尾部 4KB 中 Spring Boot 启动完成标记
        if !started && console_path.exists() {
            if let Ok(mut f) = std::fs::File::open(&console_path) {
                use std::io::{Read, Seek, SeekFrom};
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                let skip = len.saturating_sub(4096);
                let mut tail = vec![0u8; (len - skip) as usize];
                if f.seek(SeekFrom::Start(skip)).is_ok() && f.read_exact(&mut tail).is_ok() {
                    if String::from_utf8_lossy(&tail).contains("Started ") {
                        started = true;
                    }
                }
            }
        }
        if let Some(port) = app.port {
            if started && tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await.is_ok() {
                springboot_mgr
                    .update_status(app_id, AppStatus::Running, Some(pid), None)
                    .map_err(|e| e.to_string())?;
                let _ = app_handle.emit(
                    "springboot-status-changed",
                    (app_id.to_string(), "Running", Some(pid), None::<String>),
                );
                return Ok(());
            }
        } else if started {
            // 无端口应用：日志显示启动即视为 Running
            springboot_mgr
                .update_status(app_id, AppStatus::Running, Some(pid), None)
                .map_err(|e| e.to_string())?;
            let _ = app_handle.emit(
                "springboot-status-changed",
                (app_id.to_string(), "Running", Some(pid), None::<String>),
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // 300s 兜底
    springboot_mgr
        .update_status(app_id, AppStatus::Error, Some(pid), Some("启动超时（5min），请检查日志".to_string()))
        .map_err(|e| e.to_string())?;
    let _ = app_handle.emit(
        "springboot-status-changed",
        (app_id.to_string(), "Error", Some(pid), Some("启动超时（5min），请检查日志".to_string())),
    );
    Err("应用启动超时（5min），PID 仍存活但未就绪，请检查日志和端口配置".to_string())
}

/// 停止结果：把「操作是否成功」与「是否走了优雅路径」分开。
///
/// 应用被强制终止**同样是停止成功**，只是没能执行 shutdown hook —— 这属于要告知
/// 用户的提示，不是失败。早先把强杀当 `Err` 返回，而 Windows 上「应用没开端点」
/// 恰恰是默认情况，于是 `restart_app` / 换包重启里的 `?` 在主路径上短路
/// （重启退化成「只停不起」），分组停止也会在第一个被强杀的应用处中断。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StopOutcome {
    /// 是否走完了应用的优雅停机（`false` = 强制终止，未执行 shutdown hook）
    pub graceful: bool,
    /// 需要告知用户的提示，仅在 `graceful = false` 时存在
    pub message: Option<String>,
}

impl StopOutcome {
    fn graceful() -> Self {
        Self { graceful: true, message: None }
    }

    fn forced(message: String) -> Self {
        Self { graceful: false, message: Some(message) }
    }
}

/// 停止判定：把「有没有通道 / 有没有自行退出」到结果的决策抽成纯函数，便于覆盖测试。
fn stop_outcome(pid: u32, has_channel: bool, exited: bool, timeout_secs: u64) -> StopOutcome {
    if exited {
        return StopOutcome::graceful();
    }
    if has_channel {
        StopOutcome::forced(format!(
            "应用 PID {pid} 在 {timeout_secs}s 内未退出，已强制终止（本次未执行 shutdown hook）"
        ))
    } else {
        StopOutcome::forced(format!(
            "应用 PID {pid} 未提供优雅停止通道，已直接强制终止（未执行 shutdown hook）"
        ))
    }
}

/// 停止 Spring Boot 应用。
///
/// Windows 上没有 SIGTERM，让 JVM 走完 shutdown hook 的唯一通道是
/// **attach + agent 注入** —— 应用零配置：用 JDK 的 attach API 把一小段代码注入
/// 目标 JVM，让它自己 `System.exit(0)`（详见 [`agent`](super::agent)）；
/// 仍未退出则强制终止。
///
/// 已实测无效、故未采用的手段：`jcmd <pid> Shutdown`（该命令不存在，JDK 8/21 均报
/// `Unknown diagnostic command`）、`taskkill /PID` 不带 `/F`（WM_CLOSE 只对进程
/// 自己拥有的窗口有效）、`AttachConsole` + `CTRL_BREAK`（API 返回成功但信号不送达）。
///
/// 已移除的手段：`POST <Actuator 停止端点>`。它依赖应用侧配置（默认关闭、属性名还随
/// 大版本变），在「端点被禁用 / 站点关闭」的应用上每次停止都会先白等一次连接超时；
/// 而注入对 JRE 与 JDK 目标一律有效，覆盖了它原本的唯一价值场景。
///
/// Unix 上 `kill -TERM` 本身就是 JVM 能响应的信号，无需再注入。
///
/// 返回的 [`StopOutcome`] 只描述「是否走完了优雅流程」；应用真的没停掉才算 `Err`。
pub async fn stop_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    app_handle: &AppHandle,
) -> Result<StopOutcome, String> {
    let app = springboot_mgr.find_app(app_id).map_err(|e| e.to_string())?;

    if !matches!(app.status, AppStatus::Running | AppStatus::Error) {
        return Err("应用未运行".to_string());
    }

    springboot_mgr
        .update_status(app_id, AppStatus::Stopping, None, None)
        .map_err(|e| e.to_string())?;
    let _ = app_handle.emit(
        "springboot-status-changed",
        (app_id.to_string(), "Stopping", None::<u32>, None::<String>),
    );

    let mut outcome = StopOutcome::graceful();
    if let Some(pid) = app.pid {
        // ① 发出优雅停止请求。两条通道按平台分工：
        //    Unix —— SIGTERM 是 JVM 能真正响应的信号，Spring Boot 的优雅停机就建立在其上
        //    Windows —— 没有 SIGTERM，用 attach + agent 注入让 JVM 自己 System.exit(0)
        #[cfg(not(windows))]
        let term_sent = {
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .output();
            tracing::info!(app_id, pid, "已发送 SIGTERM");
            true
        };
        #[cfg(windows)]
        let term_sent = false;

        // agent 让 JVM **自己** System.exit(0)，走完整关闭流程 —— 这是 Windows 上
        // 唯一「应用不用改任何配置」的优雅停止通道，也是本平台唯一的通道。
        #[cfg(windows)]
        let agent_requested = match agent::resolve_launcher(&app, software_mgr) {
            Ok(launcher) => match agent::request_graceful_exit(pid, launcher).await {
                Ok(agent::InjectOutcome::Accepted) => {
                    tracing::info!(app_id, pid, "已注入停止 agent，JVM 将自行退出");
                    true
                }
                // 启动器跑起来了但报错：命令可能已经送达——attach 是客户端先发命令
                // 再读响应，跨版本时客户端解析响应失败而目标已执行（实测）。
                // 所以仍算「发出过停止请求」，值得等待。
                Ok(agent::InjectOutcome::Failed(why)) => {
                    tracing::info!(app_id, pid, why = %why, "停止 agent 注入报错，仍等待应用退出");
                    true
                }
                Err(why) => {
                    tracing::info!(app_id, pid, why = %why, "停止 agent 注入不可用");
                    false
                }
            },
            Err(why) => {
                tracing::info!(app_id, pid, why = %why, "停止 agent 不可用");
                false
            }
        };
        #[cfg(not(windows))]
        let agent_requested = false;

        // ② 等它自己退——**只在确实发出过它能响应的请求时才等**。
        //    没有任何通道时进程不可能自行退出，等满超时纯属空耗
        //    （Windows 上本机没装 JDK 而应用又不是 JDK 启动时就是这种情况，干等只会让停止显得变慢）。
        let has_channel = term_sent || agent_requested;
        let timeout_secs = if has_channel {
            app.stop_timeout_secs.clamp(1, 600)
        } else {
            0
        };
        let mut exited = false;
        for _ in 0..timeout_secs {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !is_pid_alive(pid) {
                exited = true;
                break;
            }
        }

        // ③ 还没退 → 强杀
        if exited {
            tracing::info!(app_id, pid, "应用已自行退出");
        } else {
            force_kill(pid);
            tracing::warn!(app_id, pid, timeout_secs, has_channel, "应用未自行退出，已强制终止");
        }
        outcome = stop_outcome(pid, has_channel, exited, timeout_secs);
    }

    // 从进程注册表中注销
    lifecycle::unregister(app_id);

    // ponytail: 无论优雅退出还是强制终止，进程已死就设 Stopped
    springboot_mgr
        .update_status(app_id, AppStatus::Stopped, None, None)
        .map_err(|e| e.to_string())?;
    let _ = app_handle.emit(
        "springboot-status-changed",
        (app_id.to_string(), "Stopped", None::<u32>, None::<String>),
    );

    Ok(outcome)
}

/// 强制终止进程（Windows `taskkill /F`；Unix `kill -9`）。
fn force_kill(pid: u32) {
    #[cfg(windows)]
    {
        let _ = hidden("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
}

/// 重启 Spring Boot 应用。
///
/// 返回停止阶段的结论：重启本身成功即 `Ok`，若停止时走的是强杀，
/// 提示会挂在本结果的 `message` 上，供 UI 一并告知用户。
pub async fn restart_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    app_handle: &AppHandle,
) -> Result<StopOutcome, String> {
    let outcome = stop_app(app_id, springboot_mgr, software_mgr, app_handle).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    start_app(app_id, springboot_mgr, software_mgr, app_handle).await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{stop_outcome, StopOutcome};
    use std::time::Duration;

    /// 应用自行退出＝真正的优雅停止，不应带任何警告
    #[test]
    fn stop_outcome_graceful_when_app_exited() {
        assert_eq!(stop_outcome(4242, true, true, 30), StopOutcome::graceful());
        // 即使没有任何通道，只要进程退了就是优雅退出
        assert_eq!(stop_outcome(4242, false, true, 0), StopOutcome::graceful());
    }

    /// 强杀**不是失败**：必须返回可序列化的结果而不是 Err，
    /// 否则 `restart_app` / 换包重启里的 `?` 会在主路径上短路（重启变「只停不起」）。
    #[test]
    fn stop_outcome_forced_is_not_an_error() {
        let waited = stop_outcome(4242, true, false, 30);
        assert!(!waited.graceful);
        let msg = waited.message.expect("强杀应带提示");
        assert!(msg.contains("4242") && msg.contains("30s"), "实际: {msg}");

        let no_channel = stop_outcome(4242, false, false, 0);
        assert!(!no_channel.graceful);
        let msg = no_channel.message.expect("无通道强杀应带提示");
        assert!(msg.contains("未提供优雅停止通道"), "实际: {msg}");
    }

    /// 返回前端的字段名必须与 snake_case 约定一致，前端按 `graceful` / `message` 读取
    #[test]
    fn stop_outcome_serializes_with_expected_fields() {
        let json = serde_json::to_value(stop_outcome(1, false, false, 0)).expect("序列化");
        assert_eq!(json["graceful"], serde_json::json!(false));
        assert!(json["message"].is_string());
        let ok = serde_json::to_value(StopOutcome::graceful()).expect("序列化");
        assert_eq!(ok["graceful"], serde_json::json!(true));
        assert!(ok["message"].is_null());
    }

    /// 真实进程验证：强杀确实让进程消失。
    ///
    /// 这是「强杀算停止成功、返回 Ok」的前提 —— 若进程没死却返回 Ok，调用方会带着
    /// 一个还活着的进程继续往下走（重启时端口占用、新旧进程并存），比返回 Err 更糟。
    #[tokio::test]
    async fn force_kill_ends_real_process() {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = super::hidden("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = std::process::Command::new("sleep");
            c.arg("30");
            c
        };
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn().expect("启动测试进程");
        let pid = child.id();
        assert!(super::is_pid_alive(pid), "测试进程应已启动");

        super::force_kill(pid);

        let mut dead = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !super::is_pid_alive(pid) {
                dead = true;
                break;
            }
        }
        let _ = child.wait();
        assert!(dead, "force_kill 后 PID {pid} 仍存活");

        // 进程确实没了 → 结论必须是「成功但未优雅」，而不是 Err
        assert!(!stop_outcome(pid, false, false, 0).graceful);
    }
}
