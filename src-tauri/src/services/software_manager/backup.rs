//! 备份 / 恢复 / 重置后端服务（C 扩展 · 备份与恢复）
//!
//! 负责：对 provider 暴露的 `data_dirs()` 做 zip 快照（复用现有 zip 0.6 + walkdir + chrono）、
//! 列出 / 恢复 / 删除快照（manifest.json 持久化元信息），以及一键重置（对每个 data_dir 重建空态）。
//!
//! 设计要点（见 design 文档 §3.3 / 共享知识 §6）：
//! - 快照存储根：<app_data>/backups/<installed_id>/，内含 `<ts>.zip` + `manifest.json`
//! - 恢复前若实例运行中则拒绝（先停服）；跨大版本恢复需 `force` 确认（决策 8）

use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::Local;
use tauri::AppHandle;

use crate::models::software::{BackupMode, SnapshotMeta, SoftwareStatus};
use crate::services::software_manager::health_check;
use crate::services::software_manager::lifecycle;
use crate::services::software_manager::providers::{all_providers, DataDirContext};
use crate::services::software_manager::SoftwareManager;
use crate::utils::paths;

const SNAPSHOT_FORMAT: &str = "zip";

/// 备份根目录：<app_data>/backups/<installed_id>/
fn backups_root(installed_id: &str) -> PathBuf {
    paths::data_dir().join("backups").join(installed_id)
}

/// 解析大版本号：版本必须以数字开头才解析（如 "8.4.11" -> 8；"RELEASE.2025" -> None，与 MinIO 等对齐）。
fn parse_major_version(version: &str) -> Option<u32> {
    let first = version.chars().next()?;
    if !first.is_ascii_digit() {
        return None;
    }
    version
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok()
}

fn manifest_path(installed_id: &str) -> PathBuf {
    backups_root(installed_id).join("manifest.json")
}

fn read_manifest(installed_id: &str) -> Vec<SnapshotMeta> {
    let p = manifest_path(installed_id);
    if !p.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&p) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn write_manifest(installed_id: &str, metas: &[SnapshotMeta]) -> anyhow::Result<()> {
    let p = manifest_path(installed_id);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(metas)?;
    std::fs::write(&p, content)?;
    Ok(())
}

/// 把目录递归加入 zip。entry 以 `rel_root` 为相对根（恢复时映射回原目录）；
/// 绝对路径数据目录直接用绝对路径作为 entry（恢复时按原路径解压）。
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    rel_root: &Path,
    is_absolute: bool,
    opts: zip::write::FileOptions,
) -> anyhow::Result<()> {
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = if is_absolute {
            path.to_string_lossy().replace('\\', "/")
        } else {
            let rel = path
                .strip_prefix(rel_root)
                .unwrap_or_else(|_| path.file_name().map(Path::new).unwrap_or(path));
            rel.to_string_lossy().replace('\\', "/")
        };
        if path.is_dir() {
            zip.add_directory(&format!("{}/", name), opts)
                .map_err(|e| anyhow::anyhow!("写入目录失败: {}", e))?;
        } else {
            zip.start_file(&name, opts)
                .map_err(|e| anyhow::anyhow!("写入文件头失败: {}", e))?;
            let data = std::fs::read(path)
                .map_err(|e| anyhow::anyhow!("读取 {} 失败: {}", path.display(), e))?;
            zip.write_all(&data)
                .map_err(|e| anyhow::anyhow!("写入 {} 失败: {}", path.display(), e))?;
        }
    }
    Ok(())
}

fn is_absolute_entry(name: &str) -> bool {
    (name.len() >= 2 && name.as_bytes()[1] == b':') || name.starts_with('/')
}

