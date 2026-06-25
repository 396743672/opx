use anyhow::Result;
use std::path::Path;

pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(dest_dir)?;
    Ok(())
}

pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let bufreader = std::io::BufReader::new(file);
    let gzr = flate2::read::GzDecoder::new(bufreader);
    let mut ar = tar::Archive::new(gzr);
    ar.unpack(dest_dir)?;
    Ok(())
}