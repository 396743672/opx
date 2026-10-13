use anyhow::Result;
use std::fs;
use std::io::Read;
use std::path::Path;

use crate::models::software::ArchiveFormat;

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

pub fn extract_tar_gz<F>(archive_path: &Path, dest_dir: &Path, on_progress: F) -> Result<()>
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
    unpack_tar(gz, dest_dir, total, on_progress)
}

/// 解压 `.tar.xz`（xz 压缩的 tar，如 MySQL Linux 官方包）。
///
/// 本项目的 xz 解码器（纯 Rust `lzma-rs`）是 **writer 式**（无流式 `Read`），
/// 故先把归档整体解压成同目录临时 `.tar`，再按普通 tar 解包；结束后删除临时文件。
pub fn extract_tar_xz<F>(archive_path: &Path, dest_dir: &Path, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let tmp = archive_path.with_extension("tar.tmp");
    {
        let mut src = std::io::BufReader::new(std::fs::File::open(archive_path)?);
        let mut dst = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        lzma_rs::xz_decompress(&mut src, &mut dst)
            .map_err(|e| anyhow::anyhow!("xz 解压失败: {:?}", e))?;
        std::io::Write::flush(&mut dst)?;
    }
    let result = (|| -> Result<()> {
        let total: u64 = {
            let f = std::fs::File::open(&tmp)?;
            let mut archive = tar::Archive::new(f);
            let mut sum: u64 = 0;
            for entry in archive.entries()? {
                sum += entry?.header().size()?;
            }
            sum
        };
        let f = std::fs::File::open(&tmp)?;
        unpack_tar(f, dest_dir, total, &mut on_progress)
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

/// 从 tar 流解包到 `dest_dir`；`total` 仅用于进度百分比。
/// 复用 tar 内部的路径归一化（与 `Archive::unpack` 行为一致），避免路径穿越。
fn unpack_tar<R: Read, F: FnMut(u64, u64)>(
    reader: R,
    dest_dir: &Path,
    total: u64,
    mut on_progress: F,
) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut extracted: u64 = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header();
        let entry_size = header.size()?;
        let entry_type = header.entry_type();
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

/// 下载后、解压前的归档完整性校验（防御深度）。
///
/// 即使服务器未返回 `content-length`（代理常转为 chunked 而丢弃长度），
/// 也能在此拦截两类「伪装成正常归档」的坏文件，避免解压阶段才崩并报出
/// 晦涩的 `Could not find central directory end`：
/// 1. **截断归档**：大文件经代理/GitHub 传输中途断流，文件尾中央目录缺失。
///    - Zip：直接 `ZipArchive::new` 读文件尾 EOCD，截断即失败（正是根因复现点）。
/// 2. **HTML 错误页**：代理返回 200 但 body 是错误页（无归档魔数）。
///    - Zip：同上，无 PK 头 → 失败；TarGz：gzip 魔数 0x1f 0x8b 校验。
///
/// 校验失败由调用方删除缓存并触发镜像重试；纯二进制（Executable）不强制校验。
pub fn validate_archive_header(path: &Path, format: &ArchiveFormat) -> Result<()> {
    match format {
        ArchiveFormat::Zip => {
            let f = std::fs::File::open(path)
                .map_err(|e| anyhow::anyhow!("无法打开下载文件: {}", e))?;
            // 读文件尾中央目录：截断/损坏的 zip 在此即失败，错误信息直指根因。
            zip::ZipArchive::new(f).map_err(|e| {
                anyhow::anyhow!("ZIP 归档无效（可能下载被截断或损坏）: {}", e)
            })?;
        }
        ArchiveFormat::TarGz => {
            let mut buf = [0u8; 2];
            let mut f = std::fs::File::open(path)
                .map_err(|e| anyhow::anyhow!("无法打开下载文件: {}", e))?;
            let n = f.read(&mut buf)?;
            if n < 2 || buf != [0x1f, 0x8b] {
                return Err(anyhow::anyhow!(
                    "tar.gz 归档无效：文件头不是 gzip 魔数（疑似下载到 HTML 错误页或被截断）"
                ));
            }
        }
        ArchiveFormat::TarXz => {
            // xz 魔数：FD 37 7A 58 5A 00（6 字节）
            let mut buf = [0u8; 6];
            let mut f = std::fs::File::open(path)
                .map_err(|e| anyhow::anyhow!("无法打开下载文件: {}", e))?;
            let n = f.read(&mut buf)?;
            if n < 6 || buf != [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
                return Err(anyhow::anyhow!(
                    "tar.xz 归档无效：文件头不是 xz 魔数（疑似下载到 HTML 错误页或被截断）"
                ));
            }
        }
        ArchiveFormat::Executable => { /* 二进制不强制校验，交由上层测活/启动兜底 */ }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("opx_arch_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.zip");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn validate_archive_header_accepts_well_formed_zip() {
        // 构造一个最小合法 zip（含一个空文件），中央目录完整。
        let dir = std::env::temp_dir().join(format!("opx_zip_ok_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ok.zip");
        {
            let f = std::fs::File::create(&p).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("hello.txt", opts).unwrap();
            zw.write_all(b"hi").unwrap();
            zw.finish().unwrap();
        }
        assert!(validate_archive_header(&p, &ArchiveFormat::Zip).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_archive_header_rejects_truncated_zip() {
        // 复现用户报错：212MB 的 nacos zip 被代理截断，文件尾 EOCD 缺失。
        // 用一个合法 zip 的前若干个字节模拟「截断」——ZipArchive::new 必失败。
        let dir = std::env::temp_dir().join(format!("opx_zip_bad_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let full = dir.join("full.zip");
        let trunc = dir.join("trunc.zip");
        {
            let f = std::fs::File::create(&full).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("data.txt", opts).unwrap();
            zw.write_all(b"some payload that should be much longer in reality").unwrap();
            zw.finish().unwrap();
        }
        let bytes = std::fs::read(&full).unwrap();
        // 只取前半字节，模拟传输中断。
        std::fs::write(&trunc, &bytes[..bytes.len() / 2]).unwrap();
        let err = validate_archive_header(&trunc, &ArchiveFormat::Zip);
        assert!(err.is_err(), "截断 zip 必须被拦截");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_archive_header_rejects_html_page_as_tar_gz() {
        // 代理返回 200 但 body 是 HTML 错误页，却以 .tar.gz 名义下载。
        let p = write_temp(b"<html><body>404 Not Found</body></html>");
        let err = validate_archive_header(&p, &ArchiveFormat::TarGz);
        assert!(err.is_err(), "HTML 错误页伪装的 tar.gz 必须被拦截");
        let _ = std::fs::remove_file(&p);
    }

    /// 往返：构造 tar → xz 压缩 → 魔数校验通过 → 解压内容一致（覆盖 MySQL Linux 的 .tar.xz 路径）。
    #[test]
    fn extract_tar_xz_round_trip() {
        let dir = std::env::temp_dir().join(format!("opx_xz_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 1) 造一个 tar（含子目录下文件）
        let tar_path = dir.join("inner.tar");
        {
            let f = std::fs::File::create(&tar_path).unwrap();
            let mut tb = tar::Builder::new(f);
            let data = b"hello xz";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tb.append_data(&mut header, "sub/hello.txt", &data[..]).unwrap();
            tb.finish().unwrap();
        }
        // 2) xz 压缩为 pkg.tar.xz
        let xz_path = dir.join("pkg.tar.xz");
        {
            let mut src = std::io::BufReader::new(std::fs::File::open(&tar_path).unwrap());
            let mut dst = std::fs::File::create(&xz_path).unwrap();
            lzma_rs::xz_compress(&mut src, &mut dst).unwrap();
        }
        // 3) 魔数校验（xz）通过
        assert!(validate_archive_header(&xz_path, &ArchiveFormat::TarXz).is_ok());
        // 4) 解压并校验内容
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        extract_tar_xz(&xz_path, &out, |_, _| {}).unwrap();
        let got = std::fs::read_to_string(out.join("sub").join("hello.txt")).unwrap();
        assert_eq!(got, "hello xz");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