/// 从 zip 解压覆盖到各数据目录（按 entry 映射：绝对→原路径；相对→install_path.join(entry)）
fn extract_zip_to_data_dirs(
    zip_path: &Path,
    data_dirs: &[PathBuf],
    install_path: &Path,
) -> anyhow::Result<()> {
    let f = std::fs::File::open(zip_path)
        .map_err(|e| anyhow::anyhow!("打开快照失败 {}: {}", zip_path.display(), e))?;
    let mut archive = zip::ZipArchive::new(f)
        .map_err(|e| anyhow::anyhow!("快照格式错误 {}: {}", zip_path.display(), e))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| anyhow::anyhow!("读取条目失败: {}", e))?;
        let name = file.name().to_string();
        let target = if is_absolute_entry(&name) {
            PathBuf::from(&name)
        } else {
            install_path.join(&name)
        };
        // 护栏：解压目标必须落在某个数据目录内或 install_path 内，防 zip-slip
        let allowed = data_dirs.iter().any(|d| target.starts_with(d))
            || target.starts_with(install_path);
        if !allowed {
            return Err(anyhow::anyhow!(
                "快照含越界路径，拒绝解压: {}",
                target.display()
            ));
        }
        if file.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&target)
                .map_err(|e| anyhow::anyhow!("创建 {} 失败: {}", target.display(), e))?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            out.write_all(&buf)?;
        }
    }
    Ok(())
}

