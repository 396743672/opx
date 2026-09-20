use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::models::software::ArchiveFormat;
use crate::models::software::{
    CatalogEntry, CatalogVersion, CustomInstallParams, InstallParams, InstallSource,
    InstalledSoftware, MirrorSource, SoftwareStatus,
};
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

/// install_id → (action, target, detail)：供 emit_event 在终态补写审计完成记录。
/// ponytail: 命令侧与 emit_event 之间隔着 50 余处调用点，逐点透传审计上下文改动过大；
/// 用一张按 install_id 索引的小表收口，终态事件一次性 take 后即释放。
static INSTALL_AUDIT_CTX: LazyLock<Mutex<HashMap<String, (String, String, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 命令发起安装任务时登记审计上下文（与 oplog_begin! 同处调用）。
pub fn register_install_audit(install_id: &str, action: &str, target: &str, detail: &str) {
    INSTALL_AUDIT_CTX
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            install_id.to_string(),
            (action.to_string(), target.to_string(), detail.to_string()),
        );
}

fn emit_event(app: &AppHandle, payload: serde_json::Value) {
    // 终态（failed / completed）顺带补写审计完成记录：这是安装任务唯一的完成出口，
    // 覆盖 install_software / install_custom / install_from_builtin 的全部失败分支。
    let phase = payload.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    if phase == "failed" || phase == "completed" {
        if let Some(id) = payload.get("install_id").and_then(|v| v.as_str()) {
            let ctx = INSTALL_AUDIT_CTX
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id); // take：同一 install_id 只落一条完成记录
            if let Some((action, target, detail)) = ctx {
                let (result, error) = if phase == "completed" {
                    (
                        crate::services::software_manager::audit::RESULT_OK,
                        String::new(),
                    )
                } else {
                    (
                        crate::services::software_manager::audit::RESULT_FAIL,
                        payload
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    )
                };
                crate::services::software_manager::audit::record_full(
                    action, target, detail, result, error,
                );
            }
        }
    }
    let _ = app.emit("install-progress", payload);
}

// ponytail: 节流 emit，避免大文件每 chunk 刷屏 IPC
struct ThrottledEmitter {
    last_emit: std::time::Instant,
}

impl ThrottledEmitter {
    fn new() -> Self {
        Self {
            last_emit: std::time::Instant::now(),
        }
    }

    fn should_emit(&mut self, _percent: i64) -> bool {
        // ponytail: 纯 timeout 节流——最多每 200ms emit 一次。
        // 不因 percent 变化而额外触发：解压大 zip 时每文件 percent 都变，
        // 若按变化 emit 会刷屏 IPC 卡死前端（见 BUG2 修复）。
        let timeout = self.last_emit.elapsed().as_millis() >= 200;
        if timeout {
            self.last_emit = std::time::Instant::now();
            true
        } else {
            false
        }
    }
}

