use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

const SERVICE_NAME: &str = "opx";

pub fn register_current() -> Result<()> {
    let executable_path = std::env::current_exe()
        .context("Failed to get current executable path")?;

    let unit_content = format!(r#"[Unit]
Description=OPX - Lightweight cross-platform operations management tool
After=network.target

[Service]
Type=simple
ExecStart={}
WorkingDirectory={}
Restart=on-failure

[Install]
WantedBy=multi-user.target
"#, executable_path.display(), executable_path.parent().unwrap().display());

    let systemd_dir = Path::new("/etc/systemd/system");
    let unit_path = systemd_dir.join(format!("{}.service", SERVICE_NAME));

    fs::write(&unit_path, unit_content)
        .context("Failed to write systemd unit file. Try running with sudo.")?;

    println!("Systemd service installed to {}", unit_path.display());
    println!("Run: sudo systemctl daemon-reload && sudo systemctl enable {}", SERVICE_NAME);

    Ok(())
}

pub fn unregister_current() -> Result<()> {
    let systemd_dir = Path::new("/etc/systemd/system");
    let unit_path = systemd_dir.join(format!("{}.service", SERVICE_NAME));

    if unit_path.exists() {
        fs::remove_file(&unit_path)
            .context("Failed to remove systemd unit file. Try running with sudo.")?;
        println!("Systemd service removed from {}", unit_path.display());
    }

    Ok(())
}