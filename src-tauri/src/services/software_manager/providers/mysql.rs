use anyhow::Result;
use std::fs::File;
use std::io::Write;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, MirrorSource, SoftwareCategory,
};

use super::{InstallContext, SoftwareProvider};

pub struct MySqlProvider;

impl MySqlProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MySqlProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for MySqlProvider {
    fn key(&self) -> &str {
        "mysql"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "8.4.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "MySQL 官方 CDN".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.0-winx64.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
            versions.push(CatalogVersion {
                version: "8.0.36".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "MySQL 官方 CDN".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.0/mysql-8.0.36-winx64.zip".to_string(),
                        builtin: None,
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "8.4.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "MySQL 官方 CDN".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.0-linux-glibc2.28-x86_64.tar.gz".to_string(),
                    },
                ],
                archive: ArchiveInfo {
                    format: ArchiveFormat::TarGz,
                    size: None,
                    sha256: None,
                },
            });
        }

        CatalogEntry {
            key: "mysql".to_string(),
            name: "MySQL".to_string(),
            description: "关系型数据库".to_string(),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "8.4.0".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // MySQL zip 解压后包含 mysql-{version}-winx64 子目录
        // my.ini 应放在该子目录内（MySQL 程序目录根），basedir/datadir 也指向该子目录
        let mysql_subdir_name = format!("mysql-{}-winx64", ctx.version);
        let mysql_dir = ctx.install_dir().join(&mysql_subdir_name);

        let (my_ini_path, basedir) = if mysql_dir.is_dir() {
            (mysql_dir.join("my.ini"), mysql_dir)
        } else {
            // 兜底：解压结构不同时，放到 install_dir 根
            (ctx.install_dir().join("my.ini"), ctx.install_dir().to_path_buf())
        };

        let basedir_forward = basedir.to_string_lossy().replace('\\', "/");
        let mut file = File::create(&my_ini_path)?;
        let content = format!(
            "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\nport=3306\nbasedir={}\ndatadir={}/data\ncharacter-set-server=utf8mb4\ndefault-storage-engine=INNODB\n",
            basedir_forward, basedir_forward
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}
