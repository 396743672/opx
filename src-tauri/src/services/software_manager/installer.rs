use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

use crate::models::software::{
    CustomInstallParams, InstallParams, InstallSource, InstalledSoftware, SoftwareStatus,
};
use crate::models::software::ArchiveFormat;
use crate::services::software_manager::providers::{all_providers, InstallContext};
use crate::services::software_manager::SoftwareManager;
use crate::utils::{archive, download, paths};
use chrono::Utc;

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn cleanup_path(path: &Path) {
    if path.exists() {
        if let Err(e) = fs::remove_dir_all(path) {
            eprintln!("清理 {} 失败: {}", path.display(), e);
        }
    }
}

fn cleanup_file(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn emit_event(app: &AppHandle, payload: serde_json::Value) {
    let _ = app.emit("install-progress", payload);
}

/// 安装预置软件（在线镜像）
pub async fn install_software(
    app: AppHandle,
    manager: Arc<SoftwareManager>,
    params: InstallParams,
    install_id: String,
) {
    let tmp_path = paths::tmp_dir().join(format!("{}-{}.tmp", params.key, params.version));
    let install_path = paths::apps_dir()
        .join(&params.key)
        .join(&params.version);

    let catalog = manager.get_catalog();
    let entry = match catalog.entries.iter().find(|e| e.key == params.key) {
        Some(e) => e,
        None => {
            emit_event(
                &app,
                serde_json::json!({
                    "install_id": install_id,
                    "phase": "failed",
                    "error": format!("未知软件：{}", params.key),
                    "stage": "download"
                }),
            );
            return;
        }
    };
    let version_info = match entry.versions.iter().find(|v| v.version == params.version) {
        Some(v) => v,
        None => {
            emit_event(
                &app,
                serde_json::json!({
                    "install_id": install_id,
                    "phase": "failed",
                    "error": format!("{} 不支持版本 {}", entry.name, params.version),
                    "stage": "download"
                }),
            );
            return;
        }
    };
    if params.mirror_index >= version_info.mirrors.len() {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "镜像源选择无效",
                "stage": "download"
            }),
        );
        return;
    }
    let mirror = &version_info.mirrors[params.mirror_index];

    if manager.is_installed(&params.key, &params.version) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("{} {} 已安装", entry.name, params.version),
                "stage": "download"
            }),
        );
        return;
    }
    if manager.is_installing(&params.key, &params.version) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("{} {} 正在安装中", entry.name, params.version),
                "stage": "download"
            }),
        );
        return;
    }

    if let Err(e) = fs::create_dir_all(&install_path) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("创建安装目录失败: {}", e),
                "stage": "download"
            }),
        );
        return;
    }

    manager.add_install_task(
        install_id.clone(),
        params.key.clone(),
        params.version.clone(),
    );

    let result: Result<()> = async {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "downloading",
                "downloaded": 0,
                "total": serde_json::Value::Null,
                "percent": serde_json::Value::Null
            }),
        );

        let app_for_progress = app.clone();
        let install_id_for_progress = install_id.clone();
        download::download_with_progress(&mirror.url, &tmp_path, move |downloaded, total| {
            let percent = total.map(|t| (downloaded as f64 / t as f64 * 100.0) as i64);
            emit_event(
                &app_for_progress,
                serde_json::json!({
                    "install_id": install_id_for_progress.clone(),
                    "phase": "downloading",
                    "downloaded": downloaded,
                    "total": total,
                    "percent": percent
                }),
            );
        })
        .await?;

        if let Some(expected_sha) = &version_info.archive.sha256 {
            let computed = compute_sha256(&tmp_path)?;
            if computed != expected_sha.to_lowercase() {
                return Err(anyhow::anyhow!("SHA256 校验失败"));
            }
        }

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 0
            }),
        );

        match version_info.archive.format {
            ArchiveFormat::Zip => archive::extract_zip(&tmp_path, &install_path)?,
            ArchiveFormat::TarGz => archive::extract_tar_gz(&tmp_path, &install_path)?,
        }

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 50
            }),
        );

        if let Some(provider) = all_providers().into_iter().find(|p| p.key() == params.key) {
            let ctx = InstallContext::new(
                params.key.clone(),
                params.version.clone(),
                install_path.to_string_lossy().to_string(),
            );
            provider.post_install(&ctx)?;
        }

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 100
            }),
        );

        // 关键：在 move 进 InstalledSoftware 之前克隆 installed_id，
        // 后续的 jre_default 更新和 completed 事件需要使用这个 id。
        let installed_id = uuid::Uuid::new_v4().to_string();
        let installed_id_for_event = installed_id.clone();
        let now = Utc::now().naive_utc();
        let installed = InstalledSoftware {
            id: installed_id,
            key: params.key.clone(),
            version: params.version.clone(),
            name: format!("{} {}", entry.name, params.version),
            install_path: install_path.to_string_lossy().to_string(),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: mirror.name.clone(),
                url: mirror.url.clone(),
            },
        };
        manager.add_installed(installed)?;

        if params.key == "jre" && params.set_as_default_jre {
            manager.update_jre_default(Some(installed_id_for_event.clone()))?;
        }

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "completed",
                "installed_id": installed_id_for_event
            }),
        );

        Ok(())
    }
    .await;

    if let Err(e) = result {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "failed",
                "error": format!("{}", e),
                "stage": "download"
            }),
        );
        cleanup_file(&tmp_path);
        cleanup_path(&install_path);
    } else {
        cleanup_file(&tmp_path);
    }

    manager.remove_install_task(&install_id);
}

