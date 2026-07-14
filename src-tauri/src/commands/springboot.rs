use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::models::springboot::{
    AppGroup, CreateAppParams, JvmInfo, JvmOptsTemplate, ReplaceResult, SpringBootApp,
    UpdateAppParams,
};
use crate::services::software_manager::SoftwareManager;
use crate::services::springboot_manager::jvm_opts;
use crate::services::springboot_manager::SpringBootManager;

#[tauri::command]
pub async fn list_springboot_apps(
    manager: State<'_, Arc<SpringBootManager>>,
) -> Result<Vec<SpringBootApp>, String> {
    Ok(manager.list_apps())
}

#[tauri::command]
pub async fn create_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    params: CreateAppParams,
) -> Result<SpringBootApp, String> {
    // ponytail: 有指定端口才查重，None 表示动态端口不校验
    if let Some(port) = params.port {
        let apps = manager.list_apps();
        if apps.iter().any(|a| a.port == Some(port)) {
            return Err(format!("端口 {} 已被其他应用占用", port));
        }
    }
    manager.create_app(params).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    params: UpdateAppParams,
) -> Result<SpringBootApp, String> {
    manager.update_app(&id, params).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
) -> Result<(), String> {
    manager.delete_app(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
}

#[tauri::command]
pub async fn stop_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    crate::services::springboot_manager::lifecycle::stop_app(
        &id, &manager, &app_handle,
    ).await
}

#[tauri::command]
pub async fn restart_springboot_app(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    crate::services::springboot_manager::lifecycle::restart_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
}

#[tauri::command]
pub async fn replace_springboot_jar(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    use chrono::Local;
    use std::path::Path;

    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    if app.status == crate::models::springboot::AppStatus::Running {
        return Err("运行中的应用不可换包".to_string());
    }

    let new_path = Path::new(&new_jar_path);
    if !new_path.exists() {
        return Err("新 JAR 文件不存在".to_string());
    }

    let old_jar = Path::new(&app.jar_path);
    if !old_jar.exists() {
        return Err("原 JAR 文件不存在".to_string());
    }

    // 备份：{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak
    let backup_dir = crate::utils::paths::data_dir()
        .join("backups")
        .join(&app.name);
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let fname = old_jar.file_name().unwrap().to_str().unwrap();
    let backup_path = backup_dir.join(format!("{}.{}.bak", fname, timestamp));

    std::fs::copy(old_jar, &backup_path).map_err(|e| format!("备份失败: {}", e))?;

    // 替换
    std::fs::copy(new_path, old_jar).map_err(|e| format!("替换 JAR 失败: {}", e))?;

    // 读取新版本
    let new_version = crate::services::springboot_manager::read_jar_version(new_path.to_str().unwrap())
        .unwrap_or_else(|| "unknown".to_string());
    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;

    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap().to_string(),
        old_version: app.version,
        new_version,
    })
}

#[tauri::command]
pub async fn get_springboot_jvm_metrics(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
) -> Result<Option<JvmInfo>, String> {
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    if let Some(pid) = app.pid {
        Ok(crate::services::springboot_manager::monitor::collect_jvm_metrics(pid))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub async fn list_springboot_groups(
    manager: State<'_, Arc<SpringBootManager>>,
) -> Result<Vec<AppGroup>, String> {
    Ok(manager.list_groups())
}

#[tauri::command]
pub async fn save_springboot_groups(
    manager: State<'_, Arc<SpringBootManager>>,
    groups: Vec<AppGroup>,
) -> Result<(), String> {
    manager.save_groups(groups).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_recommended_jvm_opts(
    jdk_installed_id: String,
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<JvmOptsTemplate, String> {
    let jdk = software_mgr
        .find_installed(&jdk_installed_id)
        .ok_or("所选 JDK 未找到")?;
    let version = jvm_opts::detect_jdk_version(&jdk.install_path)
        .ok_or("无法检测 JDK 版本")?;
    Ok(jvm_opts::generate_opts(version))
}

#[tauri::command]
pub async fn list_springboot_dependency_candidates(
    software_mgr: State<'_, Arc<SoftwareManager>>,
) -> Result<Vec<crate::models::software::InstalledSoftware>, String> {
    Ok(crate::services::springboot_manager::deps::list_dependency_candidates(&software_mgr))
}

#[tauri::command]
pub async fn read_jar_version_info(
    jar_path: String,
) -> Result<String, String> {
    Ok(crate::services::springboot_manager::read_jar_version(&jar_path)
        .unwrap_or_else(|| "unknown".to_string()))
}

#[tauri::command]
pub async fn read_jar_port(
    jar_path: String,
) -> Result<Option<u16>, String> {
    Ok(crate::services::springboot_manager::read_port_from_jar(&jar_path))
}

/// ponytail: 后端读日志，绕过 fs 插件路径限制
#[tauri::command]
pub async fn read_springboot_log(path: String) -> Result<Vec<String>, String> {
    if !std::path::Path::new(&path).exists() {
        return Ok(vec!["日志文件尚未生成，请先启动应用".to_string()]);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取日志失败: {}", e))?;
    let all: Vec<&str> = content.lines().collect();
    let tail = all.len().saturating_sub(500);
    let lines: Vec<String> = all[tail..].iter().map(|s| s.to_string()).collect();
    Ok(lines)
}
