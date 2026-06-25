use crate::services::software_manager::SoftwareProvider;
use crate::models::software::{SoftwareMeta, InstallParams};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Default)]
pub struct MysqlProvider;

impl SoftwareProvider for MysqlProvider {
    fn meta(&self) -> SoftwareMeta {
        SoftwareMeta {
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            description: "开源关系型数据库".to_string(),
            available_versions: vec![
                "8.0.36".to_string(),
                "8.1.0".to_string(),
                "5.7.44".to_string(),
            ],
            default_version: "8.0.36".to_string(),
        }
    }

    fn download_url(&self, version: &str) -> String {
        #[cfg(windows)]
        {
            format!("https://cdn.mysql.com/Downloads/MySQL-8.0/mysql-{}-winx64.zip", version)
        }
        #[cfg(target_os = "macos")]
        {
            format!("https://cdn.mysql.com/Downloads/MySQL-8.0/mysql-{}-macos13-x86_64.tar.gz", version)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            format!("https://cdn.mysql.com/Downloads/MySQL-8.0/mysql-{}-linux-glibc2.28-x86_64.tar.xz", version)
        }
    }

    fn post_install(&self, install_path: &Path, params: &InstallParams) -> Result<()> {
        // TODO: initialize data directory
        Ok(())
    }

    fn start_command(&self, software: &crate::models::software::InstalledSoftware) -> Result<Command> {
        let bin_path = PathBuf::from(&software.install_path).join("bin");
        let mysqld = bin_path.join("mysqld");
        #[cfg(windows)]
        let mysqld = bin_path.join("mysqld.exe");

        let mut cmd = Command::new(mysqld);
        cmd.current_dir(install_path);
        Ok(cmd)
    }

    fn stop_command(&self, software: &crate::models::software::InstalledSoftware) -> Result<()> {
        // TODO: implement proper stop
        Ok(())
    }
}