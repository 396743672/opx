use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
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
    }
    Ok(())
}

pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest_dir)?;
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

        extract_zip(&archive_path, &dest_dir).expect("zip should extract");

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

        extract_tar_gz(&archive_path, &dest_dir).expect("tar.gz should extract");

        assert_eq!(
            fs::read_to_string(dest_dir.join("nested").join("file.txt"))
                .expect("extracted file should exist"),
            "tar-content"
        );
        fs::remove_dir_all(root).ok();
    }
}
