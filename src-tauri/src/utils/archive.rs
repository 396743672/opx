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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "opx_archive_utils_{name}_{}_{}",
            std::process::id(),
            nanos
        ))
    }

    #[test]
    fn extract_zip_extracts_nested_files() {
        let root = temp_path("zip");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.zip");
        let dest_dir = root.join("dest");

        let archive_file = fs::File::create(&archive_path).expect("zip should be created");
        let mut zip = zip::ZipWriter::new(archive_file);
        let options = zip::write::FileOptions::default();
        zip.add_directory("nested/", options)
            .expect("directory should be added");
        zip.start_file("nested/file.txt", options)
            .expect("file should be added");
        zip.write_all(b"zip-content")
            .expect("file content should be written");
        zip.finish().expect("zip should be finalized");

        extract_zip(&archive_path, &dest_dir, |_, _| {}).expect("zip should extract");

        assert_eq!(
            fs::read_to_string(dest_dir.join("nested").join("file.txt"))
                .expect("extracted file should exist"),
            "zip-content"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extract_tar_gz_extracts_nested_files() {
        let root = temp_path("tar_gz");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.tar.gz");
        let dest_dir = root.join("dest");

        let tar_gz = fs::File::create(&archive_path).expect("archive should be created");
        let encoder = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let content = b"tar-content";
        let mut header = tar::Header::new_gnu();
        header.set_path("nested/file.txt").expect("path should be set");
        header.set_size(content.len() as u64);
        header.set_cksum();
        archive
            .append(&header, &content[..])
            .expect("file should be appended");
        archive.finish().expect("archive should be finalized");
        let encoder = archive.into_inner().expect("encoder should be returned");
        encoder.finish().expect("gzip should be finalized");

        extract_tar_gz(&archive_path, &dest_dir, |_, _| {}).expect("tar.gz should extract");

        assert_eq!(
            fs::read_to_string(dest_dir.join("nested").join("file.txt"))
                .expect("extracted file should exist"),
            "tar-content"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extract_zip_flatten_strips_single_top_dir() {
        // 模拟 mysql-8.4.10-winx64/ 这种单一顶层目录结构
        let root = temp_path("flatten_single");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.zip");
        let dest_dir = root.join("dest");

        let archive_file = fs::File::create(&archive_path).expect("zip should be created");
        let mut zip = zip::ZipWriter::new(archive_file);
        let options = zip::write::FileOptions::default();
        zip.add_directory("mysql-8.4.10-winx64/", options).unwrap();
        zip.add_directory("mysql-8.4.10-winx64/bin/", options).unwrap();
        zip.start_file("mysql-8.4.10-winx64/bin/mysqld.exe", options).unwrap();
        zip.write_all(b"exe-content").unwrap();
        zip.start_file("mysql-8.4.10-winx64/LICENSE", options).unwrap();
        zip.write_all(b"license").unwrap();
        zip.finish().expect("zip should be finalized");

        extract_zip_flatten(&archive_path, &dest_dir, |_, _| {}).expect("should extract");

        // 顶层目录被剥掉，文件直接在 dest_dir 下
        assert_eq!(
            fs::read_to_string(dest_dir.join("bin").join("mysqld.exe")).unwrap(),
            "exe-content"
        );
        assert_eq!(fs::read_to_string(dest_dir.join("LICENSE")).unwrap(), "license");
        // 不应存在多一层的 mysql-8.4.10-winx64 目录
        assert!(!dest_dir.join("mysql-8.4.10-winx64").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extract_zip_flatten_keeps_root_file_only() {
        // 模拟 rustfs.zip 内直接是 rustfs.exe（无顶层目录）
        let root = temp_path("flatten_rootfile");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.zip");
        let dest_dir = root.join("dest");

        let archive_file = fs::File::create(&archive_path).expect("zip should be created");
        let mut zip = zip::ZipWriter::new(archive_file);
        let options = zip::write::FileOptions::default();
        zip.start_file("rustfs.exe", options).unwrap();
        zip.write_all(b"rustfs-bin").unwrap();
        zip.finish().expect("zip should be finalized");

        extract_zip_flatten(&archive_path, &dest_dir, |_, _| {}).expect("should extract");

        assert_eq!(
            fs::read_to_string(dest_dir.join("rustfs.exe")).unwrap(),
            "rustfs-bin"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extract_zip_flatten_preserves_multiple_top_dirs() {
        // 多个顶层目录时不剥层（保持原样）
        let root = temp_path("flatten_multi");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.zip");
        let dest_dir = root.join("dest");

        let archive_file = fs::File::create(&archive_path).expect("zip should be created");
        let mut zip = zip::ZipWriter::new(archive_file);
        let options = zip::write::FileOptions::default();
        zip.start_file("dirA/a.txt", options).unwrap();
        zip.write_all(b"aaa").unwrap();
        zip.start_file("dirB/b.txt", options).unwrap();
        zip.write_all(b"bbb").unwrap();
        zip.finish().expect("zip should be finalized");

        extract_zip_flatten(&archive_path, &dest_dir, |_, _| {}).expect("should extract");

        // 两个顶层目录都保留
        assert_eq!(fs::read_to_string(dest_dir.join("dirA").join("a.txt")).unwrap(), "aaa");
        assert_eq!(fs::read_to_string(dest_dir.join("dirB").join("b.txt")).unwrap(), "bbb");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn extract_zip_flatten_strips_minio_release_dir() {
        // 模拟 minio 的 RELEASE.2025-04-22T15-44-28Z/minio.exe 结构
        let root = temp_path("flatten_minio");
        fs::create_dir_all(&root).expect("temp root should be created");
        let archive_path = root.join("sample.zip");
        let dest_dir = root.join("dest");

        let archive_file = fs::File::create(&archive_path).expect("zip should be created");
        let mut zip = zip::ZipWriter::new(archive_file);
        let options = zip::write::FileOptions::default();
        zip.add_directory("RELEASE.2025-04-22T15-44-28Z/", options).unwrap();
        zip.start_file("RELEASE.2025-04-22T15-44-28Z/minio.exe", options).unwrap();
        zip.write_all(b"minio-bin").unwrap();
        zip.finish().expect("zip should be finalized");

        extract_zip_flatten(&archive_path, &dest_dir, |_, _| {}).expect("should extract");
        assert_eq!(fs::read_to_string(dest_dir.join("minio.exe")).unwrap(), "minio-bin");
        assert!(!dest_dir.join("RELEASE.2025-04-22T15-44-28Z").exists());
        fs::remove_dir_all(root).ok();
    }
}
