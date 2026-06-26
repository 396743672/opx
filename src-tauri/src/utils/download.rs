use anyhow::Result;
use reqwest::blocking;
use std::path::Path;

pub fn download(url: &str, destination: &Path) -> Result<()> {
    let response = blocking::get(url)?.error_for_status()?;
    let content = response.bytes()?;
    std::fs::write(destination, content)?;
    Ok(())
}
