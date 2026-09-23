//! 备份 / 恢复 / 重置后端服务（C 扩展 · 备份与恢复）
//!
//! 负责：对 provider 暴露的 `data_dirs()` 做 zip 快照（复用现有 zip 0.6 + walkdir + chrono）、
//! 列出 / 恢复 / 删除快照（manifest.json 持久化元信息），以及一键重置（对每个 data_dir 重建空态）。
//!
//! 设计要点（见 design 文档 §3.3 / 共享知识 §6）：
//! - 快照存储根：<app_data>/backups/<installed_id>/，内含 `<ts>.zip` + `manifest.json`
//! - 恢复前若实例运行中则拒绝（先停服）；跨大版本恢复需 `force` 确认（决策 8）

use std::io::{Read, Write};
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

/// 每实例快照滚动保留数量的默认值（可配置，见 AppSettings.snapshot_keep）
#[cfg(test)]
const DEFAULT_MAX_SNAPSHOTS: usize = 5;

fn write_manifest(installed_id: &str, metas: &[SnapshotMeta], max_keep: usize) -> anyhow::Result<()> {
    let p = manifest_path(installed_id);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 滚动保留最近 max_keep 个快照（按传入顺序保留末尾最近）
    let kept: &[SnapshotMeta] = if metas.len() > max_keep {
        &metas[metas.len() - max_keep..]
    } else {
        metas
    };
    let content = serde_json::to_string_pretty(kept)?;
    std::fs::write(&p, content)?;
    Ok(())
}

