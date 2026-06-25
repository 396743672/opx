use crate::services::software_manager::SoftwareProvider;
use crate::models::software::{SoftwareMeta, InstallParams};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct RedisProvider;

impl SoftwareProvider for RedisProvider {
    fn meta(&self) -> SoftwareMeta {
        SoftwareMeta {
            key: "redis".to_string(),
            name: "Redis".to_string(),
            description: "内存数据结构存储系统".to_string(),
            available_versions: vec![
                "7.2.4".to_string(),
                "6.2.14".to_string(),
            ],
            default_version: "7.2.4".to_string(),
        }
    }

    fn download_url(&self, version: &str) -> String {
        #[cfg(windows)]
        {
            format!("https://github.com/redis/redis/releases/download/{}/redis-{}-win64.zip", version, version)
        }
        #[cfg(not(windows))]
        {
            format!("https://github.com/redis/redis/releases/download/{}/redis-{}.tar.gz", version, version)
        }
    }

    fn post_install(&self, _install_path: &Path, _params: &InstallParams) -> Result<()> {
        Ok(())
    }

    fn start_command(&self, software: &crate::models::software::InstalledSoftware) -> Result<Command> {
        let bin_path = PathBuf::from(&software.install_path);
        #[cfg(windows)]
        let redis_server = bin_path.join("redis-server.exe");
        #[cfg(not(windows))]
        let redis_server = bin_path.join("src").join("redis-server");

        let mut cmd = Command::new(redis_server);
        cmd.current_dir(&software.install_path);
        Ok(cmd)
    }

    fn stop_command(&self, _software: &crate::models::software::InstalledSoftware) -> Result<()> {
        Ok(())
    }
}