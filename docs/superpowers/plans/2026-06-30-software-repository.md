# 软件仓库模块实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 实现软件仓库模块，聚焦"安装新软件"，包括 catalog（内置+远程合并）、多版本共存、JRE 默认版本、流式下载进度推送、自定义上传。

**架构：**
- 后端：Rust `commands/software.rs` 对外暴露 Tauri 命令，`services/software_manager/` 子目录按职责拆分（catalog/installer/providers）
- 前端：Vue 3 `modules/software-manager/` 含卡片网格和安装对话框，通过 Tauri 事件监听进度

**技术栈：** Rust + Tauri 2 + Vue 3 + TypeScript + Tailwind CSS + reqwest + zip + tar + tokio

---

## 文件清单

### 新增文件

**后端：**
- `src-tauri/src/services/software_manager/catalog.rs` - catalog 加载与合并
- `src-tauri/src/services/software_manager/installer.rs` - 安装器核心（下载/解压/登记/清理）
- `src-tauri/src/services/software_manager/providers/mod.rs` - SoftwareProvider trait + 聚合
- `src-tauri/src/services/software_manager/providers/mysql.rs` - MySQL provider
- `src-tauri/src/services/software_manager/providers/jre.rs` - JRE provider
- `src-tauri/src/services/software_manager/providers/redis.rs` - Redis provider
- `src-tauri/src/services/software_manager/providers/nginx.rs` - Nginx provider
- `src-tauri/src/services/software_manager/mod.rs` - 聚合模块（替换空文件）

**前端：**
- `src/modules/software-manager/stores/catalog.ts` - catalog Pinia store
- `src/modules/software-manager/stores/install.ts` - 安装任务 Pinia store
- `src/modules/software-manager/components/SoftwareCard.vue` - 软件卡片
- `src/modules/software-manager/components/InstallDialog.vue` - 安装对话框
- `src/modules/software-manager/components/InstallJreDialog.vue` - JRE 安装对话框
- `src/modules/software-manager/components/InstallProgressDialog.vue` - 安装进度对话框
- `src/modules/software-manager/components/CustomInstallDialog.vue` - 自定义上传对话框

### 修改文件

**后端：**
- `src-tauri/src/models/software.rs` - 扩展数据模型
- `src-tauri/src/models/settings.rs` - 添加 jre_default_id
- `src-tauri/src/utils/download.rs` - 添加流式下载 with_progress
- `src-tauri/src/commands/software.rs` - 添加 Tauri 命令
- `src-tauri/src/lib.rs` - 注册命令 + 管理 State
- `src-tauri/Cargo.toml` - 添加依赖

**前端：**
- `src/locales/zh-CN.ts` - 添加中文文案
- `src/locales/en-US.ts` - 添加英文文案
- `src/modules/software-manager/pages/RepositoryPage.vue` - 替换占位符

---

## 任务列表

### 任务 1：添加依赖

**文件：**
- 修改：`src-tauri/Cargo.toml`

- [ ] **步骤 1：更新 Cargo.toml 依赖**

```toml
[dependencies]
# 现有依赖不变，添加以下
uuid = { version = "1.0", features = ["v4"] }
sha2 = "0.10"
# reqwest 已有，需启用 stream
reqwest = { version = "0.13", features = ["blocking", "json", "stream"] }
tokio = { version = "1.0", features = ["rt-multi-thread", "macros"] }

[dev-dependencies]
mockito = "1.0"
tempfile = "3.0"
```

