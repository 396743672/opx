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
                        name: "清华镜像".to_string(),
                        url: "https://mirrors.tuna.tsinghua.edu.cn/mysql/downloads/mysql-8.4/mysql-8.4.0-winx64.zip".to_string(),
                    },
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/mysql/Downloads/mysql-8.4/mysql-8.4.0-winx64.zip".to_string(),
                    },
                    MirrorSource {
                        name: "官方".to_string(),
                        url: "https://dev.mysql.com/get/Downloads/mysql-8.4/mysql-8.4.0-winx64.zip".to_string(),
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
                        name: "清华镜像".to_string(),
                        url: "https://mirrors.tuna.tsinghua.edu.cn/mysql/downloads/mysql-8.0/mysql-8.0.36-winx64.zip".to_string(),
                    },
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/mysql/Downloads/mysql-8.0/mysql-8.0.36-winx64.zip".to_string(),
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
                        name: "清华镜像".to_string(),
                        url: "https://mirrors.tuna.tsinghua.edu.cn/mysql/downloads/mysql-8.4/mysql-8.4.0-linux-glibc2.28-x86_64.tar.gz".to_string(),
                    },
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/mysql/Downloads/mysql-8.4/mysql-8.4.0-linux-glibc2.28-x86_64.tar.gz".to_string(),
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
        let my_ini_path = ctx.install_dir().join("my.ini");
        let mut file = File::create(&my_ini_path)?;
        let install_path_forward = ctx.install_path.replace('\\', "/");
        let content = format!(
            "[mysql]\ndefault-character-set=utf8mb4\n\n[mysqld]\nport=3306\nbasedir={}\ndatadir={}/data\ncharacter-set-server=utf8mb4\ndefault-storage-engine=INNODB\n",
            install_path_forward, install_path_forward
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}
