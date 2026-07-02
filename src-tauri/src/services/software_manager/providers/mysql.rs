use anyhow::Result;
use std::fs::File;
use std::io::Write;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, BuiltinInfo, CatalogEntry, CatalogVersion, MirrorSource,
    SoftwareCategory,
};
use crate::services::software_manager::providers::builtin_manifest;

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
                mirrors: {
                    let mut m = vec![];
                    let sha = builtin_manifest()
                        .get_builtin("mysql", "8.4.0")
                        .map(|e| e.sha256.clone())
                        .unwrap_or_default();
                    let size = builtin_manifest()
                        .get_builtin("mysql", "8.4.0")
                        .map(|e| e.size)
                        .unwrap_or(0);
                    m.push(MirrorSource {
                        name: "内置默认版本（离线）".to_string(),
                        url: "builtin://software/mysql/8.4.0.zip".to_string(),
                        builtin: Some(BuiltinInfo {
                            version: "8.4.0".to_string(),
                            sha256: sha,
                            size: size,
                        }),
                    });
                    m.push(MirrorSource {
                        name: "MySQL 官方 CDN".to_string(),
                        url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.0-winx64.zip".to_string(),
                        builtin: None,
                    });
                    m
                },
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
                        builtin: None,
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

    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> {
        // 爬 dev.mysql.com/downloads/mysql/ HTML，提取版本号
        // 注意：页面结构可能变更，失败返回 None
        let url = "https://dev.mysql.com/downloads/mysql/";
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .ok()?
            .get(url)
            .header("User-Agent", "OPX")
            .send()
            .ok()?;

        if !response.status().is_success() {
            eprintln!("[mysql] 下载页返回 {}", response.status());
            return None;
        }

        let html = response.text().ok()?;
        let mut versions = vec![];
        let mut seen = std::collections::HashSet::new();

        // 正则匹配 mysql-X.Y.Z-winx64.zip
        let re = regex::Regex::new(r"mysql-(\d+\.\d+\.\d+)-winx64\.zip").ok()?;

        for cap in re.captures_iter(&html) {
            let version = cap.get(1)?.as_str().to_string();
            if seen.contains(&version) {
                continue;
            }
            seen.insert(version.clone());

            // 只取 8.x（最低 8.0.36+，跳过 8.0.36 之前的版本）
            let major: u32 = version
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let minor: u32 = version
                .split('.')
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let patch: u32 = version
                .split('.')
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            if major != 8 {
                continue;
            }
            if minor == 0 && patch < 36 {
                continue;
            }

            // 8.4.x 走 mysql-8.4 路径，8.0.x 走 mysql-8.0 路径
            let path_segment = if minor == 4 {
                "mysql-8.4"
            } else {
                "mysql-8.0"
            };

            versions.push(CatalogVersion {
                version: version.clone(),
                mirrors: vec![MirrorSource {
                    name: "MySQL 官方 CDN".to_string(),
                    url: format!(
                        "https://cdn.mysql.com/archives/{}/mysql-{}-winx64.zip",
                        path_segment, version
                    ),
                    builtin: None,
                }],
                archive: ArchiveInfo {
                    format: ArchiveFormat::Zip,
                    size: None,
                    sha256: None,
                },
            });
        }

        if versions.is_empty() {
            None
        } else {
            Some(versions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_840_has_builtin_as_first_mirror() {
        let entry = MySqlProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "8.4.0")
            .expect("应有 8.4.0 版本");
        assert!(v.mirrors[0].builtin.is_some());
        assert_eq!(v.mirrors[0].builtin.as_ref().unwrap().version, "8.4.0");
    }

    #[test]
    fn mysql_8036_has_no_builtin() {
        let entry = MySqlProvider::new().catalog_entry();
        let v = entry
            .versions
            .iter()
            .find(|v| v.version == "8.0.36")
            .expect("应有 8.0.36 版本");
        for m in &v.mirrors {
            assert!(m.builtin.is_none());
        }
    }

    #[test]
    fn mysql_fetch_remote_versions_method_exists() {
        let provider = MySqlProvider::new();
        let _ = provider.fetch_remote_versions();
    }
}