/// 创建快照：压缩 data_dirs() → <backups>/<ts>.zip，并写入/更新 manifest.json
#[allow(clippy::too_many_arguments)]
pub fn create_snapshot(
    manager: &SoftwareManager,
    app: &AppHandle,
    installed_id: &str,
    mode: BackupMode,
    name: Option<String>,
    note: Option<String>,
) -> anyhow::Result<SnapshotMeta> {
    let sw = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    let install_path = PathBuf::from(&sw.install_path);
    let providers = all_providers();
    let provider = providers
        .iter()
        .find(|p| p.key() == sw.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", sw.key))?;
    let dctx = DataDirContext {
        install_path: sw.install_path.clone(),
        version: sw.version.clone(),
        config: sw.config.clone(),
    };
    let data_dirs = provider.data_dirs(&dctx);

    // StopAndBackup 模式：先停服（决策 2 默认提示停服）
    if mode == BackupMode::StopAndBackup {
        if let Some(pid) = sw.pid {
            if health_check::is_process_alive(pid) {
                lifecycle::stop_one(pid);
                manager.update_runtime_fields(
                    installed_id,
                    SoftwareStatus::Stopped,
                    None,
                    None,
                    Some(Local::now().naive_local()),
                    None,
                )?;
                lifecycle::emit_status_changed(app, installed_id, SoftwareStatus::Stopped, None, None);
            }
        }
    }

    let root = backups_root(installed_id);
    std::fs::create_dir_all(&root)?;
    let ts = Local::now().format("%Y%m%d_%H%M%S").to_string();
    let zip_path = root.join(format!("{}.zip", ts));

    let f = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(f);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for dir in &data_dirs {
        if !dir.exists() {
            // 目录尚未创建（如未初始化的 data）：跳过，避免空快照
            continue;
        }
        let is_abs = dir.is_absolute();
        let rel_root = if is_abs { dir.clone() } else { install_path.clone() };
        add_dir_to_zip(&mut zip, dir, &rel_root, is_abs, opts)?;
    }
    let f = zip.finish()?;
    f.sync_all()?;

    let size_bytes = std::fs::metadata(&zip_path)?.len();
    let meta = SnapshotMeta {
        id: ts,
        created_at: Local::now().to_rfc3339(),
        source_key: sw.key.clone(),
        source_version: sw.version.clone(),
        major_version: parse_major_version(&sw.version),
        size_bytes,
        format: SNAPSHOT_FORMAT.to_string(),
        name,
        note,
    };
    let mut manifest = read_manifest(installed_id);
    manifest.push(meta.clone());
    write_manifest(installed_id, &manifest)?;
    Ok(meta)
}

/// 列出某实例的全部快照（读 manifest.json）
pub fn list_snapshots(installed_id: &str) -> anyhow::Result<Vec<SnapshotMeta>> {
    Ok(read_manifest(installed_id))
}

/// 恢复快照：运行态必须停服；跨大版本/跨软件校验（不一致且非 force 则返回警告错误）
pub fn restore_snapshot(
    manager: &SoftwareManager,
    installed_id: &str,
    snapshot_id: &str,
    force: bool,
) -> anyhow::Result<()> {
    let sw = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;

    // 运行态必须停服（决策 2：热备外需停服；此处统一要求先停服再恢复）
    if let Some(pid) = sw.pid {
        if health_check::is_process_alive(pid) {
            return Err(anyhow::anyhow!("实例正在运行，请先停止后再恢复快照"));
        }
    }

    let manifest = read_manifest(installed_id);
    let meta = manifest
        .iter()
        .find(|m| m.id == snapshot_id)
        .ok_or_else(|| anyhow::anyhow!("未找到快照: {}", snapshot_id))?
        .clone();

    // P1: 跨软件 / 跨大版本校验（决策 8）
    if !force {
        if meta.source_key != sw.key {
            return Err(anyhow::anyhow!(
                "快照来源({})与当前实例({})不一致，如需继续请使用强制恢复",
                meta.source_key,
                sw.key
            ));
        }
        if let (Some(mv), Some(cmv)) = (meta.major_version, parse_major_version(&sw.version)) {
            if mv != cmv {
                return Err(anyhow::anyhow!(
                    "快照大版本({})与当前实例大版本({})不一致，跨大版本恢复可能不兼容；如需继续请使用强制恢复（force）",
                    mv,
                    cmv
                ));
            }
        }
    }

    let zip_path = backups_root(installed_id).join(format!("{}.zip", snapshot_id));
    if !zip_path.exists() {
        return Err(anyhow::anyhow!("快照文件不存在: {}", zip_path.display()));
    }

    let install_path = PathBuf::from(&sw.install_path);
    let providers = all_providers();
    let provider = providers
        .iter()
        .find(|p| p.key() == sw.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", sw.key))?;
    let dctx = DataDirContext {
        install_path: sw.install_path.clone(),
        version: sw.version.clone(),
        config: sw.config.clone(),
    };
    let data_dirs = provider.data_dirs(&dctx);

    // 恢复前清空各数据目录（保留目录本身，避免旧数据残留覆盖）
    for dir in &data_dirs {
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir)?;
    }
    extract_zip_to_data_dirs(&zip_path, &data_dirs, &install_path)?;
    Ok(())
}

/// 删除快照：删 zip + 更新 manifest
pub fn delete_snapshot(installed_id: &str, snapshot_id: &str) -> anyhow::Result<()> {
    let mut manifest = read_manifest(installed_id);
    manifest.retain(|m| m.id != snapshot_id);
    write_manifest(installed_id, &manifest)?;
    let zip_path = backups_root(installed_id).join(format!("{}.zip", snapshot_id));
    if zip_path.exists() {
        std::fs::remove_file(&zip_path)?;
    }
    Ok(())
}

/// 一键重置：对每个 data_dir 重建空态（含护栏：必须位于 install_path 下且为目录）
pub fn reset_instance(manager: &SoftwareManager, installed_id: &str) -> anyhow::Result<()> {
    let sw = manager
        .find_installed(installed_id)
        .ok_or_else(|| anyhow::anyhow!("未找到安装记录: {}", installed_id))?;
    let install_path = PathBuf::from(&sw.install_path);
    let providers = all_providers();
    let provider = providers
        .iter()
        .find(|p| p.key() == sw.key)
        .ok_or_else(|| anyhow::anyhow!("未找到 provider: {}", sw.key))?;
    let dctx = DataDirContext {
        install_path: sw.install_path.clone(),
        version: sw.version.clone(),
        config: sw.config.clone(),
    };
    let data_dirs = provider.data_dirs(&dctx);
    lifecycle::reset_data_dirs(&data_dirs, &install_path)?;
    Ok(())
}