/// 解析快照 id 与 zip 路径：同一秒内重复创建时 zip 会重名（`File::create` 会覆盖旧文件），
/// 故已存在时追加序号 `-1`、`-2`…，保证快照 id（= zip 文件名 stem）唯一。
fn resolve_snapshot_id(root: &Path, ts: &str) -> (String, PathBuf) {
    let mut id = ts.to_string();
    let mut zip_path = root.join(format!("{}.zip", id));
    let mut seq = 1u32;
    while zip_path.exists() {
        id = format!("{}-{}", ts, seq);
        zip_path = root.join(format!("{}.zip", id));
        seq += 1;
    }
    (id, zip_path)
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
        // zip-slip 防护：拒绝含 .. 的 entry（防目录逃逸）
        if name.contains("..") {
            return Err(anyhow::anyhow!(
                "快照含非法路径（.. 逃逸），拒绝解压: {}",
                name
            ));
        }
        // 绝对条目（创建快照时对绝对 data_dir 存的是原绝对路径）→ 恢复到原路径；
        // 相对条目 → install_path 下对应位置。二者统一走下方 allowed 护栏校验防越界。
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
    max_keep: usize,
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
    let (snap_id, zip_path) = resolve_snapshot_id(&root, &ts);

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
        id: snap_id,
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
    // 预先计算将被滚动淘汰的最旧快照（写 manifest 前先删其 zip，避免孤立文件）
    let max_keep = max_keep.max(1);
    let dropped: Vec<String> = if manifest.len() >= max_keep {
        manifest
            .iter()
            .take(manifest.len() - max_keep + 1)
            .map(|m| m.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    manifest.push(meta.clone());
    write_manifest(installed_id, &manifest, max_keep)?;
    // 清理被淘汰快照的 zip 文件（manifest 已由 write_manifest 裁剪保留最近 5 个）
    for id in dropped {
        let old_zip = backups_root(installed_id).join(format!("{}.zip", id));
        if old_zip.exists() {
            let _ = std::fs::remove_file(&old_zip);
        }
    }
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
pub fn delete_snapshot(installed_id: &str, snapshot_id: &str, max_keep: usize) -> anyhow::Result<()> {
    let mut manifest = read_manifest(installed_id);
    manifest.retain(|m| m.id != snapshot_id);
    write_manifest(installed_id, &manifest, max_keep)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::paths::data_dir;
    use std::fs;
    use std::io::Write;

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn meta(id: &str) -> SnapshotMeta {
        SnapshotMeta {
            id: id.to_string(),
            created_at: "2024-01-01T00:00:00+00:00".to_string(),
            source_key: "mysql".to_string(),
            source_version: "8.4.11".to_string(),
            major_version: Some(8),
            size_bytes: 1024,
            format: "zip".to_string(),
            name: None,
            note: None,
        }
    }

    #[test]
    fn test_parse_major_version() {
        assert_eq!(parse_major_version("8.4.11"), Some(8));
        assert_eq!(parse_major_version("9.7.2"), Some(9));
        assert_eq!(parse_major_version("10"), Some(10));
        assert_eq!(parse_major_version("RELEASE.2025"), None);
        assert_eq!(parse_major_version("v1.2.3"), None);
        assert_eq!(parse_major_version(""), None);
    }

    /// 预期：默认保留 5 个快照（write_manifest 按 max_keep 滚动保留末尾最近的）。
    #[test]
    fn test_snapshot_retention_keeps_five() {
        let id = format!("__qa_retention_{}", unique_suffix());
        let metas: Vec<SnapshotMeta> = (0..6).map(|i| meta(&format!("s{}", i))).collect();
        write_manifest(&id, &metas, DEFAULT_MAX_SNAPSHOTS).unwrap();
        let got = read_manifest(&id);
        // 清理：避免污染 target 目录下的数据目录
        let _ = fs::remove_dir_all(data_dir().join("backups").join(&id));
        assert!(
            got.len() <= 5,
            "应保留最多 5 个快照（默认滚动保留），实际 {} 个",
            got.len()
        );
    }

    /// 预期：max_keep 可配置——传 10 时 6 条全保留，不被裁剪。
    #[test]
    fn test_snapshot_retention_respects_configured_max() {
        let id = format!("__qa_retention_cfg_{}", unique_suffix());
        let metas: Vec<SnapshotMeta> = (0..6).map(|i| meta(&format!("s{}", i))).collect();
        write_manifest(&id, &metas, 10).unwrap();
        let got = read_manifest(&id);
        let _ = fs::remove_dir_all(data_dir().join("backups").join(&id));
        assert_eq!(got.len(), 6, "max_keep=10 时 6 条应全部保留");
    }

    /// 预期：同一秒重复创建（zip 已存在）时 id 追加序号，不覆盖已有文件。
    #[test]
    fn test_resolve_snapshot_id_avoids_collision() {
        let base = std::env::temp_dir().join(format!("opx_qa_resolve_{}", unique_suffix()));
        let _ = fs::create_dir_all(&base);
        let ts = "20260923_140816";
        let (id1, p1) = resolve_snapshot_id(&base, ts);
        assert_eq!(id1, ts, "首次解析直接用时间戳");
        fs::File::create(&p1).unwrap(); // 模拟快照文件已落盘
        let (id2, p2) = resolve_snapshot_id(&base, ts);
        assert_eq!(id2, format!("{}-1", ts), "冲突时追加 -1");
        assert_ne!(p1, p2);
        fs::File::create(&p2).unwrap();
        let (id3, _p3) = resolve_snapshot_id(&base, ts);
        assert_eq!(id3, format!("{}-2", ts), "再次冲突追加 -2");
        let _ = fs::remove_dir_all(&base);
    }

    /// 预期：restore 解压应对含 ../ 的 entry 做 zip-slip 防护（拒绝或归一化到目标目录内）。
    /// 当前 extract_zip_to_data_dirs 用未归一化的 target.starts_with(install_path) 判定，
    /// Rust 的 Path::starts_with 不会消解 ..，导致 ../ 条目被放行 => 该断言在修复前会失败。
    #[test]
    fn test_restore_rejects_path_traversal() {
        let base = std::env::temp_dir().join(format!("opx_qa_restore_{}", unique_suffix()));
        let _ = fs::create_dir_all(&base);
        let zip_path = base.join("snap.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            // 恶意 entry：尝试逃出 install_path
            zw.start_file("../__opx_qa_escape__.txt", opts).unwrap();
            zw.write_all(b"pwned").unwrap();
            zw.finish().unwrap();
        }
        let install_path = base.join("install");
        let _ = fs::create_dir_all(&install_path);
        let result = extract_zip_to_data_dirs(&zip_path, &[], &install_path);
        // 清理：含可能被错误写出到上级目录的文件
        let _ = fs::remove_file(base.join("__opx_qa_escape__.txt"));
        let _ = fs::remove_dir_all(&base);
        assert!(
            result.is_err(),
            "restore 应包含 ../ 的 entry 拒绝/归一化（zip-slip 防护）"
        );
    }

    /// 预期：绝对路径条目应恢复到原 data_dir 内（创建快照时对绝对 data_dir 存的是原绝对路径）。
    /// 修复前 extract_zip_to_data_dirs 一律拒绝绝对条目，恢复必然失败 => 该断言在修复前会失败。
    #[test]
    fn test_restore_allows_absolute_entry_in_data_dir() {
        let base = std::env::temp_dir().join(format!("opx_qa_restore_abs_{}", unique_suffix()));
        let install_path = base.join("install");
        let data_dir = install_path.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        let zip_path = base.join("snap.zip");
        let abs_entry = data_dir.join("payload.txt");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            // 模拟 add_dir_to_zip 对绝对 data_dir 的存法：entry = 原绝对路径
            zw.start_file(abs_entry.to_string_lossy().replace('\\', "/"), opts)
                .unwrap();
            zw.write_all(b"hello").unwrap();
            zw.finish().unwrap();
        }
        // 恢复前清空 data_dir（与 restore_snapshot 行为一致）
        fs::remove_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&data_dir).unwrap();
        extract_zip_to_data_dirs(&zip_path, &[data_dir.clone()], &install_path).unwrap();
        let restored = fs::read_to_string(&abs_entry).unwrap();
        let _ = fs::remove_dir_all(&base);
        assert_eq!(restored, "hello", "绝对路径条目应恢复到原 data_dir 内");
    }
}
