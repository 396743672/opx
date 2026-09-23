use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::SoftwareManager;
use crate::utils::process::hidden;

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

/// 停止 Spring Boot 应用。
///
/// Windows 上没有 SIGTERM，**HTTP 是唯一能让 JVM 走完 shutdown hook 的通道**：
/// - `jcmd <pid> Shutdown` 这个命令**不存在**（JDK 8 与 21 实测均报
///   `java.lang.IllegalArgumentException: Unknown diagnostic command`，进程照旧存活）
/// - `taskkill /PID`（不带 `/F`）发的是 WM_CLOSE，**只对进程自己拥有的窗口有效**；
///   控制台程序的窗口属于 conhost.exe，实测被直接拒绝（「只能强行终止这个进程(带 /F 选项)」）
/// - Ctrl+C / CTRL_BREAK 全走控制台通道，而 opx 用 `CREATE_NO_WINDOW` 启动子进程（防闪黑窗）
///   → 子进程没有控制台，收不到
///
/// 故顺序为：POST 应用的 Actuator 停止端点 → 等它自行退出（上限 `stop_timeout_secs`）
/// → 仍未退出才强制终止，并如实说明是「自行退出」还是「强制终止」。
pub async fn stop_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    app_handle: &AppHandle,
) -> Result<(), String> {
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

    let mut force_msg = None;
    if let Some(pid) = app.pid {
        // ① 发出优雅停止请求。两条通道按平台分工：
        //    Unix —— SIGTERM 是 JVM 能真正响应的信号，Spring Boot 的优雅停机就建立在其上
        //    Windows —— 没有 SIGTERM，HTTP 是唯一通道
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

        let shutdown_accepted =
            match resolve_actuator_url(app.actuator_shutdown_url.as_deref(), app.port) {
                Some(url) => match request_shutdown(&url).await {
                    Ok(()) => {
                        tracing::info!(app_id, pid, %url, "已请求 Actuator 优雅停止");
                        true
                    }
                    // 多数应用没开这个端点，属预期情况
                    Err(why) => {
                        tracing::info!(app_id, pid, %url, why = %why, "Actuator 优雅停止不可用");
                        false
                    }
                },
                None => {
                    tracing::info!(app_id, pid, "未配置端口与 Actuator 地址，跳过 HTTP 优雅停止");
                    false
                }
            };

        // ② 等它自己退——**只在确实发出过它能响应的请求时才等**。
        //    没有任何通道时进程不可能自行退出，等满超时纯属空耗
        //    （Windows 上没暴露端点的应用就是这种情况，干等只会让停止显得变慢）。
        let has_channel = term_sent || shutdown_accepted;
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
            force_msg = Some(if has_channel {
                format!(
                    "应用 PID {pid} 在 {timeout_secs}s 内未退出，已强制终止（本次未执行 shutdown hook）"
                )
            } else {
                format!(
                    "应用 PID {pid} 未提供优雅停止通道，已直接强制终止（未执行 shutdown hook）"
                )
            });
            tracing::warn!(app_id, pid, timeout_secs, has_channel, "应用未自行退出，已强制终止");
        }
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

    if let Some(msg) = force_msg {
        Err(msg)
    } else {
        Ok(())
    }
}

/// 推导优雅停止地址：显式配置优先，否则按应用端口拼 Actuator 默认路径。
///
/// 返回 `None` 表示无从请求（既没配地址也没填端口），此时只能等待 + 强杀。
fn resolve_actuator_url(configured: Option<&str>, port: Option<u16>) -> Option<String> {
    let cfg = configured.map(str::trim).filter(|s| !s.is_empty());
    match cfg {
        Some(url) => Some(url.to_string()),
        None => port.map(|p| format!("http://127.0.0.1:{p}/actuator/shutdown")),
    }
}