/// 安装用户上传的自定义压缩包
pub async fn install_custom(
    app: AppHandle,
    manager: Arc<SoftwareManager>,
    params: CustomInstallParams,
    install_id: String,
) {
    let name_trimmed = params.name.trim();
    let install_path = paths::apps_dir().join("custom").join(name_trimmed);
    let archive_path = Path::new(&params.archive_path);

    // 校验压缩包存在
    if !archive_path.exists() {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "压缩包不存在",
                "stage": "extract"
            }),
        );
        return;
    }

    // 校验扩展名
    let path_str = &params.archive_path;
    let is_zip = path_str.to_lowercase().ends_with(".zip");
    let is_tar_gz = path_str.to_lowercase().ends_with(".tar.gz")
        || path_str.to_lowercase().ends_with(".tgz");
    if !is_zip && !is_tar_gz {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "仅支持 .zip 和 .tar.gz 格式",
                "stage": "extract"
            }),
        );
        return;
    }

    // 校验名称非空（custom 的 name 存在 InstalledSoftware.key，version 存 name 用于查重）
    if name_trimmed.is_empty() {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "名称不能为空",
                "stage": "extract"
            }),
        );
        return;
    }

    // 查重：custom 用 key="custom" + version=name 查重
    if manager.is_installed("custom", name_trimmed) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("名称 {} 已存在", name_trimmed),
                "stage": "extract"
            }),
        );
        return;
    }

    if let Err(e) = fs::create_dir_all(&install_path) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("创建安装目录失败: {}", e),
                "stage": "extract"
            }),
        );
        return;
    }

    manager.add_install_task(
        install_id.clone(),
        "custom".to_string(),
        name_trimmed.to_string(),
    );

    let result: Result<()> = async {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 0
            }),
        );

        if is_zip {
            archive::extract_zip(archive_path, &install_path)?;
        } else {
            archive::extract_tar_gz(archive_path, &install_path)?;
        }

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 100
            }),
        );

        let installed_id = uuid::Uuid::new_v4().to_string();
        let installed_id_for_event = installed_id.clone();
        let now = Utc::now().naive_utc();
        let archive_name = archive_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let installed = InstalledSoftware {
            id: installed_id,
            key: "custom".to_string(),
            version: name_trimmed.to_string(),
            name: name_trimmed.to_string(),
            install_path: install_path.to_string_lossy().to_string(),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: true,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Custom { archive_name },
        };
        manager.add_installed(installed)?;

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "completed",
                "installed_id": installed_id_for_event
            }),
        );

        Ok(())
    }
    .await;

    if let Err(e) = result {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "failed",
                "error": format!("{}", e),
                "stage": "extract"
            }),
        );
        cleanup_path(&install_path);
    }

    manager.remove_install_task(&install_id);
}
