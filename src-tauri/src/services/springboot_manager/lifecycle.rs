use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter};

use crate::models::software::SoftwareStatus;
use crate::models::springboot::AppStatus;
use crate::services::process_registry;
use crate::services::springboot_manager::SpringBootManager;
use crate::services::software_manager::SoftwareManager;

/// app_id -> 进程注册表内部 id
static APP_REG_IDS: Lazy<Mutex<HashMap<String, i64>>> = Lazy::new(|| Mutex::new(HashMap::new()));

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

    // 确保日志目录存在
    if let Some(parent) = std::path::Path::new(&app.log_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // 构建命令
    let mut cmd = std::process::Command::new(&java_bin);
    for opt in &app.jvm_opts {
        cmd.arg(opt);
    }
    cmd.arg("-jar").arg(&app.jar_path);
    if let Some(p) = app.port {
        cmd.arg(format!("--server.port={}", p));
    }
    if !app.profile.is_empty() {
        cmd.arg(format!("--spring.profiles.active={}", app.profile));
    }
    for arg in &app.program_args {
        cmd.arg(arg);
    }
    for (k, v) in &app.env_vars {
        cmd.env(k, v);
    }

    // stdout/stderr -> 日志文件
    let log_file =
        std::fs::File::create(&app.log_path).map_err(|e| format!("无法创建日志文件: {}", e))?;
    let log_file_clone = log_file
        .try_clone()
        .map_err(|e| format!("无法克隆日志文件句柄: {}", e))?;
    cmd.stdout(std::process::Stdio::from(log_file));
    cmd.stderr(std::process::Stdio::from(log_file_clone));

    let child = cmd.spawn().map_err(|e| format!("启动失败: {}", e))?;
    let pid = child.id();

    // 注册到全局进程注册表
    let reg_id = process_registry::register(pid, app.name.clone(), "springboot".to_string());
    APP_REG_IDS.lock().unwrap().insert(app_id.to_string(), reg_id);

    // ponytail: 健康检查超时由 health_check_timeout_secs 控制
    let max_attempts = app.health_check_timeout_secs.max(5);
    let mut healthy = app.port.is_none(); // 无端口时直接认为健康
    for _ in 0..max_attempts {
        if let Some(port) = app.port {
            if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                .await
                .is_ok()
            {
                healthy = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    if healthy {
        springboot_mgr
            .update_status(app_id, AppStatus::Running, Some(pid), None)
            .map_err(|e| e.to_string())?;
        let _ = app_handle.emit(
            "springboot-status-changed",
            (app_id.to_string(), "Running", Some(pid), None::<String>),
        );
        Ok(())
    } else {
        springboot_mgr
            .update_status(
                app_id,
                AppStatus::Error,
                Some(pid),
                Some("健康检查超时".to_string()),
            )
            .map_err(|e| e.to_string())?;
        let _ = app_handle.emit(
            "springboot-status-changed",
            (
                app_id.to_string(),
                "Error",
                Some(pid),
                Some("健康检查超时".to_string()),
            ),
        );
        Err("应用启动但健康检查超时（30s），请检查日志和端口配置".to_string())
    }
}

/// 停止 Spring Boot 应用
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

    if let Some(pid) = app.pid {
        if cfg!(windows) {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        } else {
            // SIGTERM
            let _ = std::process::Command::new("kill")
                .args([&pid.to_string()])
                .output();
            // 等待 10s 让进程优雅退出
            tokio::time::sleep(Duration::from_secs(10)).await;
            // 检查是否仍在运行，若是则 SIGKILL
            let alive = std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if alive {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
        }
    }

    // 从进程注册表中注销
    if let Some(reg_id) = APP_REG_IDS.lock().unwrap().remove(app_id) {
        process_registry::unregister(reg_id);
    }

    springboot_mgr
        .update_status(app_id, AppStatus::Stopped, None, None)
        .map_err(|e| e.to_string())?;
    let _ = app_handle.emit(
        "springboot-status-changed",
        (app_id.to_string(), "Stopped", None::<u32>, None::<String>),
    );
    Ok(())
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
