use anyhow::{Result, Context};
use std::fs;
use std::path::Path;

const LABEL: &str = "dev.opx.helper";

pub fn register_current() -> Result<()> {
    let executable_path = std::env::current_exe()
        .context("Failed to get current executable path")?;

    let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#, LABEL, executable_path.display());

    let launchd_dir = Path::new("/Library/LaunchDaemons");
    let plist_path = launchd_dir.join(format!("{}.plist", LABEL));

    fs::write(&plist_path, plist_content)
        .context("Failed to write launchd plist. Try running with sudo.")?;

    println!("Launchd plist installed to {}", plist_path.display());
    println!("Run: sudo launchctl load {}", plist_path.display());

    Ok(())
}

pub fn unregister_current() -> Result<()> {
    let launchd_dir = Path::new("/Library/LaunchDaemons");
    let plist_path = launchd_dir.join(format!("{}.plist", LABEL));

    if plist_path.exists() {
        // Unload first
        let output = std::process::Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&plist_path)
            .output()
            .context("Failed to run launchctl unload");

        if let Err(e) = &output {
            eprintln!("Warning: launchctl unload failed to execute: {:?}", e);
        } else if !output.as_ref().unwrap().status.success() {
            eprintln!("Warning: launchctl unload failed with exit code: {:?}", output.as_ref().unwrap().status.code());
        }

        fs::remove_file(&plist_path)
            .context("Failed to remove launchd plist. Try running with sudo.")?;

        println!("Launchd entry removed from {}", plist_path.display());
    }

    Ok(())
}