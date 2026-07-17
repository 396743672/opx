use std::time::Duration;

use sysinfo::{Pid, System, ProcessesToUpdate};
use tauri::{AppHandle, Emitter};

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::SoftwareManager;

// ponytail: sysinfo 判活，跨平台，不依赖 tasklist 输出编码
pub(crate) fn is_pid_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All);
    sys.process(Pid::from_u32(pid)).is_some()
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
    let mut cmd = std::process::Command::new(&java_bin);
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
    // ponytail: 日志检测启动完成（Started…），不等固定超时；PID 死则秒报
    let mut started = app.port.is_none();
    for _ in 0..300 {
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
        // 检测日志中 Spring Boot 启动完成标记
        if !started && console_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&console_path) {
                if content.contains("Started ") {
                    started = true;
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
        tokio::time::sleep(Duration::from_secs(1)).await;
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

/// 停止 Spring Boot 应用
pub async fn stop_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
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
        // ponytail: 优先用 jcmd 发 Shutdown 信号（优雅），走 /F 兜底
        let jcmd_path = software_mgr
            .find_installed(&app.jdk_installed_id)
            .map(|jdk| std::path::PathBuf::from(&jdk.install_path).join("bin").join("jcmd.exe"));
        if let Some(ref jcmd) = jcmd_path {
            let _ = std::process::Command::new(jcmd)
                .args([&pid.to_string(), "Shutdown"])
                .output();
        }
        // 兜底：taskkill /PID（WM_CLOSE 信号）
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .output();

        // ponytail: 等待 10s 让进程退出，用 sysinfo 判活（tasklist 中文 Windows 编码不稳）
        let mut exited = false;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if !is_pid_alive(pid) { exited = true; break; }
        }
        if !exited {
            // 10s 未退出 → 强制终止
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output();
            }
            #[cfg(not(windows))]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
            force_msg = Some(format!("应用 PID {} 在 10s 内未优雅退出，已强制终止", pid));
        }
    }

    // 从进程注册表中注销
    lifecycle::unregister(&app_id);

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

/// 重启 Spring Boot 应用
pub async fn restart_app(
    app_id: &str,
    springboot_mgr: &SpringBootManager,
    software_mgr: &SoftwareManager,
    app_handle: &AppHandle,
) -> Result<(), String> {
    stop_app(app_id, springboot_mgr, software_mgr, app_handle).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    start_app(app_id, springboot_mgr, software_mgr, app_handle).await
}