/// 校验自定义软件名称：仅允许字母、数字、下划线、连字符
fn is_valid_custom_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 安装预置软件（在线镜像）
pub async fn install_software(
    app: AppHandle,
    manager: Arc<SoftwareManager>,
    params: InstallParams,
    install_id: String,
) {
    let install_path = paths::apps_dir().join(&params.key).join(&params.version);

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
    let selected_mirror = &version_info.mirrors[params.mirror_index];

    // builtin 分流：若选中的镜像带 builtin 标记，走本地解压
    if selected_mirror.builtin.is_some() {
        install_from_builtin(
            app,
            manager,
            params,
            install_id,
            entry,
            version_info,
            selected_mirror,
        )
        .await;
        return;
    }

    // 查重
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
        // 下载到缓存→SHA 校验→解压到 install_path→provider.post_install（含进度事件）
        // 多源回退：选中源不可达时自动尝试其余可联网镜像（自持源 → 官方源），
        // 避免「只试一次就整体失败、排序在后的兜底源永远用不上」。
        let used_mirror = download_with_mirror_fallback(
            &params,
            &install_path,
            &app,
            &install_id,
            version_info,
            params.mirror_index,
        )
        .await?;

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
            install_path: format!("{}/{}", params.key, params.version),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: used_mirror.name.clone(),
                url: used_mirror.url.clone(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
            icon: String::new(),
            category: None,
            depends_on: vec![],
            auto_restart: false,
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
        eprintln!(
            "[software] install failed: key={}, version={}, error={}",
            params.key, params.version, e
        );
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "failed",
                "error": format!("{}", e),
                "stage": "download"
            }),
        );
        cleanup_path(&install_path);
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
    let is_tar_gz =
        path_str.to_lowercase().ends_with(".tar.gz") || path_str.to_lowercase().ends_with(".tgz");
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

    // 校验名称字符集（防止路径遍历：仅允许字母、数字、下划线、连字符）
    if !is_valid_custom_name(name_trimmed) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "名称仅允许字母、数字、下划线、连字符",
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

        let app_ep = app.clone();
        let id_ep = install_id.clone();
        let mut throttle = ThrottledEmitter::new();
        let on_progress = move |extracted: u64, total: u64| {
            let percent = if total > 0 {
                (extracted as f64 / total as f64 * 100.0) as i64
            } else {
                0
            };
            if throttle.should_emit(percent) {
                emit_event(
                    &app_ep,
                    serde_json::json!({
                        "install_id": id_ep.clone(),
                        "phase": "extracting",
                        "percent": percent
                    }),
                );
            }
        };

        if is_zip {
            archive::extract_zip(archive_path, &install_path, on_progress)?;
        } else {
            archive::extract_tar_gz(archive_path, &install_path, on_progress)?;
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
            install_path: format!("custom/{}", name_trimmed),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: true,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Custom { archive_name },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
            icon: String::new(),
            category: None,
            depends_on: vec![],
            auto_restart: false,
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
        eprintln!(
            "[software] custom install failed: name={}, error={}",
            name_trimmed, e
        );
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

/// 下载到缓存→SHA 校验→解压到 install_path→provider.post_install（含进度事件）。
/// `install_software` / `upgrade_software` 共用；不含 add_installed（记录合并由调用方负责）。
pub async fn download_and_extract(
    params: &InstallParams,
    install_path: &Path,
    app: &AppHandle,
    install_id: &str,
    version_info: &CatalogVersion,
    mirror: &MirrorSource,
) -> Result<(), anyhow::Error> {
    // 缓存目录：cache/{key}/（持久保留，复用避免重复下载）
    let cache_key_dir = paths::cache_dir().join(&params.key);
    fs::create_dir_all(&cache_key_dir)?;

    // 缓存文件名按归档格式派生：
    // - Zip → {version}.zip；TarGz → {version}.tar.gz
    // - Executable → 取 URL 文件名（如 minio.exe），避免存成误导性的 .zip
    let cache_file_name = match version_info.archive.format {
        ArchiveFormat::Zip => format!("{}.zip", params.version),
        ArchiveFormat::TarGz => format!("{}.tar.gz", params.version),
        ArchiveFormat::Executable => mirror
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("app.exe")
            .to_string(),
    };
    let cache_path = cache_key_dir.join(&cache_file_name);

    // 检查缓存：若 cache_path 存在则跳过下载
    let cache_hit = cache_path.exists();
    if cache_hit {
        emit_event(
            app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "downloading",
                "downloaded": 0,
                "total": serde_json::Value::Null,
                "percent": 100,
                "cached": true
            }),
        );
    } else {
        emit_event(
            app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "downloading",
                "downloaded": 0,
                "total": serde_json::Value::Null,
                "percent": serde_json::Value::Null
            }),
        );

        let app_for_progress = app.clone();
        let install_id_for_progress = install_id.to_string();
        // 节流：大文件按每 chunk 回调会刷屏 IPC，限制为最多每 200ms 或百分比变化时 emit 一次
        let mut last_emit = std::time::Instant::now();
        let mut last_percent: i64 = -1;
        if let Err(e) =
            download::download_with_progress(&mirror.url, &cache_path, move |downloaded, total| {
                let percent = total.map(|t| (downloaded as f64 / t as f64 * 100.0) as i64);
                let percent_changed = percent.map(|p| p != last_percent).unwrap_or(false);
                if last_emit.elapsed().as_millis() >= 200 || percent_changed {
                    last_emit = std::time::Instant::now();
                    if let Some(p) = percent {
                        last_percent = p;
                    }
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
                }
            })
            .await
        {
            // 下载失败：删除不完整的缓存文件，避免下次误当命中复用
            let _ = fs::remove_file(&cache_path);
            return Err(e);
        }
    }

    if let Some(expected_sha) = &version_info.archive.sha256 {
        let computed = compute_sha256(&cache_path)?;
        if computed != expected_sha.to_lowercase() {
            // 校验失败：缓存可能损坏，删除缓存让下次重新下载
            let _ = fs::remove_file(&cache_path);
            return Err(anyhow::anyhow!("SHA256 校验失败"));
        }
    }

    emit_event(
        app,
        serde_json::json!({
            "install_id": install_id,
            "phase": "extracting",
            "percent": 0
        }),
    );

    let app_ep = app.clone();
    let id_ep = install_id.to_string();
    let cache_path2 = cache_path.clone();
    let install_path2 = install_path.to_path_buf();
    let fmt = version_info.archive.format.clone();
    let blocking_result = tokio::task::spawn_blocking(move || {
        let mut throttle = ThrottledEmitter::new();
        let on_progress = move |extracted: u64, total: u64| {
            let percent = if total > 0 {
                (extracted as f64 / total as f64 * 100.0) as i64
            } else {
                0
            };
            if throttle.should_emit(percent) {
                emit_event(
                    &app_ep,
                    serde_json::json!({
                        "install_id": id_ep.clone(),
                        "phase": "extracting",
                        "percent": percent
                    }),
                );
            }
        };

        match fmt {
            ArchiveFormat::Zip => {
                archive::extract_zip_flatten(&cache_path2, &install_path2, on_progress)
            }
            ArchiveFormat::TarGz => {
                archive::extract_tar_gz(&cache_path2, &install_path2, on_progress)
            }
            ArchiveFormat::Executable => {
                let dest_file = install_path2.join(
                    cache_path2
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("app.exe")),
                );
                fs::copy(&cache_path2, &dest_file)
                    .map(|_| ())
                    .map_err(Into::into)
            }
        }
    })
    .await;
    let inner = blocking_result.map_err(|e| anyhow::anyhow!("解压线程异常: {}", e))?;
    inner?;

    emit_event(
        app,
        serde_json::json!({
            "install_id": install_id,
            "phase": "extracting",
            "percent": 100
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
        app,
        serde_json::json!({
            "install_id": install_id,
            "phase": "extracting",
            "percent": 100
        }),
    );

    Ok(())
}

/// 候选镜像排序：`preferred_index` 指定的镜像优先，其余按目录声明顺序补齐。
///
/// 只保留可联网下载的镜像（`builtin` 镜像没有真实地址，本地资源分支由
/// `install_from_builtin` 负责）。`sort_by_key` 是稳定排序，故「其余」保持声明顺序，
/// 回退次序可预期。
fn mirror_candidates(version_info: &CatalogVersion, preferred_index: usize) -> Vec<&MirrorSource> {
    let mut candidates: Vec<(usize, &MirrorSource)> = version_info
        .mirrors
        .iter()
        .enumerate()
        .filter(|(_, m)| m.builtin.is_none() && m.url.starts_with("http"))
        .collect();
    // 首选源排最前；其余同权，稳定排序保持声明顺序
    candidates.sort_by_key(|(i, _)| usize::from(*i != preferred_index));
    candidates.into_iter().map(|(_, m)| m).collect()
}

/// 按候选顺序依次尝试镜像，返回首个「下载 + SHA 校验 + 解压」全链路成功的镜像。
///
/// 中间失败只记日志、**不 emit `failed`**：emit_event 会在 failed 终态消费
/// `INSTALL_AUDIT_CTX` 中的审计上下文并写入失败记录，导致后续成功也无审计可写。
/// 最终失败由调用方统一上报。
pub async fn download_with_mirror_fallback(
    params: &InstallParams,
    install_path: &Path,
    app: &AppHandle,
    install_id: &str,
    version_info: &CatalogVersion,
    preferred_index: usize,
) -> Result<MirrorSource> {
    let candidates = mirror_candidates(version_info, preferred_index);
    if candidates.is_empty() {
        return Err(anyhow::anyhow!(
            "{} {} 无可用镜像源",
            params.key,
            params.version
        ));
    }
    let total = candidates.len();

    let mut last_err: Option<anyhow::Error> = None;
    for (attempt, mirror) in candidates.into_iter().enumerate() {
        if attempt > 0 {
            // 上一源若在解压阶段失败，install_path 会留下半成品；先清空再试，避免新旧内容叠加。
            // Executable 格式需要目录存在才能 copy，故清空后补建。
            cleanup_path(install_path);
            let _ = fs::create_dir_all(install_path);
            eprintln!(
                "[software] mirror fallback -> {} ({})",
                mirror.name, mirror.url
            );
        }

        match download_and_extract(params, install_path, app, install_id, version_info, mirror).await
        {
            Ok(()) => {
                if attempt > 0 {
                    eprintln!(
                        "[software] mirror fallback succeeded: key={}, version={}, mirror={}",
                        params.key, params.version, mirror.name
                    );
                }
                return Ok(mirror.clone());
            }
            Err(e) => {
                eprintln!(
                    "[software] mirror failed ({}/{}): {} ({}) -> {}",
                    attempt + 1,
                    total,
                    mirror.name,
                    mirror.url,
                    e
                );
                last_err = Some(e);
            }
        }
    }

    // candidates 非空已在上方保证，循环必然留下错误
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("所有镜像源均失败")))
}

