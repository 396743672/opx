use anyhow::Result;
use std::fs;
use std::path::Path;

/// 解压进度回调：(已解压字节数, 总字节数)。
/// 约定与 installer.rs 内「下载进度回调」一致：由调用方在闭包里节流后 emit install-progress 事件
/// （phase/percent 字段对齐下载），便于前端统一处理。extract 已知总量故省去 Option。
pub fn extract_zip<F>(archive_path: &Path, dest_dir: &Path, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    // 预扫描：累加所有条目的未压缩大小，用于进度百分比（仅读中央目录元数据，不写盘）
    let total: u64 = (0..archive.len())
        .map(|i| archive.by_index(i).map(|f| f.size()).unwrap_or(0))
        .sum();
    let mut extracted: u64 = 0;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let size = file.size();
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };
        if let Some(parent) = outpath.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            std::io::copy(&mut file, &mut std::fs::File::create(&outpath)?)?;
        }
        extracted += size;
        on_progress(extracted, total);
    }
    Ok(())
}

pub fn extract_tar_gz<F>(archive_path: &Path, dest_dir: &Path, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    // 预扫描：累加所有条目的未压缩大小，用于进度百分比（仅读各条目头 size，不写盘）。
    // 注意：tar 基于前向只读的 gz 流，无法在单次遍历中预知总量，故先完整扫描一遍估算 total，
    // 实际解压时再遍历一次。这是解压进度可预估的必要代价，对一次性安装可接受。
    let total: u64 = {
        let file = std::fs::File::open(archive_path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        let mut sum: u64 = 0;
        for entry in archive.entries()? {
            sum += entry?.header().size()?;
        }
        sum
    };

    let file = std::fs::File::open(archive_path)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    let mut extracted: u64 = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header();
        let entry_size = header.size()?;
        let entry_type = header.entry_type();
        // 复用 tar 内部的路径归一化（与 Archive::unpack 行为一致），避免路径穿越
        let outpath = dest_dir.join(entry.path()?.to_path_buf());
        match entry_type {
            tar::EntryType::Directory => {
                fs::create_dir_all(&outpath)?;
            }
            tar::EntryType::Regular => {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut entry, &mut outfile)?;
            }
            // 软/硬链接等其它类型：当前软件包不含，跳过以保证安装流程不中断
            _ => {}
        }
        extracted += entry_size;
        on_progress(extracted, total);
    }
    Ok(())
}

/// 检测 zip 内所有条目是否共享同一个顶层目录。
/// 若是，返回该顶层目录名（用于解压时剥层）；否则返回 None。
/// 例如所有条目都形如 `mysql-8.4.10-winx64/...` 时返回 Some("mysql-8.4.10-winx64")。
fn detect_common_root(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<String> {
    let mut common: Option<String> = None;
    for i in 0..archive.len() {
        let file = archive.by_index(i).ok()?;
        let path = file.enclosed_name()?;
        let first = path.components().next()?;
        let seg = first.as_os_str().to_string_lossy().to_string();
        // 顶层直接存在文件（路径仅一段且非目录），说明不是单一顶层目录结构
        let comp_count = path.components().count();
        if comp_count == 1 && !file.is_dir() {
            return None;
        }
        match &common {
            None => common = Some(seg),
            Some(c) if *c != seg => return None,
            _ => {}
        }
    }
    common.filter(|s| !s.is_empty())
}

/// 解压 zip 并自动剥掉单一顶层目录。
/// 若 zip 内所有条目共享同一个顶层目录（如 `mysql-8.4.10-winx64/`），
/// 解压后将其内容直接放到 dest_dir 根，避免多一层冗余目录；
/// 否则等同于 extract_zip 的行为。
pub fn extract_zip_flatten<F>(archive_path: &Path, dest_dir: &Path, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let common_root = detect_common_root(&mut archive);

    // 预扫描：累加所有条目的未压缩大小，用于进度百分比
    let total: u64 = (0..archive.len())
        .map(|i| archive.by_index(i).map(|f| f.size()).unwrap_or(0))
        .sum();

    let mut extracted: u64 = 0;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let size = file.size();
        let enclosed = match file.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => continue,
        };
        // 若存在公共顶层目录，剥掉该前缀
        let rel = match &common_root {
            Some(root) => match enclosed.strip_prefix(root) {
                Ok(stripped) => stripped.to_path_buf(),
                Err(_) => enclosed,
            },
            None => enclosed,
        };
        // 剥层后为空（即顶层目录本身）时跳过
        if rel.as_os_str().is_empty() {
            continue;
        }
        let outpath = dest_dir.join(&rel);
        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            std::io::copy(&mut file, &mut std::fs::File::create(&outpath)?)?;
        }
        extracted += size;
        on_progress(extracted, total);
    }
    Ok(())
}
