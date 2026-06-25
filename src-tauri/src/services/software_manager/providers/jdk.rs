use crate::services::software_manager::SoftwareProvider;
use crate::models::software::{SoftwareMeta, InstallParams};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct JdkProvider;

impl SoftwareProvider for JdkProvider {
    fn meta(&self) -> SoftwareMeta {
        SoftwareMeta {
            key: "jdk".to_string(),
            name: "JDK".to_string(),
            description: "Java Development Kit".to_string(),
            available_versions: vec![
                "17.0.10".to_string(),
                "21.0.2".to_string(),
                "8.392.0.17".to_string(),
            ],
            default_version: "17.0.10".to_string(),
        }
    }

    fn download_url(&self, version: &str) -> String {
        // Use Adoptium mirror
        format!("https://github.com/adoptium/temurin{}-binaries/releases/download/jdk-{}/OpenJDK{}U-jdk_x64_windows_hotspot_{}.zip",
            version.split('.').next().unwrap_or("17"),
            version,
            version
        )
    }

    fn post_install(&self, _install_path: &Path, _params: &InstallParams) -> Result<()> {
        Ok(())
    }

    fn start_command(&self, _software: &crate::models::software::InstalledSoftware) -> Result<Command> {
        // JDK doesn't need to be started as a service
        Ok(Command::new(""))
    }

    fn stop_command(&self, _software: &crate::models::software::InstalledSoftware) -> Result<()> {
        Ok(())
    }
}