/// 从内置 zip 安装（离线安装）
async fn install_from_builtin(
    app: AppHandle,
    manager: Arc<SoftwareManager>,
    params: InstallParams,
    install_id: String,
    entry: &CatalogEntry,
    version_info: &CatalogVersion,
    mirror: &MirrorSource,
) {
    let builtin = match &mirror.builtin {
        Some(b) => b,
        None => return,
    };
    let install_path = paths::apps_dir().join(&params.key).join(&params.version);

    // 1. 解析本地 resource 路径（离线优先）
    // Windows 上 resource_dir() 返回 exe 目录，资源实际在 resources/ 子目录下；
    // macOS 上 resource_dir() 已是 .app/Contents/Resources/，资源直接在其下。
    // resolve_builtin_resource 会尝试两个候选路径并返回第一个存在的；不存在则返回 None。
    // resource_zip 改为 Option：本地存在时为 Some(路径)，缺失时为 None（交由下方回退下载）。
    let resource_zip: Option<std::path::PathBuf> = match app.path().resource_dir() {
        Ok(d) => {
            let rel = format!("software/{}/{}.zip", &params.key, &params.version);
            crate::utils::paths::resolve_builtin_resource(&d, &rel)
        }
        Err(e) => {
            emit_event(
                &app,
                serde_json::json!({
                    "install_id": install_id,
                    "phase": "failed",
                    "error": format!("无法定位资源目录: {}", e),
                    "stage": "extract"
                }),
            );
            return;
        }
    };

    // 本地缺失时的回退下载源：取版本内 builtin:None 的 http(s) mirror（如官网 CDN）。
    // 仅 MySQL 等超过代码托管单文件体积限制的内置软件会配置此回退源；
    // 其余内置软件（jre/redis 等）包体已入库，本地必定存在，不会走到此分支。
    let download_url = version_info
        .mirrors
        .iter()
        .find(|m| m.builtin.is_none() && m.url.starts_with("http"))
        .map(|m| m.url.clone());

    // 2. 准备临时副本 temp_zip（后续 sha256 校验与解压均操作它，原始资源不受影响）。
    //    来源优先级：本地存在 → 直接拷贝到临时目录；本地缺失且有回退源 → 联网下载到临时目录；
    //    两者皆无 → 清晰报错，提示手动放置安装包或检查网络。
    let tmp_dir = paths::tmp_dir();
    let temp_zip = tmp_dir.join(format!("builtin_{}_{}.zip", &params.key, &params.version));
    let cleanup_temp = |p: &std::path::Path| {
        let _ = fs::remove_file(p);
    };

    let local_exists = resource_zip.as_ref().map(|p| p.exists()).unwrap_or(false);
    // downloaded 标记：仅当安装包来自「联网下载」时为 true，用于下方 sha256 校验判定。
    // 本地内置资源（builtin）可能经过人工修改（如重命名目录），其 sha256 与官网原版不一致，
    // 故本地来源不校验；下载来源必须校验官方 sha256，防止损坏/篡改。
    let mut downloaded = false;
    if !local_exists {
        match &download_url {
            Some(url) => {
                // 进入 downloading 阶段，联网拉取官方安装包
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
                let app_ep = app.clone();
                let id_ep = install_id.clone();
                let mut last_emit = std::time::Instant::now();
                let mut last_percent: i64 = -1;
                if let Err(e) =
                    download::download_with_progress(url, &temp_zip, move |downloaded, total| {
                        let percent = total.map(|t| (downloaded as f64 / t as f64 * 100.0) as i64);
                        let percent_changed = percent.map(|p| p != last_percent).unwrap_or(false);
                        if last_emit.elapsed().as_millis() >= 200 || percent_changed {
                            last_emit = std::time::Instant::now();
                            if let Some(p) = percent {
                                last_percent = p;
                            }
                            emit_event(
                                &app_ep,
                                serde_json::json!({
                                    "install_id": id_ep.clone(),
                                    "phase": "downloading",
                                    "downloaded": downloaded,
                                    "total": total,
                                    "percent": percent
                                }),
                            );
                        }
                    })
                    .await
                {
                    cleanup_temp(&temp_zip);
                    emit_event(
                        &app,
                        serde_json::json!({
                            "install_id": install_id,
                            "phase": "failed",
                            "error": format!("下载安装包失败: {}", e),
                            "stage": "download"
                        }),
                    );
                    return;
                }
                downloaded = true;
            }
            None => {
                cleanup_temp(&temp_zip);
                emit_event(
                    &app,
                    serde_json::json!({
                        "install_id": install_id,
                        "phase": "failed",
                        "error": format!(
                            "内置安装包缺失，请将 {}.zip 放到 resources/software/{}/ 后重试，或检查网络连接",
                            &params.version, &params.key
                        ),
                        "stage": "extract"
                    }),
                );
                return;
            }
        }
    } else {
        // 本地存在：拷贝到临时目录保护原始资源
        if let Err(e) = fs::copy(resource_zip.as_ref().unwrap(), &temp_zip) {
            cleanup_temp(&temp_zip);
            emit_event(
                &app,
                serde_json::json!({
                    "install_id": install_id,
                    "phase": "failed",
                    "error": format!("拷贝内置安装包到临时目录失败: {}", e),
                    "stage": "extract"
                }),
            );
            return;
        }
    }

    // 3. 查重
    if manager.is_installed(&params.key, &params.version) {
        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": format!("{} {} 已安装", entry.name, params.version),
                "stage": "extract"
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
                "stage": "extract"
            }),
        );
        return;
    }

    // 4. 创建 install_path
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
        params.key.clone(),
        params.version.clone(),
    );

    // 5. 通知前端进入 extracting 阶段。
    //    本地存在时跳过了 downloading；走回退下载时已先行 emit 过 downloading，
    //    此处切到 extracting 衔接解压，避免 createTask 默认 downloading 阶段滞留。
    emit_event(
        &app,
        serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 0
        }),
    );

    let result: Result<()> = async {
        // 6. temp_zip / cleanup_temp 已在上方准备：本地存在则已拷贝，本地缺失则已下载。

        // 6a. sha256 校验（仅对「联网下载」的安装包校验，比对官网 sha256）
        //     本地内置资源（builtin）可能经过人工修改（如重命名目录），其 sha256 与官网
        //     原版不一致，故本地来源跳过校验、信任本地文件；下载来源必须校验官方 sha256，
        //     防止文件损坏或被篡改。
        if downloaded && !builtin.sha256.is_empty() {
            let computed = compute_sha256(&temp_zip)?;
            if computed != builtin.sha256.to_lowercase() {
                cleanup_temp(&temp_zip);
                return Err(anyhow::anyhow!("内置安装包校验失败，文件可能损坏"));
            }
        }

        let app_ep = app.clone();
        let id_ep = install_id.clone();
        let mut throttle = ThrottledEmitter::new();
        let on_progress = move |extracted: u64, total: u64| {
            let percent = if total > 0 {
                (extracted as f64 / total as f64 * 100.0) as i64
            } else {
                0
            };
            if throttle.should_emit(percent) {
                emit_event(
                    &app_ep,
                    serde_json::json!({
                        "install_id": id_ep.clone(),
                        "phase": "extracting",
                        "percent": percent
                    }),
                );
            }
        };

        let extract_result = match version_info.archive.format {
            // 剥掉 zip 内单一顶层目录，避免 install_path 下多一层冗余目录
            ArchiveFormat::Zip => {
                archive::extract_zip_flatten(&temp_zip, &install_path, on_progress)
            }
            ArchiveFormat::TarGz => archive::extract_tar_gz(&temp_zip, &install_path, on_progress),
            ArchiveFormat::Executable => {
                // 单个可执行文件：从临时副本复制到 install_path 下
                // （文件名仍用原始 resource_zip 的文件名，保持语义一致）
                let dest_file = install_path.join(
                    resource_zip
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .unwrap_or_else(|| std::ffi::OsStr::new("app.exe")),
                );
                fs::copy(&temp_zip, &dest_file)
                    .map(|_| ())
                    .map_err(Into::into)
            }
        };

        // 无论解压成功与否，立即清理临时副本
        cleanup_temp(&temp_zip);

        // 传播解压结果
        extract_result?;

        emit_event(
            &app,
            serde_json::json!({
                "install_id": install_id.clone(),
                "phase": "extracting",
                "percent": 100
            }),
        );

        // 7. post_install
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

        // 8. 登记 InstalledSoftware（source = Builtin）
        let installed_id = uuid::Uuid::new_v4().to_string();
        let installed_id_for_event = installed_id.clone();
        let now = Utc::now().naive_utc();
        let installed = InstalledSoftware {
            id: installed_id,
            key: params.key.clone(),
            version: params.version.clone(),
            name: format!("{} {}", entry.name, params.version),
            install_path: format!("{}/{}", params.key, params.version),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Builtin {
                version: builtin.version.clone(),
            },
            pid: None,
            last_started_at: None,
            last_stopped_at: None,
            last_error: None,
            custom_start_command: None,
            icon: String::new(),
            category: None,
            depends_on: vec![],
            auto_restart: false,
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
        eprintln!(
            "[software] builtin install failed: key={}, version={}, error={}",
            params.key, params.version, e
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::software::{ArchiveInfo, BuiltinInfo};

    fn mirror(name: &str, builtin: bool) -> MirrorSource {
        MirrorSource {
            name: name.to_string(),
            url: if builtin {
                format!("builtin://{}", name)
            } else {
                format!("https://example.com/{}.zip", name)
            },
            builtin: builtin.then(|| BuiltinInfo {
                version: "1.0.0".to_string(),
                sha256: String::new(),
                size: 0,
            }),
        }
    }

    fn version(mirrors: Vec<MirrorSource>) -> CatalogVersion {
        CatalogVersion {
            version: "1.0.0".to_string(),
            mirrors,
            archive: ArchiveInfo {
                format: ArchiveFormat::Zip,
                size: None,
                sha256: None,
            },
        }
    }

    fn names(mirrors: &[&MirrorSource]) -> Vec<String> {
        mirrors.iter().map(|m| m.name.clone()).collect()
    }

    /// 首选源排最前，其余保持声明顺序作为兜底
    #[test]
    fn candidates_put_preferred_first_and_keep_declaration_order() {
        let v = version(vec![mirror("self-hosted", false), mirror("official", false)]);
        assert_eq!(
            names(&mirror_candidates(&v, 1)),
            vec!["official", "self-hosted"]
        );
        assert_eq!(
            names(&mirror_candidates(&v, 0)),
            vec!["self-hosted", "official"]
        );
    }

    /// builtin 镜像无真实下载地址，不进入 HTTP 下载候选
    #[test]
    fn candidates_skip_builtin_mirrors() {
        let v = version(vec![mirror("builtin", true), mirror("official", false)]);
        assert_eq!(names(&mirror_candidates(&v, 0)), vec!["official"]);
    }

    /// 首选下标越界或指向 builtin 时不应 panic，退回声明顺序
    #[test]
    fn candidates_are_safe_when_preferred_index_is_out_of_range() {
        let v = version(vec![mirror("a", false), mirror("b", false)]);
        assert_eq!(names(&mirror_candidates(&v, 99)), vec!["a", "b"]);
    }

    /// 全部是 builtin 镜像时没有可联网源，由调用方报错
    #[test]
    fn candidates_empty_when_no_downloadable_mirror() {
        let v = version(vec![mirror("builtin", true)]);
        assert!(mirror_candidates(&v, 0).is_empty());
    }
}
