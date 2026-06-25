use crate::services::software_manager::SoftwareProvider;
use crate::models::software::{SoftwareMeta, InstallParams};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct NginxProvider;

impl SoftwareProvider for NginxProvider {
    fn meta(&self) -> SoftwareMeta {
        SoftwareMeta {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "HTTP and reverse proxy server".to_string(),
            available_versions: vec![
                "1.25.4".to_string(),
                "1.24.0".to_string(),
            ],
            default_version: "1.25.4".to_string(),
        }
    }

    fn download_url(&self, version: &str) -> String {
        #[cfg(windows)]
        {
            format!("https://github.com/zenorachi/nginx-build/releases/download/{}/nginx-{}-win64.zip", version, version)
        }
        #[cfg(not(windows))]
        {
            format!("http://nginx.org/download/nginx-{}.tar.gz", version)
        }
    }

    fn post_install(&self, _install_path: &Path, _params: &InstallParams) -> Result<()> {
        Ok(())
    }

    fn start_command(&self, software: &crate::models::software::InstalledSoftware) -> Result<Command> {
        let bin_path = PathBuf::from(&software.install_path).join("nginx");
        #[cfg(windows)]
        let nginx = bin_path.join("nginx.exe");
        #[cfg(not(windows))]
        let nginx = bin_path.join("sbin").join("nginx");

        let mut cmd = Command::new(nginx);
        cmd.current_dir(&software.install_path);
        Ok(cmd)
    }

    fn stop_command(&self, _software: &crate::models::software::InstalledSoftware) -> Result<()> {
        Ok(())
    }
}