- [ ] **步骤 2：运行 cargo check 验证依赖**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "feat: 添加软件仓库模块依赖"
```

---

### 任务 2：扩展数据模型

**文件：**
- 修改：`src-tauri/src/models/software.rs`
- 修改：`src-tauri/src/models/settings.rs`

- [ ] **步骤 1：重写 src-tauri/src/models/software.rs**

```rust
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub format: ArchiveFormat,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSource {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogVersion {
    pub version: String,
    pub mirrors: Vec<MirrorSource>,
    pub archive: ArchiveInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareCategory {
    Database,
    Runtime,
    Cache,
    WebServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: SoftwareCategory,
    pub icon: String,
    pub versions: Vec<CatalogVersion>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
}

impl Default for SoftwareStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstallSource {
    Mirror {
        mirror_name: String,
        url: String,
    },
    Custom {
        archive_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub id: String,
    pub key: String,
    pub version: String,
    pub name: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
    pub source: InstallSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftwareList {
    pub software: Vec<InstalledSoftware>,
}

impl Default for InstalledSoftwareList {
    fn default() -> Self {
        Self {
            software: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallParams {
    pub key: String,
    pub version: String,
    pub mirror_index: usize,
    pub set_as_default_jre: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInstallParams {
    pub name: String,
    pub archive_path: String,
}

// 保留原设计中的类型（兼容性），逐步迁移
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub available_versions: Vec<String>,
    pub default_version: String,
}
```

- [ ] **步骤 2：更新 src-tauri/src/models/settings.rs 添加 jre_default_id**

在 AppSettings 结构体中添加：
```rust
pub jre_default_id: Option<String>,
```

在 Default 实现中添加：
```rust
jre_default_id: None,
```

- [ ] **步骤 3：更新 models/tests 中的软件模型测试**

修改 `src-tauri/src/models/mod.rs` 中的 `software_models_support_json_roundtrip` 测试，更新测试数据以匹配新模型结构。

- [ ] **步骤 4：运行测试验证**

运行：`cd src-tauri && cargo test`
预期：所有测试通过

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/models/software.rs src-tauri/src/models/settings.rs
git commit -m "feat: 扩展软件仓库数据模型"
```

---

### 任务 3：扩展下载工具支持流式进度

**文件：**
- 修改：`src-tauri/src/utils/download.rs`

- [ ] **步骤 1：重写 src-tauri/src/utils/download.rs**

```rust
use anyhow::Result;
use reqwest::{self, Response};
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn download(url: &str, destination: &Path) -> Result<()> {
    let response = reqwest::blocking::get(url)?.error_for_status()?;
    let content = response.bytes()?;
    std::fs::write(destination, content)?;
    Ok(())
}

pub async fn download_with_progress<F>(
    url: &str,
    destination: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?;

    let total_size = response.content_length();
    let mut file = File::create(destination)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = futures::TryStreamExt::try_next(&mut stream).await? {
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_size);
    }
    Ok(())
}
```

- [ ] **步骤 2：更新 src-tauri/src/utils/mod.rs 以导出新函数**

确保 mod 有：
```rust
pub mod download;
```

- [ ] **步骤 3：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/utils/download.rs
git commit -m "feat: 添加流式下载 with_progress"
```

---

### 任务 4：实现 providers 子模块

**文件：**
- 创建：`src-tauri/src/services/software_manager/providers/mod.rs`
- 创建：`src-tauri/src/services/software_manager/providers/mysql.rs`
- 创建：`src-tauri/src/services/software_manager/providers/jre.rs`
- 创建：`src-tauri/src/services/software_manager/providers/redis.rs`
- 创建：`src-tauri/src/services/software_manager/providers/nginx.rs`

- [ ] **步骤 1：创建 src-tauri/src/services/software_manager/providers/mod.rs**

```rust
use anyhow::Result;
use super::catalog::{CatalogEntry, CatalogVersion, MirrorSource, ArchiveInfo, ArchiveFormat, SoftwareCategory};
use std::path::Path;

pub mod mysql;
pub mod jre;
pub mod redis;
pub mod nginx;

pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;
}

pub struct InstallContext {
    pub key: String,
    pub version: String,
    pub install_path: String,
}

impl InstallContext {
    pub fn new(key: String, version: String, install_path: String) -> Self {
        Self {
            key,
            version,
            install_path,
        }
    }

    pub fn install_dir(&self) -> &Path {
        Path::new(&self.install_path)
    }
}

pub fn all_providers() -> Vec<Box<dyn SoftwareProvider>> {
    vec![
        Box::new(mysql::MySqlProvider::new()),
        Box::new(jre::JreProvider::new()),
        Box::new(redis::RedisProvider::new()),
        Box::new(nginx::NginxProvider::new()),
    ]
}
```

- [ ] **步骤 2：创建 src-tauri/src/services/software_manager/providers/mysql.rs**

```rust
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use super::{SoftwareProvider, InstallContext};
use super::super::catalog::{CatalogEntry, CatalogVersion, MirrorSource, ArchiveInfo, ArchiveFormat, SoftwareCategory};

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
        let content = format!(
            "[mysql]
default-character-set=utf8mb4

[mysqld]
port=3306
basedir={}
datadir={}/data
character-set-server=utf8mb4
default-storage-engine=INNODB
",
            ctx.install_path.replace('\\', "/"),
            ctx.install_path.replace('\\', "/")
        );
        file.write_all(content.as_bytes())?;
        Ok(())
    }
}
```

- [ ] **步骤 3：创建 src-tauri/src/services/software_manager/providers/jre.rs**

```rust
use anyhow::Result;
use super::{SoftwareProvider, InstallContext};
use super::super::catalog::{CatalogEntry, CatalogVersion, MirrorSource, ArchiveInfo, ArchiveFormat, SoftwareCategory};

pub struct JreProvider;

impl JreProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JreProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for JreProvider {
    fn key(&self) -> &str {
        "jre"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "21.0.2".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/adoptium/releases/21.0.2/OpenJDK21U-jre_x64_windows_hotspot_21.0.2_13.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
            versions.push(CatalogVersion {
                version: "17.0.10".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/adoptium/releases/17.0.10/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
            versions.push(CatalogVersion {
                version: "11.0.22".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/adoptium/releases/11.0.22/OpenJDK11U-jre_x64_windows_hotspot_11.0.22_7.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
            versions.push(CatalogVersion {
                version: "1.8".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "腾讯镜像".to_string(),
                        url: "https://mirrors.cloud.tencent.com/Adoptium/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "21.0.2".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo { format: ArchiveFormat::TarGz, size: None, sha256: None },
            });
        }

        CatalogEntry {
            key: "jre".to_string(),
            name: "JRE".to_string(),
            description: "Java 运行时环境（Eclipse Temurin）".to_string(),
            category: SoftwareCategory::Runtime,
            icon: "mdi:play-circle".to_string(),
            versions,
            default_version: "17.0.10".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
```

- [ ] **步骤 4：创建 src-tauri/src/services/software_manager/providers/redis.rs**

```rust
use anyhow::Result;
use super::{SoftwareProvider, InstallContext};
use super::super::catalog::{CatalogEntry, CatalogVersion, MirrorSource, ArchiveInfo, ArchiveFormat, SoftwareCategory};

pub struct RedisProvider;

impl RedisProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RedisProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for RedisProvider {
    fn key(&self) -> &str {
        "redis"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "7.4.0".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/redis-for-windows/7.4.0/redis-7.4.0-windows-x64.zip".to_string(),
                    },
                    MirrorSource {
                        name: "GitHub Releases".to_string(),
                        url: "https://github.com/redis/redis-windows/releases/download/7.4.0/redis-7.4.0-windows-x64.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
            versions.push(CatalogVersion {
                version: "7.2.4".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/redis-for-windows/7.2.4/redis-7.2.4-windows-x64.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "7.4.0".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo { format: ArchiveFormat::TarGz, size: None, sha256: None },
            });
        }

        CatalogEntry {
            key: "redis".to_string(),
            name: "Redis".to_string(),
            description: "内存键值存储".to_string(),
            category: SoftwareCategory::Cache,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "7.4.0".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
```

- [ ] **步骤 5：创建 src-tauri/src/services/software_manager/providers/nginx.rs**

```rust
use anyhow::Result;
use super::{SoftwareProvider, InstallContext};
use super::super::catalog::{CatalogEntry, CatalogVersion, MirrorSource, ArchiveInfo, ArchiveFormat, SoftwareCategory};

pub struct NginxProvider;

impl NginxProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NginxProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareProvider for NginxProvider {
    fn key(&self) -> &str {
        "nginx"
    }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];

        #[cfg(windows)]
        {
            versions.push(CatalogVersion {
                version: "1.26.1".to_string(),
                mirrors: vec![
                    MirrorSource {
                        name: "华为镜像".to_string(),
                        url: "https://mirrors.huaweicloud.com/nginx/nginx-1.26.1/nginx-1.26.1.zip".to_string(),
                    },
                    MirrorSource {
                        name: "官方".to_string(),
                        url: "https://nginx.org/download/nginx-1.26.1.zip".to_string(),
                    },
                ],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }

        #[cfg(unix)]
        {
            versions.push(CatalogVersion {
                version: "1.26.1".to_string(),
                mirrors: vec![],
                archive: ArchiveInfo { format: ArchiveFormat::TarGz, size: None, sha256: None },
            });
        }

        CatalogEntry {
            key: "nginx".to_string(),
            name: "Nginx".to_string(),
            description: "高性能 HTTP 服务器与反向代理".to_string(),
            category: SoftwareCategory::WebServer,
            icon: "mdi:web".to_string(),
            versions,
            default_version: "1.26.1".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        Ok(())
    }
}
```

- [ ] **步骤 6：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 7：Commit**

```bash
git add src-tauri/src/services/software_manager/providers/mod.rs
git add src-tauri/src/services/software_manager/providers/mysql.rs
git add src-tauri/src/services/software_manager/providers/jre.rs
git add src-tauri/src/services/software_manager/providers/redis.rs
git add src-tauri/src/services/software_manager/providers/nginx.rs
git commit -m "feat: 实现软件 provider 子模块"
```

---

### 任务 5：实现 catalog 模块

**文件：**
- 创建：`src-tauri/src/services/software_manager/catalog.rs`

- [ ] **步骤 1：创建 src-tauri/src/services/software_manager/catalog.rs**

```rust
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::models::settings::AppSettings;
use crate::utils::paths;
use super::providers::{all_providers, SoftwareProvider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub format: ArchiveFormat,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSource {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogVersion {
    pub version: String,
    pub mirrors: Vec<MirrorSource>,
    pub archive: ArchiveInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareCategory {
    Database,
    Runtime,
    Cache,
    WebServer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub key: String,
    pub name: String,
    pub description: String,
    pub category: SoftwareCategory,
    pub icon: String,
    pub versions: Vec<CatalogVersion>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub updated_at: Option<String>,
}

pub fn build_builtin_catalog() -> Catalog {
    let entries = all_providers()
        .into_iter()
        .map(|p| p.catalog_entry())
        .collect();
    Catalog {
        entries,
        updated_at: None,
    }
}

async fn fetch_remote_catalog(mirror_url: &str) -> Result<Option<Catalog>> {
    let url = format!("{}/catalog.json", mirror_url.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(&url)
        .send()
        .await;

    match response {
        Ok(r) if r.status().is_success() => {
            let catalog = r.json::<Catalog>().await?;
            Ok(Some(catalog))
        }
        _ => Ok(None),
    }
}

pub fn merge_catalogs(builtin: Catalog, remote: Option<Catalog>) -> Catalog {
    let remote = match remote {
        Some(r) => r,
        None => return builtin,
    };

    let mut builtin_map: HashMap<_, _> = builtin.entries.into_iter().map(|e| (e.key.clone(), e)).collect();

    for remote_entry in remote.entries {
        builtin_map.insert(remote_entry.key.clone(), remote_entry);
    }

    Catalog {
        entries: builtin_map.into_values().collect(),
        updated_at: remote.updated_at,
    }
}
```

- [ ] **步骤 2：在 services/software_manager/providers/mod.rs 中引用正确的类型**

更新 providers/mod.rs，确保引用同一 catalog 模块的类型，而非模型模块的重复定义。

- [ ] **步骤 3：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/services/software_manager/catalog.rs
git commit -m "feat: 实现 catalog 模块"
```

---

### 任务 6：实现 software_manager 聚合模块

**文件：**
- 创建：`src-tauri/src/services/software_manager/mod.rs`（替换空文件）

- [ ] **步骤 1：删除空文件并创建新的 src-tauri/src/services/software_manager/mod.rs**

```rust
pub mod catalog;
pub mod installer;
pub mod providers;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::path::Path;
use crate::models::software::{InstalledSoftware, InstalledSoftwareList, Catalog, CatalogEntry};
use crate::models::settings::AppSettings;
use crate::utils::paths;
use anyhow::Result;

pub struct SoftwareManager {
    catalog: RwLock<Catalog>,
    installed: RwLock<InstalledSoftwareList>,
    install_tasks: Mutex<HashMap<String, InstallTaskState>>,
}

pub struct InstallTaskState {
    pub key: String,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Local>,
}

impl SoftwareManager {
    pub fn new() -> Self {
        let builtin = catalog::build_builtin_catalog();
        let installed = Self::load_installed_list().unwrap_or_default();
        Self {
            catalog: RwLock::new(builtin),
            installed: RwLock::new(installed),
            install_tasks: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_catalog(&self) -> Catalog {
        self.catalog.read().unwrap().clone()
    }

    pub fn get_installed(&self) -> Vec<InstalledSoftware> {
        self.installed.read().unwrap().software.clone()
    }

    pub fn is_installed(&self, key: &str, version: &str) -> bool {
        let installed = self.installed.read().unwrap();
        installed.software.iter().any(|s| s.key == key && s.version == version)
    }

    pub fn is_installing(&self, key: &str, version: &str) -> bool {
        let tasks = self.install_tasks.lock().unwrap();
        tasks.values().any(|t| t.key == key && t.version == version)
    }

    pub fn add_install_task(&self, install_id: String, key: String, version: String) {
        let mut tasks = self.install_tasks.lock().unwrap();
        tasks.insert(install_id, InstallTaskState {
            key,
            version,
            created_at: chrono::Local::now(),
        });
    }

    pub fn remove_install_task(&self, install_id: &str) {
        let mut tasks = self.install_tasks.lock().unwrap();
        tasks.remove(install_id);
    }

    pub fn add_installed(&self, software: InstalledSoftware) -> Result<()> {
        let mut installed = self.installed.write().unwrap();
        installed.software.push(software);
        Self::save_installed_list(&installed)?;
        Ok(())
    }

    fn load_installed_list() -> Result<InstalledSoftwareList> {
        let path = paths::config_dir().join("installed.json");
        if !path.exists() {
            return Ok(InstalledSoftwareList::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let list = serde_json::from_str(&content)?;
        Ok(list)
    }

    fn save_installed_list(list: &InstalledSoftwareList) -> Result<()> {
        let path = paths::config_dir().join("installed.json");
        let content = serde_json::to_string_pretty(list)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn update_jre_default(&self, jre_id: Option<String>) -> Result<()> {
        let sp = paths::settings_path();
        let mut settings = if sp.exists() {
            let content = std::fs::read_to_string(&sp)?;
            serde_json::from_str::<AppSettings>(&content).unwrap_or_default()
        } else {
            AppSettings::default()
        };
        settings.jre_default_id = jre_id;
        let tmp = sp.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(&settings)?;
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &sp)?;
        Ok(())
    }

    pub fn get_jre_default(&self) -> Option<String> {
        let sp = paths::settings_path();
        if !sp.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&sp).ok()?;
        let settings = serde_json::from_str::<AppSettings>(&content).ok()?;
        settings.jre_default_id
    }
}

impl Default for SoftwareManager {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **步骤 2：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/src/services/software_manager/mod.rs
git commit -m "feat: 实现 software_manager 聚合模块"
```

---

### 任务 7：实现 installer 核心模块

**文件：**
- 创建：`src-tauri/src/services/software_manager/installer.rs`

- [ ] **步骤 1：创建 src-tauri/src/services/software_manager/installer.rs**

```rust
use anyhow::Result;
use sha2::{Sha256, Digest};
use std::fs;
use std::io::Read;
use std::path::Path;
use tauri::AppHandle;
use crate::models::software::{InstallParams, CustomInstallParams, InstalledSoftware, SoftwareStatus, InstallSource};
use crate::services::software_manager::SoftwareManager;
use crate::services::software_manager::providers::{all_providers, InstallContext};
use crate::utils::paths;
use crate::utils::download;
use crate::utils::archive;
use chrono::Utc;

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

fn cleanup_path(path: &Path) {
    if path.exists() {
        if let Err(e) = fs::remove_dir_all(path) {
            log::warn!("清理 {} 失败: {}", path.display(), e);
        }
    }
}

fn cleanup_file(path: &Path) {
    if path.exists() {
        if let Err(e) = fs::remove_file(path) {
            log::warn!("删除临时文件 {} 失败: {}", path.display(), e);
        }
    }
}

pub async fn install_software(
    app: AppHandle,
    manager: std::sync::Arc<tauri::State<SoftwareManager>>,
    params: InstallParams,
    install_id: String,
) {
    let tmp_path = paths::tmp_dir().join(format!("{}-{}.tmp", params.key, params.version));
    let install_path = paths::apps_dir().join(&params.key).join(&params.version);

    let catalog = manager.get_catalog();
    let entry = match catalog.entries.iter().find(|e| e.key == params.key) {
        Some(e) => e,
        None => {
            app.emit("install-progress", serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "未知软件"
            })).ok();
            return;
        }
    };
    let version_info = match entry.versions.iter().find(|v| v.version == params.version) {
        Some(v) => v,
        None => {
            app.emit("install-progress", serde_json::json!({
                "install_id": install_id,
                "phase": "failed",
                "error": "不支持该版本"
            })).ok();
            return;
        }
    };
    if params.mirror_index >= version_info.mirrors.len() {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "镜像索引无效"
        })).ok();
        return;
    }
    let mirror = &version_info.mirrors[params.mirror_index];

    if manager.is_installed(&params.key, &params.version) {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "该版本已安装"
        })).ok();
        return;
    }
    if manager.is_installing(&params.key, &params.version) {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "该版本正在安装中"
        })).ok();
        return;
    }

    if let Err(e) = fs::create_dir_all(&install_path) {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": format!("创建安装目录失败: {}", e)
        })).ok();
        return;
    }

    manager.add_install_task(install_id.clone(), params.key.clone(), params.version.clone());

    let result: Result<()> = async {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "downloading",
            "downloaded": 0,
            "total": serde_json::Value::Null,
            "percent": serde_json::Value::Null
        })).ok();

        let app_clone = app.clone();
        let install_id_clone = install_id.clone();
        download::download_with_progress(&mirror.url, &tmp_path, move |downloaded, total| {
            let percent = total.map(|t| (downloaded as f64 / t as f64 * 100.0) as i32);
            app_clone.emit("install-progress", serde_json::json!({
                "install_id": install_id_clone.clone(),
                "phase": "downloading",
                "downloaded": downloaded,
                "total": total,
                "percent": percent
            })).ok();
        }).await?;

        if let Some(expected_sha) = &version_info.archive.sha256 {
            let computed = compute_sha256(&tmp_path)?;
            if computed != expected_sha.to_lowercase() {
                return Err(anyhow::anyhow!("SHA256 校验失败"));
            }
        }

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 0
        })).ok();

        match version_info.archive.format {
            super::catalog::ArchiveFormat::Zip => archive::extract_zip(&tmp_path, &install_path)?,
            super::catalog::ArchiveFormat::TarGz => archive::extract_tar_gz(&tmp_path, &install_path)?,
        }

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 50
        })).ok();

        if let Some(provider) = all_providers().into_iter().find(|p| p.key() == params.key) {
            let ctx = InstallContext::new(
                params.key.clone(),
                params.version.clone(),
                install_path.to_string_lossy().to_string(),
            );
            provider.post_install(&ctx)?;
        }

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 100
        })).ok();

        let installed_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();
        let installed = InstalledSoftware {
            id: installed_id.clone(),
            key: params.key.clone(),
            version: params.version.clone(),
            name: format!("{} {}", entry.name, params.version),
            install_path: install_path.to_string_lossy().to_string(),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: false,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Mirror {
                mirror_name: mirror.name.clone(),
                url: mirror.url.clone(),
            },
        };
        manager.add_installed(installed)?;

        if params.key == "jre" && params.set_as_default_jre {
            manager.update_jre_default(Some(installed_id.clone()))?;
        }

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "completed",
            "installed_id": installed_id
        })).ok();

        Ok(())
    }.await;

    if let Err(e) = result {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "failed",
            "error": format!("{}", e)
        })).ok();
        cleanup_file(&tmp_path);
        cleanup_path(&install_path);
    } else {
        cleanup_file(&tmp_path);
    }

    manager.remove_install_task(&install_id);
}

pub async fn install_custom(
    app: AppHandle,
    manager: std::sync::Arc<tauri::State<SoftwareManager>>,
    params: CustomInstallParams,
    install_id: String,
) {
    let install_path = paths::apps_dir().join("custom").join(&params.name);
    let archive_path = Path::new(&params.archive_path);

    if !archive_path.exists() {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "压缩包不存在"
        })).ok();
        return;
    }

    let ext = archive_path.extension().and_then(|s| s.to_str()).unwrap_or_default();
    let ext_lower = ext.to_lowercase();
    if ext_lower != "zip" && ext_lower != "gz" && !params.archive_path.ends_with(".tar.gz") {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "仅支持 zip 和 tar.gz 格式"
        })).ok();
        return;
    }

    if manager.is_installed("custom", &params.name) {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": "该名称已存在"
        })).ok();
        return;
    }

    if let Err(e) = fs::create_dir_all(&install_path) {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id,
            "phase": "failed",
            "error": format!("创建安装目录失败: {}", e)
        })).ok();
        return;
    }

    let result: Result<()> = async {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 0
        })).ok();

        if params.archive_path.ends_with(".zip") {
            archive::extract_zip(archive_path, &install_path)?;
        } else {
            archive::extract_tar_gz(archive_path, &install_path)?;
        }

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "extracting",
            "percent": 100
        })).ok();

        let installed_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();
        let installed = InstalledSoftware {
            id: installed_id.clone(),
            key: params.name.clone(),
            version: "".to_string(),
            name: params.name.clone(),
            install_path: install_path.to_string_lossy().to_string(),
            install_time: now,
            status: SoftwareStatus::Unknown,
            port: 0,
            config: serde_json::json!({}),
            is_custom: true,
            auto_start_on_app_start: false,
            startup_order: 0,
            source: InstallSource::Custom {
                archive_name: archive_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
            },
        };
        manager.add_installed(installed)?;

        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "completed",
            "installed_id": installed_id
        })).ok();

        Ok(())
    }.await;

    if let Err(e) = result {
        app.emit("install-progress", serde_json::json!({
            "install_id": install_id.clone(),
            "phase": "failed",
            "error": format!("{}", e)
        })).ok();
        cleanup_path(&install_path);
    }
}
```

- [ ] **步骤 2：更新 services/software_manager/mod.rs 导出 installer**

在 services/software_manager/mod.rs 中添加：
```rust
pub mod installer;
```

- [ ] **步骤 3：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/services/software_manager/installer.rs
git commit -m "feat: 实现 installer 核心模块"
```

---

### 任务 8：实现 Tauri 命令层

**文件：**
- 修改：`src-tauri/src/commands/software.rs`

- [ ] **步骤 1：重写 src-tauri/src/commands/software.rs**

```rust
use tauri::AppHandle;
use crate::models::software::{CatalogEntry, InstalledSoftware, InstallParams, CustomInstallParams};
use crate::services::software_manager::SoftwareManager;
use crate::services::software_manager::catalog;
use crate::services::software_manager::installer;

#[tauri::command]
pub async fn list_available_software(
    manager: tauri::State<'_, SoftwareManager>,
) -> Vec<CatalogEntry> {
    manager.get_catalog().entries
}

#[tauri::command]
pub async fn refresh_catalog(
    manager: tauri::State<'_, SoftwareManager>,
    app: AppHandle,
) -> Vec<CatalogEntry> {
    let builtin = catalog::build_builtin_catalog();
    let remote = None;
    let merged = catalog::merge_catalogs(builtin, remote);
    *manager.catalog.write().unwrap() = merged;
    manager.get_catalog().entries
}

#[tauri::command]
pub async fn list_installed_software(
    manager: tauri::State<'_, SoftwareManager>,
) -> Vec<InstalledSoftware> {
    manager.get_installed()
}

#[tauri::command]
pub async fn install_software(
    manager: tauri::State<'_, SoftwareManager>,
    app: AppHandle,
    params: InstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let arc_manager = std::sync::Arc::new(manager);
    tauri::async_runtime::spawn(async move {
        installer::install_software(app, arc_manager, params, install_id.clone()).await;
    });
    Ok(install_id)
}

#[tauri::command]
pub async fn install_custom(
    manager: tauri::State<'_, SoftwareManager>,
    app: AppHandle,
    params: CustomInstallParams,
) -> Result<String, String> {
    let install_id = uuid::Uuid::new_v4().to_string();
    let arc_manager = std::sync::Arc::new(manager);
    tauri::async_runtime::spawn(async move {
        installer::install_custom(app, arc_manager, params, install_id.clone()).await;
    });
    Ok(install_id)
}
```

- [ ] **步骤 2：在 src-tauri/src/commands/mod.rs 导出 software 模块**

确保 commands/mod.rs 有：
```rust
pub mod software;
```

- [ ] **步骤 3：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/commands/software.rs
git commit -m "feat: 实现软件仓库 Tauri 命令层"
```

---

### 任务 9：注册命令与 State 管理

**文件：**
- 修改：`src-tauri/src/lib.rs`

- [ ] **步骤 1：更新 lib.rs setup 中初始化 SoftwareManager State**

在 setup 闭包开始处（在创建托盘之前）添加：
```rust
app.manage(crate::services::software_manager::SoftwareManager::new());
```

- [ ] **步骤 2：更新 lib.rs invoke_handler 注册新命令**

在 tauri::generate_handler! 列表中添加：
```rust
commands::software::list_available_software,
commands::software::refresh_catalog,
commands::software::list_installed_software,
commands::software::install_software,
commands::software::install_custom,
```

- [ ] **步骤 3：运行 cargo check 验证**

运行：`cd src-tauri && cargo check`
预期：无报错

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: 注册软件仓库命令与 State"
```

---

### 任务 10：添加国际化文案

**文件：**
- 修改：`src/locales/zh-CN.ts`
- 修改：`src/locales/en-US.ts`

- [ ] **步骤 1：更新 src/locales/zh-CN.ts 添加软件仓库相关文案**

在 export default 对象中添加：
```typescript
  // 软件仓库新增
  softwareRepository: "软件仓库",
  installNewSoftware: "安装新软件",
  selectVersion: "选择版本",
  selectMirror: "选择镜像源",
  installPath: "安装路径",
  setAsDefaultJre: "设为全局默认 JRE",
  defaultJre: "默认 JRE",
  installed: "已安装",
  installing: "安装中",
  downloading: "下载中",
  extracting: "解压中",
  installCompleted: "安装完成",
  installFailed: "安装失败",
  uploadCustom: "上传自定义压缩包",
  customName: "名称",
  selectArchive: "选择压缩包",
  supportedFormats: "支持 zip、tar.gz 格式",
  refreshCatalog: "刷新目录",
  catalogUpdateFailed: "目录更新失败",
  versionAvailable: "可选 {} 个版本",
  canUpdate: "可更新",
  latestVersion: "最新",
  retry: "重试",
```

- [ ] **步骤 2：更新 src/locales/en-US.ts 添加英文翻译**

```typescript
  softwareRepository: "Software Repository",
  installNewSoftware: "Install New Software",
  selectVersion: "Select Version",
  selectMirror: "Select Mirror",
  installPath: "Install Path",
  setAsDefaultJre: "Set as Global Default JRE",
  defaultJre: "Default JRE",
  installed: "Installed",
  installing: "Installing",
  downloading: "Downloading",
  extracting: "Extracting",
  installCompleted: "Install Completed",
  installFailed: "Install Failed",
  uploadCustom: "Upload Custom Archive",
  customName: "Name",
  selectArchive: "Select Archive",
  supportedFormats: "Supported formats: zip, tar.gz",
  refreshCatalog: "Refresh Catalog",
  catalogUpdateFailed: "Catalog Update Failed",
  versionAvailable: "{} versions available",
  canUpdate: "Can Update",
  latestVersion: "Latest",
  retry: "Retry",
```

- [ ] **步骤 3：运行 npm run build 验证**

运行：`npm run build`
预期：无 TypeScript 错误

- [ ] **步骤 4：Commit**

```bash
git add src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat: 添加软件仓库国际化文案"
```

---

### 任务 11：实现前端组件与页面

**文件：**
- 修改：`src/modules/software-manager/pages/RepositoryPage.vue`
- 创建：`src/modules/software-manager/components/SoftwareCard.vue`
- 创建：`src/modules/software-manager/components/InstallDialog.vue`
- 创建：`src/modules/software-manager/components/InstallJreDialog.vue`
- 创建：`src/modules/software-manager/components/InstallProgressDialog.vue`
- 创建：`src/modules/software-manager/components/CustomInstallDialog.vue`
- 创建：`src/modules/software-manager/stores/catalog.ts`
- 创建：`src/modules/software-manager/stores/install.ts`

- [ ] **步骤 1：创建 src/modules/software-manager/stores/catalog.ts**

```typescript
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

type SoftwareCategory = 'Database' | 'Runtime' | 'Cache' | 'WebServer'

type MirrorSource = {
  name: string
  url: string
}

type ArchiveFormat = 'Zip' | 'TarGz'

type ArchiveInfo = {
  format: ArchiveFormat
  size: number | null
  sha256: string | null
}

type CatalogVersion = {
  version: string
  mirrors: MirrorSource[]
  archive: ArchiveInfo
}

type CatalogEntry = {
  key: string
  name: string
  description: string
  category: SoftwareCategory
  icon: string
  versions: CatalogVersion[]
  default_version: string
}

export const useCatalogStore = defineStore('catalog', () => {
  const entries = ref<CatalogEntry[]>([])
  const loading = ref(false)

  const groupedEntries = computed(() => {
    const groups: Record<SoftwareCategory, CatalogEntry[]> = {
      Database: [],
      Runtime: [],
      Cache: [],
      WebServer: [],
    }
    entries.value.forEach(e => {
      if (groups[e.category]) {
        groups[e.category].push(e)
      }
    })
    return groups
  })

  async function loadCatalog() {
    loading.value = true
    try {
      entries.value = await invoke('list_available_software') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to load catalog:', e)
    } finally {
      loading.value = false
    }
  }

  async function refreshCatalog() {
    loading.value = true
    try {
      entries.value = await invoke('refresh_catalog') as CatalogEntry[]
    } catch (e) {
      console.error('Failed to refresh catalog:', e)
    } finally {
      loading.value = false
    }
  }

  return {
    entries,
    loading,
    groupedEntries,
    loadCatalog,
    refreshCatalog,
  }
})
```

- [ ] **步骤 2：创建 src/modules/software-manager/stores/install.ts**

```typescript
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { listen } from '@tauri-apps/api/event'

type InstallPhase = 'downloading' | 'extracting' | 'completed' | 'failed'

type InstallTask = {
  id: string
  key: string
  name: string
  phase: InstallPhase
  downloaded: number
  total: number | null
  percent: number | null
  error: string | null
  installedId: string | null
}

export const useInstallStore = defineStore('install', () => {
  const tasks = ref<Record<string, InstallTask>>({})

  const activeTasks = computed(() => {
    return Object.values(tasks.value).filter(t =>
      t.phase === 'downloading' || t.phase === 'extracting'
    )
  })

  function hasActiveTask(key: string, version?: string): boolean {
    return activeTasks.value.some(t =>
      t.key === key && (version == null || t.name.includes(version))
    )
  }

  function createTask(id: string, key: string, name: string) {
    tasks.value[id] = {
      id,
      key,
      name,
      phase: 'downloading',
      downloaded: 0,
      total: null,
      percent: null,
      error: null,
      installedId: null,
    }
  }

  function updateTask(id: string, payload: any) {
    const task = tasks.value[id]
    if (!task) return

    task.phase = payload.phase
    if (payload.phase === 'downloading') {
      task.downloaded = payload.downloaded
      task.total = payload.total
      task.percent = payload.percent
    } else if (payload.phase === 'extracting') {
      task.percent = payload.percent
    } else if (payload.phase === 'completed') {
      task.installedId = payload.installed_id
      setTimeout(() => delete tasks.value[id], 3000)
    } else if (payload.phase === 'failed') {
      task.error = payload.error
    }
  }

  let unlisten: (() => void) | null = null
  async function initEvents() {
    if (unlisten) unlisten()
    unlisten = await listen('install-progress', (event) => {
      const payload = event.payload as any
      const id = payload.install_id
      if (tasks.value[id]) {
        updateTask(id, payload)
      }
    })
  }

  function cleanup() {
    if (unlisten) unlisten()
    unlisten = null
  }

  return {
    tasks,
    activeTasks,
    hasActiveTask,
    createTask,
    updateTask,
    initEvents,
    cleanup,
  }
})
```

- [ ] **步骤 3：创建 src/modules/software-manager/components/SoftwareCard.vue**

```vue
<template>
  <div class="card sw-card">
    <div class="sw-card-top">
      <div class="sw-icon">
        <Icon :icon="entry.icon" />
      </div>
      <div class="sw-meta">
        <div class="sw-name">
          {{ entry.name }}
          <span v-if="entry.key === 'jre' || entry.key === 'nginx'" class="sw-tag">LTS</span>
        </div>
        <div class="sw-desc">{{ entry.description }}</div>
      </div>
    </div>
    <div class="sw-installed">
      <span v-if="isInstalled(entry.default_version)" class="pill">
        <span class="dot"></span> 已装 {{ entry.default_version }}
      </span>
      <span v-if="entry.key === 'jre' && isDefaultJre" class="sw-default">
        <Icon icon="mdi:star" /> 默认
      </span>
    </div>
    <div class="sw-footer">
      <span class="sw-versions">可选 <b>{{ entry.versions.length }}</b> 个版本</span>
      <button
        class="btn primary"
        @click="openInstall"
        :disabled="isInstalling"
      >
        <Icon icon="mdi:download" /> 安装
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { useCatalogStore } from '../stores/catalog'
import { useInstallStore } from '../stores/install'

const props = defineProps<{
  entry: any
  installedIds: Set<string>
  defaultJreId: string | null
}>()

const emit = defineEmits<{
  install: [entry: any]
}>()

function isInstalled(version: string): boolean {
  const key = `${props.entry.key}-${version}`
  return props.installedIds.has(key)
}

const isDefaultJre = computed(() => {
  return props.entry.key === 'jre' && props.defaultJreId != null
})

const isInstalling = computed(() => {
  const installStore = useInstallStore()
  return installStore.hasActiveTask(props.entry.key)
})

function openInstall() {
  emit('install', props.entry)
}
</script>

<style scoped>
.sw-card {
  border: 1px solid var(--border);
  background: var(--card);
  border-radius: var(--radius-lg);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  box-shadow: var(--shadow-card);
}
.sw-card:hover {
  border-color: color-mix(in oklch, var(--primary) 40%, var(--border));
}
.sw-card-top {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}
.sw-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
}
.sw-meta {
  flex: 1;
  min-width: 0;
}
.sw-name {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 6px;
}
.sw-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--success) 14%, transparent);
  color: var(--success);
}
.sw-desc {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-top: 2px;
}
.sw-installed {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--muted-foreground);
  min-height: 18px;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: 999px;
  background: color-mix(in oklch, var(--success) 10%, transparent);
  color: var(--success);
  border: 1px solid color-mix(in oklch, var(--success) 25%, transparent);
}
.pill .dot {
  width: 5px;
  height: 5px;
  border-radius: 999px;
  background: var(--success);
}
.sw-default {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 7px;
  border-radius: 999px;
  font-size: 10px;
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
  border: 1px solid color-mix(in oklch, var(--primary) 30%, transparent);
}
.sw-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 2px;
}
.sw-versions {
  font-size: 11px;
  color: var(--muted-foreground);
}
.sw-versions b {
  color: var(--foreground);
  font-weight: 500;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--foreground);
  transition: background 0.15s;
}
.btn:hover {
  background: var(--muted);
}
.btn.primary {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--primary) 88%, var(--background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **步骤 4：创建 src/modules/software-manager/components/InstallDialog.vue**

```vue
<template>
  <div class="overlay" @click.self="cancel">
    <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:package-variant-closed" />
          安装 {{ entry.name }}
        </div>
        <button class="dialog-close" @click="cancel">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label">版本</div>
        <div class="version-list">
          <div
            v-for="(v, idx) in entry.versions"
            :key="v.version"
            class="version-row"
            :class="{ selected: selectedVersionIdx === idx }"
            @click="selectedVersionIdx = idx"
          >
            <span class="v-name">{{ v.version }}</span>
            <span v-if="idx === 0" class="v-badge">LTS</span>
            <Icon v-if="selectedVersionIdx === idx" icon="mdi:check" class="v-check" />
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">镜像源</div>
        <div class="select" @click="showMirrorDropdown = !showMirrorDropdown">
          <span>{{ selectedMirrorName }}</span>
          <Icon icon="mdi:chevron-down" class="caret" />
        </div>
        <div v-if="showMirrorDropdown" class="dropdown">
          <div
            v-for="(m, idx) in selectedVersion.mirrors"
            :key="idx"
            class="dropdown-item"
            :class="{ selected: selectedMirrorIdx === idx }"
            @click="selectMirror(idx)"
          >
            {{ m.name }}
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">安装路径 <span class="hint">自动派生</span></div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="cancel">取消</button>
        <button class="btn primary" @click="install" :disabled="installing">
          开始安装
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useInstallStore } from '../stores/install'

const props = defineProps<{
  entry: any
}>()

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const selectedVersionIdx = ref(0)
const selectedMirrorIdx = ref(0)
const showMirrorDropdown = ref(false)
const installing = ref(false)

const selectedVersion = computed(() => props.entry.versions[selectedVersionIdx.value])
const selectedMirrorName = computed(() => selectedVersion.value.mirrors[selectedMirrorIdx.value].name)
const installPath = computed(() => `apps/${props.entry.key}/${selectedVersion.value.version}`)

function selectMirror(idx: number) {
  selectedMirrorIdx.value = idx
  showMirrorDropdown.value = false
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_software', {
      params: {
        key: props.entry.key,
        version: selectedVersion.value.version,
        mirror_index: selectedMirrorIdx.value,
        set_as_default_jre: false,
      },
    }) as string
    useInstallStore().createTask(installId, props.entry.key, `${props.entry.name} ${selectedVersion.value.version}`)
    emit('installed', installId)
  } catch (e) {
    console.error('Failed to install:', e)
  } finally {
    installing.value = false
  }
}

function cancel() {
  emit('cancel')
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--muted);
  color: var(--foreground);
}
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.hint {
  font-size: 11px;
}
.select {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
}
.select:focus {
  border-color: var(--primary);
  background: var(--card);
}
.caret {
  color: var(--muted-foreground);
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
}
.input.readonly {
  color: var(--muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.version-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.version-row:hover {
  background: var(--muted);
}
.version-row.selected {
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
}
.v-name {
  flex: 1;
  font-size: 13px;
}
.v-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--info) 14%, transparent);
  color: var(--info);
}
.v-check {
  width: 16px;
  height: 16px;
}
.dropdown {
  position: relative;
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-top: 4px;
  box-shadow: var(--shadow-popover);
}
.dropdown-item {
  padding: 8px 12px;
  cursor: pointer;
  font-size: 13px;
}
.dropdown-item:hover {
  background: var(--muted);
}
.dropdown-item.selected {
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--foreground);
}
.btn:hover {
  background: var(--muted);
}
.btn.primary {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--primary) 88%, var(--background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **步骤 5：创建 src/modules/software-manager/components/InstallJreDialog.vue**

```vue
<template>
  <div class="overlay" @click.self="cancel">
    <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:play-circle" />
          安装 JRE
        </div>
        <button class="dialog-close" @click="cancel">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label">版本 <span class="hint">均为 LTS</span></div>
        <div class="version-list">
          <div
            v-for="(v, idx) in entry.versions"
            :key="v.version"
            class="version-row"
            :class="{ selected: selectedVersionIdx === idx }"
            @click="selectedVersionIdx = idx"
          >
            <span class="v-name">{{ v.version }}</span>
            <span v-if="idx === 1" class="v-badge">推荐</span>
            <Icon v-if="selectedVersionIdx === idx" icon="mdi:check" class="v-check" />
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">镜像源</div>
        <div class="select" @click="showMirrorDropdown = !showMirrorDropdown">
          <span>{{ selectedMirrorName }}</span>
          <Icon icon="mdi:chevron-down" class="caret" />
        </div>
        <div v-if="showMirrorDropdown" class="dropdown">
          <div
            v-for="(m, idx) in selectedVersion.mirrors"
            :key="idx"
            class="dropdown-item"
            :class="{ selected: selectedMirrorIdx === idx }"
            @click="selectMirror(idx)"
          >
            {{ m.name }}
          </div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">安装路径 <span class="hint">自动派生</span></div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <label class="checkbox">
        <input type="checkbox" v-model="setAsDefault" />
        <span>设为全局默认 JRE</span>
      </label>

      <div class="dialog-footer">
        <button class="btn" @click="cancel">取消</button>
        <button class="btn primary" @click="install" :disabled="installing">
          开始安装
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { invoke } from '@tauri-apps/api/core'
import { useInstallStore } from '../stores/install'

const props = defineProps<{
  entry: any
}>()

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const selectedVersionIdx = ref(1)
const selectedMirrorIdx = ref(0)
const showMirrorDropdown = ref(false)
const installing = ref(false)
const setAsDefault = ref(true)

const selectedVersion = computed(() => props.entry.versions[selectedVersionIdx.value])
const selectedMirrorName = computed(() => selectedVersion.value.mirrors[selectedMirrorIdx.value].name)
const installPath = computed(() => `apps/jre/${selectedVersion.value.version}`)

function selectMirror(idx: number) {
  selectedMirrorIdx.value = idx
  showMirrorDropdown.value = false
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_software', {
      params: {
        key: 'jre',
        version: selectedVersion.value.version,
        mirror_index: selectedMirrorIdx.value,
        set_as_default_jre: setAsDefault.value,
      },
    }) as string
    useInstallStore().createTask(installId, 'jre', `JRE ${selectedVersion.value.version}`)
    emit('installed', installId)
  } catch (e) {
    console.error('Failed to install:', e)
  } finally {
    installing.value = false
  }
}

function cancel() {
  emit('cancel')
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--muted);
  color: var(--foreground);
}
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.hint {
  font-size: 11px;
}
.select {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--foreground);
  font-size: 13px;
  display: flex;
  align-items: center;
}
.input.readonly {
  color: var(--muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.version-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.version-row:hover {
  background: var(--muted);
}
.version-row.selected {
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
}
.v-name {
  flex: 1;
  font-size: 13px;
}
.v-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
  background: color-mix(in oklch, var(--info) 14%, transparent);
  color: var(--info);
}
.v-check {
  width: 16px;
  height: 16px;
}
.dropdown {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 6px;
  margin-top: 4px;
  box-shadow: var(--shadow-popover);
}
.dropdown-item {
  padding: 8px 12px;
  cursor: pointer;
  font-size: 13px;
}
.dropdown-item:hover {
  background: var(--muted);
}
.dropdown-item.selected {
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
}
.checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 13px;
  padding: 8px 0;
}
.checkbox input {
  width: 14px;
  height: 14px;
  accent-color: var(--primary);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--foreground);
}
.btn:hover {
  background: var(--muted);
}
.btn.primary {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--primary) 88%, var(--background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **步骤 6：创建 src/modules/software-manager/components/InstallProgressDialog.vue**

```vue
<template>
  <div class="overlay">
    <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon :icon="phaseIcon" class="phase-icon" />
          {{ phaseText }}
        </div>
      </div>

      <div class="prog-bar" :class="{ indeterminate: task.total == null && task.phase === 'downloading' }">
        <i :style="{ width: percentDisplay + '%' }"></i>
      </div>
      <div class="prog-stats">
        <span v-if="task.phase === 'downloading'">
          {{ formatSize(task.downloaded) }}
          <span v-if="task.total != null">/ {{ formatSize(task.total) }}</span>
        </span>
      </div>

      <div v-if="task.source" class="prog-meta">
        <div class="row">
          <span class="k">镜像源</span>
          <span class="v">{{ task.source }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'

const props = defineProps<{
  task: any
  source?: string
}>()

const phaseIcon = computed(() => {
  if (props.task.phase === 'downloading') return 'mdi:download'
  if (props.task.phase === 'extracting') return 'mdi:package-variant-closed'
  return 'mdi:check'
})
const phaseText = computed(() => {
  if (props.task.phase === 'downloading') return '下载中'
  if (props.task.phase === 'extracting') return '解压中'
  if (props.task.phase === 'completed') return '完成'
  return '失败'
})
const percentDisplay = computed(() => {
  if (props.task.phase === 'downloading') {
    return props.task.percent ?? 0
  }
  return props.task.percent ?? 0
})

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.phase-icon {
  color: var(--primary);
}
.prog-bar {
  height: 8px;
  background: var(--muted);
  border-radius: 999px;
  overflow: hidden;
  margin-bottom: 8px;
}
.prog-bar i {
  display: block;
  height: 100%;
  border-radius: 999px;
  background: var(--primary);
  transition: width 0.3s ease-out;
}
.prog-bar.indeterminate i {
  width: 40% !important;
  animation: indet 1.4s ease-in-out infinite;
}
@keyframes indet {
  0% { margin-left: -40%; }
  50% { margin-left: 100%; }
  100% { margin-left: 100%; }
}
.prog-stats {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--muted-foreground);
}
.prog-meta {
  margin-top: 14px;
  padding: 10px 12px;
  border-radius: 6px;
  background: var(--muted);
  font-size: 12px;
}
.prog-meta .row {
  display: flex;
  justify-content: space-between;
  padding: 2px 0;
}
.prog-meta .row .k {
  color: var(--muted-foreground);
}
.prog-meta .row .v {
  color: var(--foreground);
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
}
</style>
```

- [ ] **步骤 7：创建 src/modules/software-manager/components/CustomInstallDialog.vue**

```vue
<template>
  <div class="overlay" @click.self="cancel">
    <div class="dialog">
      <div class="dialog-head">
        <div class="dialog-title">
          <Icon icon="mdi:plus-box" />
          上传自定义压缩包
        </div>
        <button class="dialog-close" @click="cancel">
          <Icon icon="mdi:close" />
        </button>
      </div>

      <div class="field">
        <div class="field-label">名称</div>
        <input class="input" v-model="name" placeholder="my-tool" />
      </div>

      <div class="field">
        <div class="field-label">压缩包</div>
        <div class="file-drop" @click="selectFile">
          <Icon icon="mdi:upload" />
          <div class="main-text" v-if="!archivePath">点击选择</div>
          <div class="file-chosen" v-else>
            <Icon icon="mdi:check" />
            <span class="path">{{ archiveName }}</span>
            <button class="btn ghost" @click.stop="selectFile">更换</button>
          </div>
          <div class="sub-text" v-if="!archivePath">支持 zip、tar.gz 格式</div>
        </div>
      </div>

      <div class="field">
        <div class="field-label">安装路径 <span class="hint">自动派生</span></div>
        <div class="input readonly input-mono">{{ installPath }}</div>
      </div>

      <div class="dialog-footer">
        <button class="btn" @click="cancel">取消</button>
        <button class="btn primary" @click="install" :disabled="!canInstall">
          开始安装
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Icon } from '@iconify/vue'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { useInstallStore } from '../stores/install'

const emit = defineEmits<{
  cancel: []
  installed: [id: string]
}>()

const name = ref('')
const archivePath = ref('')
const installing = ref(false)

const canInstall = computed(() => name.value.trim() !== '' && archivePath.value !== '')
const archiveName = computed(() => {
  const parts = archivePath.value.split(/[\\/]/)
  return parts[parts.length - 1] || ''
})
const installPath = computed(() => name.value.trim() ? `apps/custom/${name.value.trim()}` : '')

async function selectFile() {
  const selected = await open({
    multiple: false,
    filters: [
      { name: 'Archives', extensions: ['zip', 'tar.gz', 'gz'] },
    ],
  })
  if (selected && typeof selected === 'string') {
    archivePath.value = selected
  }
}

async function install() {
  installing.value = true
  try {
    const installId = await invoke('install_custom', {
      params: {
        name: name.value.trim(),
        archive_path: archivePath.value,
      },
    }) as string
    useInstallStore().createTask(installId, 'custom', name.value.trim())
    emit('installed', installId)
  } catch (e) {
    console.error('Failed to install:', e)
  } finally {
    installing.value = false
  }
}

function cancel() {
  emit('cancel')
}
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: center;
  background: oklch(0 0 0 / 0.5);
  backdrop-filter: blur(4px);
}
.dialog {
  width: 420px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--card);
  box-shadow: var(--shadow-popover);
  padding: 20px;
}
.dialog-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}
.dialog-title {
  font-size: 15px;
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 10px;
}
.dialog-close {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--muted-foreground);
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dialog-close:hover {
  background: var(--muted);
  color: var(--foreground);
}
.field {
  margin-bottom: 14px;
}
.field-label {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.hint {
  font-size: 11px;
}
.input {
  width: 100%;
  height: 34px;
  padding: 0 10px;
  background: var(--muted);
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--foreground);
  font-size: 13px;
}
.input.readonly {
  color: var(--muted-foreground);
}
.input-mono {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
}
.file-drop {
  border: 1px dashed var(--border);
  border-radius: 8px;
  padding: 18px;
  text-align: center;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.file-drop:hover {
  border-color: var(--primary);
  background: color-mix(in oklch, var(--primary) 6%, transparent);
}
.file-drop svg {
  width: 28px;
  height: 28px;
  color: var(--muted-foreground);
  margin: 0 auto 6px;
}
.file-drop .main-text {
  font-size: 13px;
}
.file-drop .sub-text {
  font-size: 11px;
  color: var(--muted-foreground);
  margin-top: 2px;
}
.file-chosen {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--muted);
  border-radius: 6px;
  font-size: 12px;
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', 'Consolas', monospace;
}
.file-chosen svg {
  width: 14px;
  height: 14px;
  color: var(--success);
  margin: 0;
}
.file-chosen .path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 8px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  border: none;
  background: transparent;
  color: var(--foreground);
}
.btn:hover {
  background: var(--muted);
}
.btn.ghost:hover {
  background: var(--muted);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--border);
}
.btn.primary {
  height: 32px;
  padding: 0 12px;
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--primary) 88%, var(--background));
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **步骤 8：重写 src/modules/software-manager/pages/RepositoryPage.vue**

```vue
<template>
  <div class="animate-fade-in">
    <div class="page-header">
      <div class="page-header-left">
        <div class="page-header-icon">
          <Icon icon="mdi:package-variant-closed" />
        </div>
        <div>
          <h1>软件仓库</h1>
          <p>安装新软件</p>
        </div>
      </div>
      <button class="btn" @click="refreshCatalog">
        <Icon icon="mdi:refresh" /> 刷新目录
      </button>
    </div>

    <div class="content">
      <div v-for="(entries, category) in catalogStore.groupedEntries" :key="category" class="category-section">
        <div class="category-title">
          <Icon :icon="categoryIcon(category)" /> {{ categoryName(category) }}
        </div>
        <div class="sw-grid">
          <SoftwareCard
            v-for="entry in entries"
            :key="entry.key"
            :entry="entry"
            :installed-ids="installedIds"
            :default-jre-id="defaultJreId"
            @install="openInstall(entry)"
          />
        </div>
      </div>

      <div class="category-section">
        <div class="category-title">
          <Icon icon="mdi:plus-box" /> 自定义
        </div>
        <div class="custom-card" @click="openCustomInstall">
          <div class="sw-icon">
            <Icon icon="mdi:upload" />
          </div>
          <div class="custom-body">
            <h3>上传自定义压缩包</h3>
            <p>支持 zip、tar.gz 格式</p>
            <span class="format-hint">
              <Icon icon="mdi:file-document" /> 支持 zip、tar.gz
            </span>
          </div>
          <button class="btn primary">
            <Icon icon="mdi:upload" /> 上传安装
          </button>
        </div>
      </div>
    </div>

    <InstallDialog
      v-if="showInstallDialog && !isJreInstall"
      :entry="selectedEntry!"
      @cancel="showInstallDialog = false"
      @installed="onInstalled"
    />
    <InstallJreDialog
      v-if="showInstallDialog && isJreInstall"
      :entry="selectedEntry!"
      @cancel="showInstallDialog = false"
      @installed="onInstalled"
    />
    <CustomInstallDialog
      v-if="showCustomDialog"
      @cancel="showCustomDialog = false"
      @installed="onInstalled"
    />
    <InstallProgressDialog
      v-if="activeInstallId"
      :task="installStore.tasks[activeInstallId]"
      :source="activeInstallSource"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { useCatalogStore } from '../stores/catalog'
import { useInstallStore } from '../stores/install'
import SoftwareCard from '../components/SoftwareCard.vue'
import InstallDialog from '../components/InstallDialog.vue'
import InstallJreDialog from '../components/InstallJreDialog.vue'
import InstallProgressDialog from '../components/InstallProgressDialog.vue'
import CustomInstallDialog from '../components/CustomInstallDialog.vue'

const catalogStore = useCatalogStore()
const installStore = useInstallStore()

const showInstallDialog = ref(false)
const showCustomDialog = ref(false)
const selectedEntry = ref<any>(null)
const installedList = ref<any[]>([])
const defaultJreId = ref<string | null>(null)

const installedIds = computed(() => {
  const set = new Set<string>()
  installedList.value.forEach(s => {
    if (!s.is_custom) {
      set.add(`${s.key}-${s.version}`)
    }
  })
  return set
})

const isJreInstall = computed(() => selectedEntry.value?.key === 'jre')
const activeInstallId = computed(() => installStore.activeTasks[0]?.id || null)
const activeInstallSource = computed(() => {
  const task = installStore.tasks[activeInstallId.value || '']
  return task?.key === 'custom' ? null : '清华镜像'
})

function categoryName(cat: string): string {
  if (cat === 'Database') return '数据库'
  if (cat === 'Runtime') return '运行时'
  if (cat === 'Cache') return '缓存'
  if (cat === 'WebServer') return 'Web 服务器'
  return cat
}

function categoryIcon(cat: string): string {
  if (cat === 'Database') return 'mdi:database'
  if (cat === 'Runtime') return 'mdi:play-circle'
  if (cat === 'Cache') return 'mdi:database'
  if (cat === 'WebServer') return 'mdi:web'
  return 'mdi:package-variant-closed'
}

function openInstall(entry: any) {
  selectedEntry.value = entry
  showInstallDialog.value = true
}

function openCustomInstall() {
  showCustomDialog.value = true
}

function onInstalled(id: string) {
  showInstallDialog.value = false
  showCustomDialog.value = false
}

async function refreshCatalog() {
  await catalogStore.refreshCatalog()
}

async function loadInstalled() {
  try {
    const res = await window.__TAURI_INTERNALS__?.invoke('list_installed_software')
    if (res) installedList.value = res as any[]
  } catch (e) {
    console.error('Failed to load installed:', e)
  }
}

onMounted(async () => {
  await catalogStore.loadCatalog()
  await loadInstalled()
  await installStore.initEvents()
})

onUnmounted(() => {
  installStore.cleanup()
})
</script>

<style scoped>
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
}
.page-header-left {
  display: flex;
  align-items: center;
  gap: 12px;
}
.page-header-icon {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  background: color-mix(in oklch, var(--primary) 12%, transparent);
  color: var(--primary);
  display: flex;
  align-items: center;
  justify-content: center;
}
.page-header h1 {
  font-size: 20px;
  font-weight: 600;
}
.page-header p {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-top: 2px;
}
.content {
  display: flex;
  flex-direction: column;
  gap: 24px;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  border: 1px solid var(--border);
  background: var(--card);
  color: var(--foreground);
  transition: background 0.15s;
}
.btn:hover {
  background: var(--muted);
}
.btn.primary {
  background: var(--primary);
  color: var(--primary-foreground);
  border-color: var(--primary);
}
.btn.primary:hover {
  background: color-mix(in oklch, var(--primary) 88%, var(--background));
}
.category-section {
  margin-bottom: 24px;
}
.category-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--muted-foreground);
  text-transform: uppercase;
  letter-spacing: 0.06em;
  margin-bottom: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.category-title svg {
  color: var(--primary);
}
.sw-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 14px;
}
.custom-card {
  border: 1px dashed var(--border);
  background: color-mix(in oklch, var(--card) 60%, transparent);
  border-radius: var(--radius-lg);
  padding: 18px;
  display: flex;
  align-items: center;
  gap: 16px;
  cursor: pointer;
}
.custom-card:hover {
  border-color: var(--primary);
  background: color-mix(in oklch, var(--primary) 6%, transparent);
}
.custom-card .sw-icon {
  width: 40px;
  height: 40px;
  border-radius: 8px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in oklch, var(--warning) 14%, transparent);
  color: var(--warning);
}
.custom-card .custom-body {
  flex: 1;
}
.custom-card h3 {
  font-size: 15px;
  font-weight: 600;
}
.custom-card p {
  font-size: 12px;
  color: var(--muted-foreground);
  margin-top: 2px;
}
.custom-card .format-hint {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-top: 6px;
  font-size: 11px;
  color: var(--muted-foreground);
  padding: 2px 8px;
  border-radius: 4px;
  background: var(--muted);
}
</style>
```

- [ ] **步骤 9：运行 npm run build 验证**

运行：`npm run build`
预期：无 TypeScript 错误

- [ ] **步骤 10：Commit**

```bash
git add src/modules/software-manager
git commit -m "feat: 实现软件仓库前端页面与组件"
```

---

### 任务 12：完整验证与清理

- [ ] **步骤 1：运行完整构建验证**

运行：
```bash
cd src-tauri && cargo test
npm run build
```

预期：所有测试通过，无编译错误

- [ ] **步骤 2：更新 design-mockup.html 添加软件仓库预览（可选）**

- [ ] **步骤 3：运行 git status 检查未跟踪文件**

- [ ] **步骤 4：Commit（如有需要）**

---

## 规格自检

- [ ] 占位符扫描：无 TODO 或未完成章节
- [ ] 内部一致性：各章节描述一致
- [ ] 范围检查：聚焦安装新软件，边界清晰
- [ ] 模糊性检查：无歧义需求

---

## 执行选项

1. **子代理驱动（推荐）**：每个任务调度一个新的子代理，任务间进行审查，快速迭代
2. **内联执行**：在当前会话中使用 executing-plans 执行任务，批量执行并设有检查点供审查

---

## 计划文件

保存位置：`docs/superpowers/plans/2026-06-30-software-repository.md`
</think_never_used_51bce0c785ca2f68081bfa7d91973934>
<seed:tool_call><function name="Write"><parameter name="file_path" string="true">D:\object\opx\docs\superpowers\plans\2026-06-30-software-repository.md