/// POST 应用的 Actuator 停止端点。
///
/// 这里是小请求，用总超时是合适的（那种会掐断大文件下载的语义问题只存在于流式下载）。
/// 失败原因原样回传，便于区分「没开端点」「端口不通」「认证被拒」。
///
/// 应用侧需要**同时**满足「暴露」与「允许访问」，且属性名随大版本不同
/// （默认都是关闭的，所以 404 是最常见的失败）：
/// - 2.x / 3.x：`management.endpoint.shutdown.enabled=true`
/// - 4.x：改用 `management.endpoint.shutdown.access=unrestricted`（`enabled` 已废）
/// - 两版都要把端点加入 `management.endpoints.web.exposure.include`（默认只暴露 health）
async fn request_shutdown(url: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败: {e}"))?;
    let resp = client
        .post(url)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else if status.as_u16() == 404 {
        Err("端点不存在（应用未暴露 shutdown 端点，该项默认关闭）".to_string())
    } else if status.as_u16() == 401 || status.as_u16() == 403 {
        Err(format!("认证被拒（HTTP {status}），Actuator 开启了鉴权"))
    } else {
        Err(format!("返回 HTTP {status}"))
    }
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

/// 重启 Spring Boot 应用
pub async fn restart_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    app_handle: &AppHandle,
) -> Result<(), String> {
    stop_app(app_id, springboot_mgr, app_handle).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    start_app(app_id, springboot_mgr, software_mgr, app_handle).await
}

#[cfg(test)]
mod tests {
    use super::{request_shutdown, resolve_actuator_url};

    /// 未显式配置时，按应用端口拼 Actuator 的默认停止路径
    #[test]
    fn actuator_url_falls_back_to_port() {
        assert_eq!(
            resolve_actuator_url(None, Some(8080)).as_deref(),
            Some("http://127.0.0.1:8080/actuator/shutdown")
        );
    }

    /// 显式配置优先于端口推导（context-path 或 management 独立端口的情形）
    #[test]
    fn actuator_url_prefers_explicit_config() {
        assert_eq!(
            resolve_actuator_url(Some("http://127.0.0.1:9000/base/actuator/shutdown"), Some(8080))
                .as_deref(),
            Some("http://127.0.0.1:9000/base/actuator/shutdown")
        );
    }

    /// 空白配置视同未配置；无端口则无从请求（调用方会跳过 HTTP 优雅停止）
    #[test]
    fn actuator_url_handles_blank_and_missing_port() {
        assert_eq!(
            resolve_actuator_url(Some("   "), Some(8080)).as_deref(),
            Some("http://127.0.0.1:8080/actuator/shutdown")
        );
        assert_eq!(resolve_actuator_url(None, None), None);
        assert_eq!(resolve_actuator_url(Some(""), None), None);
    }

    /// 停止请求必须是 POST（Actuator 的 shutdown 端点只接受 POST），
    /// 且「端点没开」要能被识别出来——否则用户只会看到「强制终止」而不知原因。
    #[tokio::test]
    async fn request_shutdown_posts_and_reports_missing_endpoint() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
        let addr = listener.local_addr().expect("取本地地址");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接");
            let mut buf = [0u8; 512];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let request_line = req.lines().next().unwrap_or_default().to_string();
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
            let _ = stream.flush();
            request_line
        });

        let url = format!("http://{addr}/actuator/shutdown");
        let err = request_shutdown(&url).await.expect_err("404 应视为失败");
        let request_line = server.join().expect("服务端线程");

        assert!(
            request_line.starts_with("POST /actuator/shutdown"),
            "实际请求行: {request_line}"
        );
        assert!(
            err.contains("端点不存在"),
            "错误信息应指出端点未开启，实际: {err}"
        );
    }

    /// 2xx 视为已受理：这条分支判错会让「优雅停止」永远失效、每次都退化成强杀
    #[tokio::test]
    async fn request_shutdown_accepts_2xx() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
        let addr = listener.local_addr().expect("取本地地址");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("接受连接");
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
            let _ = stream.flush();
        });

        let url = format!("http://{addr}/actuator/shutdown");
        assert!(request_shutdown(&url).await.is_ok());
    }
}
