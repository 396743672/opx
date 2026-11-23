use std::time::Duration;

use std::sync::Arc;

use crate::event::{EventSink, EventSinkExt};

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

/// 定位 JDK/JRE 里的 `java` 可执行文件。
///
/// 先按正常安装取 `<根>/bin/java[e]`；找不到时**再扫描一层子目录**兜底。
/// 为什么需要兜底：官方 Linux 包（Adoptium）是「顶层目录 + 内容」结构，历史版本
/// 的 tar 解压没有剥这层，装出来是 `apps/jdk/<ver>/jdk-<ver>/bin/java`——直接拼
/// `bin/java` 必然报「java 不存在」。安装侧已在 `utils::archive::flatten_single_root`
/// 修好新装，这里负责让**存量装坏的实例不重装也能跑**。
fn resolve_java_bin(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let exec = if cfg!(windows) { "java.exe" } else { "java" };
    let direct = root.join("bin").join(exec);
    if direct.exists() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let candidate = entry.path().join("bin").join(exec);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// 启动 Spring Boot 应用
pub async fn start_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    sink: &Arc<dyn EventSink>,
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
    sink.emit_ser(
        "springboot-status-changed",
        (app_id.to_string(), "Starting", None::<u32>, None::<String>),
    );

    // 失败统一出口：写入 Starting 之后的任何失败都必须把状态回滚为 Error 并带上原因，
    // 否则应用永久卡在「启动中」——编辑/删除/换包按钮全灰，只能重启 opx 恢复。
    // 这正是便携版目录迁移后旧 JDK 记录失效时用户遇到的现象。
    let fail = |msg: String| -> String {
        let _ = springboot_mgr.update_status(
            app_id,
            AppStatus::Error,
            None,
            Some(msg.clone()),
        );
        sink.emit_ser(
            "springboot-status-changed",
            (app_id.to_string(), "Error", None::<u32>, Some(msg.clone())),
        );
        msg
    };

    // 获取 JDK 路径
    let jdk = match software_mgr.find_installed(&app.jdk_installed_id) {
        Some(j) => j,
        None => return Err(fail(format!(
            "所选 JDK 未找到（记录 {}），请重新选择 JDK 后再启动",
            app.jdk_installed_id
        ))),
    };
    let java_home = std::path::Path::new(&jdk.install_path);
    let java_exec = if cfg!(windows) { "java.exe" } else { "java" };
    let java_bin = match resolve_java_bin(java_home) {
        Some(p) => p,
        None => {
            let expected = java_home.join("bin").join(java_exec);
            return Err(fail(format!(
                "JDK [{}] 的 {} 不存在：{}（目录可能已被移动或卸载，请在编辑中重新选择 JDK）",
                jdk.name,
                java_exec,
                expected.display()
            )));
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
    // 本机 IP：Spring Boot 用 Spring Cloud InetUtils 自探测 IP，会误取 VMware/VirtualBox
    // 等虚拟网卡 IP。默认注入 ignored-interfaces 剔除虚拟网卡；若配置了 local_ip 或能
    // 自动探测到真实物理网卡 IP，则通过 default-ip-address 钉死该 IP（避免服务发现错址）。
    cmd.arg(format!(
        "-Dspring.cloud.inetutils.ignored-interfaces={}",
        crate::utils::local_ip::IGNORED_INTERFACES
    ));
    let sb_local_ip = app.local_ip.trim();
    let ip = if !sb_local_ip.is_empty() {
        Some(sb_local_ip.to_string())
    } else {
        crate::utils::local_ip::preferred_local_ip()
    };
    if let Some(ip) = ip {
        cmd.arg(format!("-Dspring.cloud.inetutils.default-ip-address={}", ip));
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
        .map_err(|e| fail(format!("无法创建控制台日志: {}", e)))?;
    let console_clone = console.try_clone()
        .map_err(|e| fail(format!("无法克隆句柄: {}", e)))?;
    cmd.stdout(std::process::Stdio::from(console));
    cmd.stderr(std::process::Stdio::from(console_clone));

    let child = cmd.spawn().map_err(|e| fail(format!("启动失败: {}", e)))?;
    let pid = child.id();

    // 注册到全局进程注册表
    lifecycle::register(app_id.to_string(), pid, app.name.clone(), "springboot".to_string(), "springboot".to_string(), 0);

    let console_path = log_dir.join("console.log");
    // ponytail: 每 5s 读日志尾部 4KB 检测 "Started "，不等固定超时；PID 死则秒报
    let mut started = app.port.is_none();
    for _ in 0..60 {
        if !is_pid_alive(pid) {
            springboot_mgr
                .update_status(app_id, AppStatus::Error, Some(pid), Some("进程意外退出".to_string()))
                .ok();
            sink.emit_ser(
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
                sink.emit_ser(
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
            sink.emit_ser(
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
    sink.emit_ser(
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
///
/// `no_channel_reason` 是没有通道时的**具体原因**（如「本机未安装 JDK」），由
/// [`stop_app`] 从注入失败的返回值里透传上来。光说「没有通道」用户无从下手——同一句
/// 提示背后可能是「没装 JDK」「所选运行时被删了」「启动器起不来」三种完全不同的处置。
fn stop_outcome(
    pid: u32,
    has_channel: bool,
    exited: bool,
    timeout_secs: u64,
    no_channel_reason: Option<&str>,
) -> StopOutcome {
    if exited {
        return StopOutcome::graceful();
    }
    if has_channel {
        StopOutcome::forced(format!(
            "应用 PID {pid} 在 {timeout_secs}s 内未退出，已强制终止（本次未执行 shutdown hook）"
        ))
    } else {
        let why = match no_channel_reason {
            Some(r) if !r.trim().is_empty() => format!("（{r}）"),
            _ => String::new(),
        };
        StopOutcome::forced(format!(
            "应用 PID {pid} 没有可用的优雅停止通道{why}，已直接强制终止（未执行 shutdown hook）"
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
// `software_mgr` 仅在 Windows 优雅停止分支（注入 agent）使用，Linux 下未使用。
#[cfg_attr(not(windows), allow(unused_variables))]
pub async fn stop_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    sink: &Arc<dyn EventSink>,
) -> Result<StopOutcome, String> {
    let app = springboot_mgr.find_app(app_id).map_err(|e| e.to_string())?;

    if !matches!(app.status, AppStatus::Running | AppStatus::Error) {
        return Err("应用未运行".to_string());
    }

    springboot_mgr
        .update_status(app_id, AppStatus::Stopping, None, None)
        .map_err(|e| e.to_string())?;
    sink.emit_ser(
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
        // 没通道时把原因带出来，最终拼进给用户的提示里。
        #[cfg(windows)]
        let (agent_requested, no_channel_reason) =
            match agent::resolve_launcher(&app, software_mgr) {
                Ok(launcher) => match agent::request_graceful_exit(pid, launcher).await {
                    Ok(agent::InjectOutcome::Accepted) => {
                        tracing::info!(app_id, pid, "已注入停止 agent，JVM 将自行退出");
                        (true, None)
                    }
                    // 启动器跑起来了但报错：命令可能已经送达——attach 是客户端先发命令
                    // 再读响应，跨版本时客户端解析响应失败而目标已执行（实测）。
                    // 所以仍算「发出过停止请求」，值得等待。
                    Ok(agent::InjectOutcome::Failed(why)) => {
                        tracing::info!(app_id, pid, why = %why, "停止 agent 注入报错，仍等待应用退出");
                        (true, None)
                    }
                    Err(why) => {
                        tracing::info!(app_id, pid, why = %why, "停止 agent 注入不可用");
                        (false, Some(why))
                    }
                },
                Err(why) => {
                    tracing::info!(app_id, pid, why = %why, "停止 agent 不可用");
                    (false, Some(why))
                }
            };
        #[cfg(not(windows))]
        let (agent_requested, no_channel_reason): (bool, Option<String>) = (false, None);

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
            tracing::warn!(
                app_id,
                pid,
                timeout_secs,
                has_channel,
                reason = ?no_channel_reason,
                "应用未自行退出，已强制终止"
            );
        }
        outcome = stop_outcome(pid, has_channel, exited, timeout_secs, no_channel_reason.as_deref());
    }

    // 从进程注册表中注销
    lifecycle::unregister(app_id);

    // ponytail: 无论优雅退出还是强制终止，进程已死就设 Stopped
    springboot_mgr
        .update_status(app_id, AppStatus::Stopped, None, None)
        .map_err(|e| e.to_string())?;
    sink.emit_ser(
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
    #[cfg(unix)]
    {
        // 直接 syscall 发 SIGKILL，不依赖 PATH 上的 `kill` 二进制——
        // 精简/沙箱环境下 `kill` 可能不在 PATH，导致静默失败、进程残留。
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
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
    sink: &Arc<dyn EventSink>,
) -> Result<StopOutcome, String> {
    let outcome = stop_app(app_id, springboot_mgr, software_mgr, sink).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    start_app(app_id, springboot_mgr, software_mgr, sink).await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{stop_outcome, StopOutcome, AppStatus, SpringBootManager, SoftwareManager, Arc, EventSink};
    use crate::models::springboot::{CreateAppParams, SpringBootStore};
    use std::time::Duration;

    // OPX_APP_ROOT 是全局环境变量；并行测试会互相覆盖导致污染，统一串行化。
    static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 应用自行退出＝真正的优雅停止，不应带任何警告
    #[test]
    fn stop_outcome_graceful_when_app_exited() {
        assert_eq!(stop_outcome(4242, true, true, 30, None), StopOutcome::graceful());
        // 即使没有任何通道，只要进程退了就是优雅退出
        assert_eq!(stop_outcome(4242, false, true, 0, None), StopOutcome::graceful());
    }

    /// 强杀**不是失败**：必须返回可序列化的结果而不是 Err，
    /// 否则 `restart_app` / 换包重启里的 `?` 会在主路径上短路（重启变「只停不起」）。
    #[test]
    fn stop_outcome_forced_is_not_an_error() {
        let waited = stop_outcome(4242, true, false, 30, None);
        assert!(!waited.graceful);
        let msg = waited.message.expect("强杀应带提示");
        assert!(msg.contains("4242") && msg.contains("30s"), "实际: {msg}");

        let no_channel = stop_outcome(4242, false, false, 0, None);
        assert!(!no_channel.graceful);
        let msg = no_channel.message.expect("无通道强杀应带提示");
        assert!(msg.contains("没有可用的优雅停止通道"), "实际: {msg}");
    }

    /// 无通道时必须把**具体原因**带给用户，否则同一句「没有通道」背后可能是
    /// 「没装 JDK」「运行时被删」「启动器起不来」三种完全不同的处置方式。
    #[test]
    fn stop_outcome_carries_the_no_channel_reason() {
        let outcome = stop_outcome(
            7720,
            false,
            false,
            0,
            Some("本机未安装 JDK，装任意一个 JDK 即可通过 attach 注入优雅停止"),
        );
        let msg = outcome.message.expect("应带提示");
        assert!(msg.contains("本机未安装 JDK"), "原因应出现在提示里，实际: {msg}");
        assert!(msg.contains("7720"), "实际: {msg}");

        // 空白原因不该留下一对空括号
        let blank = stop_outcome(7720, false, false, 0, Some("  "));
        let msg = blank.message.expect("应带提示");
        assert!(!msg.contains("（）"), "空原因不应渲染成空括号，实际: {msg}");
    }

    /// 返回前端的字段名必须与 snake_case 约定一致，前端按 `graceful` / `message` 读取
    #[test]
    fn stop_outcome_serializes_with_expected_fields() {
        let json = serde_json::to_value(stop_outcome(1, false, false, 0, None)).expect("序列化");
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
            // 先回收（reap）僵尸：SIGKILL 后子进程在父进程 wait 前是 zombie，
            // is_pid_alive（sysinfo）仍会把它判为「存活」，必须 try_wait 让它退出。
            let _ = child.try_wait();
            if !super::is_pid_alive(pid) {
                dead = true;
                break;
            }
        }
        let _ = child.wait();
        assert!(dead, "force_kill 后 PID {pid} 仍存活");

        // 进程确实没了 → 结论必须是「成功但未优雅」，而不是 Err
        assert!(!stop_outcome(pid, false, false, 0, None).graceful);
    }

    /// 复现：启动前校验失败（JDK 记录找不到）后，状态绝不能停在 `Starting`。
    ///
    /// 用户把便携版从 Desktop\opx 移到 d:/opx 后，旧 JRE 目录已不存在，
    /// 启动时 `find_installed` 拿到的记录指向无效目录 → java.exe 检查失败。
    /// 旧代码在写入 Starting 之后直接 `return Err`，没有回滚，导致应用永久
    /// 卡在「启动中」、所有按钮灰化，只能重启 opx 恢复。本测试钉死回滚行为。
    #[tokio::test]
    async fn start_failure_rolls_back_status_from_starting() {
        let _env_guard = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Noop sink：只关心状态持久化，不校验事件广播
        struct NoopSink;
        impl EventSink for NoopSink {
            fn emit(&self, _event: &str, _payload: serde_json::Value) {}
        }

        let root = std::env::temp_dir().join(format!("opx-sb-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建测试根目录");
        std::env::set_var("OPX_APP_ROOT", &root);
        // create_app 会校验源 JAR 存在并复制到 data 目录；造一个占位文件即可。
        // 真正 spawn 之前就会因 JDK 记录不存在而失败，JAR 内容不会被解析。
        let dummy_jar = root.join("dummy-source.jar");
        std::fs::write(&dummy_jar, b"placeholder").expect("写占位 JAR");

        let sb = SpringBootManager::new();
        let sw = SoftwareManager::new();
        let sink: Arc<dyn EventSink> = Arc::new(NoopSink);

        // 造一个指向不存在 JDK 记录的应用，避免真的 spawn java 进程。
        // start_app 在 find_installed 失败处返回，须把状态写回 Error/Stopped。
        let app = sb.create_app(CreateAppParams {
            jar_path: dummy_jar.to_string_lossy().to_string(),
            name: "rollback-probe".to_string(),
            jdk_installed_id: "nonexistent-jdk-id".to_string(),
            jvm_opts: vec![],
            program_args: vec![],
            profile: String::new(),
            env_vars: vec![],
            port: Some(18080),
            log_path: String::new(),
            dependencies: vec![],
            auto_start: false,
            startup_order: 0,
            auto_restart: false,
            group: None,
            jdk_type: "jdk".to_string(),
            stop_timeout_secs: 30,
            local_ip: String::new(),
        }).expect("创建测试应用");

        let result = super::start_app(&app.id, &sb, &sw, &sink).await;
        assert!(result.is_err(), "JDK 不存在时启动必须失败");

        // 核心断言：失败后状态绝不能是 Starting（否则 UI 永久卡死）
        let after = sb.find_app(&app.id).expect("应用应仍在列表中");
        assert_ne!(
            after.status,
            AppStatus::Starting,
            "启动失败后状态必须从 Starting 回滚，实际={:?}",
            after.status
        );
        assert_eq!(after.pid, None, "失败后不应残留 PID");
        assert!(
            after.last_error.as_deref().unwrap_or("").contains("JDK"),
            "失败原因应写入 last_error 供 UI 展示，实际={:?}",
            after.last_error
        );

        std::env::remove_var("OPX_APP_ROOT");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 复现：存量的「卡在 Starting 且 pid=null」应用在 list_apps 时必须自愈为 Stopped。
    ///
    /// 用户当前 apps.json 里 demo-jdk21 正是 status=Starting、pid=null。
    /// 旧兜底逻辑 `if let Some(pid)` 在 pid 为 null 时整段跳过，导致 UI 永久灰化，
    /// 不重启 opx 就无法恢复。本测试钉死自愈行为。
    #[test]
    fn list_apps_self_heals_stuck_starting_without_pid() {
        let _env_guard = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("opx-sb-stuck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建测试根目录");
        std::env::set_var("OPX_APP_ROOT", &root);

        // 直接落一份「卡在 Starting、无 pid」的存量数据
        let store_dir = root.join("data").join("springboot");
        std::fs::create_dir_all(&store_dir).expect("建 store 目录");
        let json = r#"{
            "applications": [{
                "id": "stuck-1", "name": "stuck", "jar_path": "springboot/stuck/app.jar",
                "version": "1.0", "jvm_opts": [], "program_args": [], "profile": "",
                "env_vars": [], "status": "Starting", "pid": null, "port": 8080,
                "log_path": "springboot/stuck/logs/console.log", "start_time": null,
                "last_error": null, "dependencies": [], "auto_start": false,
                "startup_order": 0, "auto_restart": false, "group": null,
                "jdk_type": "jre", "stop_timeout_secs": 30, "local_ip": "",
                "jdk_installed_id": "", "spring_boot_version": null
            }],
            "groups": [], "global_env_vars": []
        }"#;
        std::fs::write(store_dir.join("apps.json"), json).expect("写存量 apps.json");

        let sb = SpringBootManager::new();
        let apps = sb.list_apps();
        assert_eq!(apps.len(), 1, "应用不应丢失");
        assert_eq!(
            apps[0].status,
            AppStatus::Stopped,
            "卡在 Starting 且无 pid 的存量应用须自愈为 Stopped，实际={:?}",
            apps[0].status
        );

        std::env::remove_var("OPX_APP_ROOT");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 复现：apps.json 缺 `jdk_installed_id` 字段时仍须能反序列化。
    ///
    /// 用户为排查手动删掉该字段。若无 `#[serde(default)]`，整个 store
    /// 反序列化失败 → load_store().unwrap_or_default() → 应用列表被清空。
    #[test]
    fn apps_json_without_jdk_installed_id_deserializes() {
        let json = r#"{
            "applications": [{
                "id": "x1", "name": "demo", "jar_path": "springboot/demo/app.jar",
                "version": "1.0", "jvm_opts": [], "program_args": [], "profile": "",
                "env_vars": [], "status": "Stopped", "pid": null, "port": 8080,
                "log_path": "springboot/demo/logs/console.log", "start_time": null,
                "last_error": null, "dependencies": [], "auto_start": false,
                "startup_order": 0, "auto_restart": false, "group": null,
                "jdk_type": "jre", "stop_timeout_secs": 30, "local_ip": "",
                "spring_boot_version": null
            }],
            "groups": [], "global_env_vars": []
        }"#;
        let store: SpringBootStore = serde_json::from_str(json)
            .expect("缺 jdk_installed_id 字段的 apps.json 仍须能反序列化");
        assert_eq!(store.applications.len(), 1, "缺字段不应清空整个应用列表");
        assert_eq!(store.applications[0].jdk_installed_id, "", "缺失字段应回退为空串");
    }

    /// 回归：存量安装把官方包多解了一层（`<ver>/jdk-<ver>/bin/java`），
    /// 解析必须能兜底找到，否则用户不重装就永远起不来。
    #[test]
    fn resolve_java_bin_finds_nested_layout() {
        let exec = if cfg!(windows) { "java.exe" } else { "java" };
        let root = std::env::temp_dir().join(format!("opx_jdk_{}", uuid::Uuid::new_v4()));

        // 正常结构：<root>/bin/java
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin").join(exec), b"x").unwrap();
        assert_eq!(
            super::resolve_java_bin(&root).unwrap(),
            root.join("bin").join(exec)
        );

        // 多一层结构：<root>/jdk-25/bin/java
        std::fs::remove_dir_all(root.join("bin")).unwrap();
        let nested = root.join("jdk-25.0.5.1+2");
        std::fs::create_dir_all(nested.join("bin")).unwrap();
        std::fs::write(nested.join("bin").join(exec), b"x").unwrap();
        assert_eq!(
            super::resolve_java_bin(&root).unwrap(),
            nested.join("bin").join(exec)
        );

        // 找不到就返回 None（让上层给出明确报错，而不是静默用错路径）
        std::fs::remove_dir_all(&nested).unwrap();
        assert!(super::resolve_java_bin(&root).is_none());

        let _ = std::fs::remove_dir_all(&root);
    }
}
