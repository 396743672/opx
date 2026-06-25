use anyhow::Result;
use reqwest::blocking;

pub fn download(url: &str, destination: &std::path::Path) -> Result<()> {
    let response = blocking::get(url)?;
    let content = response.bytes()?;
    std::fs::write(destination, content)?;
    Ok(())
}