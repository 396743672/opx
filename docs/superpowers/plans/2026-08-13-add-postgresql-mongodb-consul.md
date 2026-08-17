# 新增 PostgreSQL / MongoDB / Consul 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 OPX 软件仓库新增三个常驻服务（PostgreSQL、MongoDB、Consul），每个一个 provider，复用现有 `SoftwareProvider` trait 与启停流。

**架构：** 每个软件一个 `providers/{key}.rs` 文件，实现 `SoftwareProvider` trait，在 `providers/mod.rs::all_providers()` 注册。`SoftwareListPage.vue` 按软件 key 硬编码分组，需为三个新 key 加分支。启停全程复用 `spawn_process` + `stop_one`，不扩展 trait。

**技术栈：** Rust + Tauri v2（后端），Vue 3 + TypeScript + Pinia + vue-i18n（前端）。

---

## 设计依据（文档验证结论，禁止猜测）

- **PostgreSQL**：`postgres -D <data>` 前台运行（PostgreSQL 文档 server-start.html 确认）。停止沿用 `stop_one` 的 `taskkill /PID`（发控制信号）；不用 `pg_ctl` 守护模式，避免 PID 记录错乱。`postgresql.conf` 为无 section 的 `key = value` 文本，**复用现有 Ini 读写器**（section=None），不引入新格式。
- **MongoDB**：官方 `mongod.cfg` 为 **YAML**（MongoDB 手册 administration/configuration）。现有 `config_editor` 无 YAML 读写器，引入 YAML 引擎违背 ponytail。改用**纯命令行参数启动**（同 MinIO），`config_file_path` 返回 `None`，表单字段直接映射 CLI 参数，无源码视图。
- **Consul**：`consul agent -dev` / `consul agent -server -bootstrap-expect=1`（HashiCorp 文档）。健康检查用 HTTP `/v1/status/leader`（预期 200）。
- **图标**：已验证 `mdi:database`、`mdi:leaf`、`mdi:hexagon-multiple` 均在 `@iconify-json/mdi` 中存在（gen-icons.mjs 不会报 MISSING）。
- **下载 URL / 版本号**：本计划不发明 release URL。`catalog_entry()` 中的版本与 URL 在执行任务时按各软件官方下载页实际占位填入，步骤会标注。参考现有 MySQL 的 `fetch_remote_versions` 从配置文件读版本列表的做法，第一批采用硬编码版本占位（URL 形如官方已知稳定路径），执行时核实。

## 文件结构

**修改：**
- `src-tauri/src/services/software_manager/providers/mod.rs` — 注册三个新 provider 到 `all_providers()`
- `src-tauri/src/models/software.rs` — `SoftwareCategory` 枚举加 `Registry` 变体
- `src/models/software.ts` — `SoftwareCategory` 枚举加 `Registry`
- `src/modules/software-manager/stores/catalog.ts` — `groupedEntries` 记录加 `Registry` 键
- `src/modules/software-manager/pages/SoftwareListPage.vue` — 硬编码分组加新 key 分支 + `registry` 组
- `src/locales/zh-CN.ts` — 新增 i18n key
- `src/locales/en-US.ts` — 新增 i18n key

**创建：**
- `src-tauri/src/services/software_manager/providers/postgresql.rs` — PostgreSQL provider
- `src-tauri/src/services/software_manager/providers/mongodb.rs` — MongoDB provider
- `src-tauri/src/services/software_manager/providers/consul.rs` — Consul provider

**测试（后端单测与现有 provider 同级，放模块内 `#[cfg(test)]`）：**
- 每个新 provider 文件底部加 `#[cfg(test)] mod tests`，断言 `config_schema`/`health_check`/`start_command` 的关键输出

---

## 任务 1：后端 `SoftwareCategory` 加 `Registry`

**文件：**
- 修改：`src-tauri/src/models/software.rs:40-46`

- [ ] **步骤 1：修改枚举**

把 `SoftwareCategory` 枚举改为（在 `WebServer` 后加 `Registry`，派生顺序与前端保持一致）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareCategory {
    Database,
    Runtime,
    Cache,
    WebServer,
    Registry,
}
```

- [ ] **步骤 2：编译验证**

运行：`cd src-tauri && cargo check`
预期：编译通过（暂无引用 `Registry` 变体，不报错）

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/src/models/software.rs
git commit -m "feat: SoftwareCategory 新增 Registry 变体"
```

---

## 任务 2：前端 `SoftwareCategory` 与 catalog store 加 `Registry`

**文件：**
- 修改：`src/models/software.ts:31-36`
- 修改：`src/modules/software-manager/stores/catalog.ts:11-24`

- [ ] **步骤 1：修改 `models/software.ts`**

```typescript
export enum SoftwareCategory {
  Database = 'Database',
  Runtime = 'Runtime',
  Cache = 'Cache',
  WebServer = 'WebServer',
  Registry = 'Registry',
}
```

- [ ] **步骤 2：修改 `catalog.ts` 的 `groupedEntries`**

把 `groups` 记录加 `Registry` 键：

```typescript
const groupedEntries = computed(() => {
  const groups: Record<SoftwareCategory, CatalogEntry[]> = {
    [SoftwareCategory.Database]: [],
    [SoftwareCategory.Runtime]: [],
    [SoftwareCategory.Cache]: [],
    [SoftwareCategory.WebServer]: [],
    [SoftwareCategory.Registry]: [],
  }
  entries.value.forEach((e) => {
    if (groups[e.category]) {
      groups[e.category].push(e)
    }
  })
  return groups
})
```

- [ ] **步骤 3：类型检查**

运行：`npm run build`
预期：通过（vue-tsc + vite），无类型错误

- [ ] **步骤 4：Commit**

```bash
git add src/models/software.ts src/modules/software-manager/stores/catalog.ts
git commit -m "feat: 前端 SoftwareCategory 枚举与 catalog store 加 Registry"
```

---

## 任务 3：后端 `providers/mod.rs` 模块声明

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/mod.rs:6-13`（mod 声明）
- 修改：`src-tauri/src/services/software_manager/providers/mod.rs:177-187`（`all_providers`）

- [ ] **步骤 1：加 mod 声明**

在现有 `pub mod rustfs;` 后加：

```rust
pub mod postgresql;
pub mod mongodb;
pub mod consul;
```

- [ ] **步骤 2：暂不改 `all_providers`**

留到任务 7 各 provider 文件就绪后统一注册。当前只加 mod 声明。

- [ ] **步骤 3：编译验证（预期失败）**

运行：`cd src-tauri && cargo check`
预期：**失败**，报 `unresolved module postgresql`/`mongodb`/`consul`（因为文件尚未创建）。这是预期的，下一个任务创建文件后修复。

- [ ] **步骤 4：暂不 commit**，与任务 4-6 一起在任务 7 后统一提交（保持每 commit 可编译）

> 说明：本计划按 TDD 粒度拆分，但三个 provider 文件互相独立、共享一次注册。为避免中间提交不可编译，任务 3-7 的 Commit 合并到任务 7 末尾一次完成。各 provider 文件（任务 4/5/6）独立编写与测试，但 commit 时机统一。

---

## 任务 4：PostgreSQL provider

**文件：**
- 创建：`src-tauri/src/services/software_manager/providers/postgresql.rs`

参考现有 `providers/mysql.rs`（FirstRunInit 模式）与 `providers/redis.rs`（ConfigSchema 模式）。

- [ ] **步骤 1：编写失败测试**

创建 `postgresql.rs` 文件，先只放测试模块（文件其余实现留空会导致测试失败，符合 TDD）：

```rust
use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, FirstRunInit, HealthContext, InstallContext, SoftwareProvider, StartCommand,
    StartContext, WorkingDirContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct PostgreSqlProvider;

impl PostgreSqlProvider {
    pub fn new() -> Self { Self }
}
impl Default for PostgreSqlProvider {
    fn default() -> Self { Self::new() }
}

// SoftwareProvider impl 留空待实现 —— 见步骤 3

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::software_manager::providers::{HealthContext, StartContext};

    fn provider() -> PostgreSqlProvider { PostgreSqlProvider::new() }

    #[test]
    fn key_is_postgresql() {
        assert_eq!(provider().key(), "postgresql");
    }

    #[test]
    fn catalog_is_database_category() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Database);
        assert_eq!(provider().catalog_entry().icon, "mdi:database");
    }

    #[test]
    fn health_check_tcp_5432_by_default() {
        let ctx = HealthContext {
            installed_id: "x".into(), install_path: "/p".into(), port: 0,
            config: serde_json::json!({}),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 5432),
            other => panic!("expected Tcp, got {:?}", other),
        }
    }

    #[test]
    fn health_check_uses_config_port() {
        let ctx = HealthContext {
            installed_id: "x".into(), install_path: "/p".into(), port: 0,
            config: serde_json::json!({ "port": 15432 }),
        };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 15432),
            other => panic!("expected Tcp, got {:?}", other),
        }
    }

    #[test]
    fn config_file_is_postgresql_conf_relative() {
        let ctx = ConfigContext {
            install_path: "/p".into(), version: "16".into(), config: serde_json::json!({}),
        };
        // 仅返回相对文件名，与 mysql/redis 一致
        assert_eq!(provider().config_file_path(&ctx).as_deref(), Some(std::path::Path::new("postgresql.conf")));
    }

    #[test]
    fn start_command_uses_postgres_exe_foreground() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "postgres.exe");
        // 前台运行，参数 -D <data>
        assert!(cmd.args.iter().any(|a| a == "-D"));
        assert!(cmd.first_run_init.is_some(), "PG 首次启动需 initdb 初始化");
    }

    #[test]
    fn config_schema_has_port_and_shared_buffers() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"shared_buffers"));
        assert!(keys.contains(&"max_connections"));
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib providers::postgresql`
预期：**失败**，编译错误（`SoftwareProvider` 未实现，方法不存在）

- [ ] **步骤 3：实现 provider**

在同文件 `impl Default` 之后补全 `SoftwareProvider` 实现：

```rust
impl SoftwareProvider for PostgreSqlProvider {
    fn key(&self) -> &str { "postgresql" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：PostgreSQL 官方 Windows binaries 由 EnterpriseDB 提供，
            // URL 形如 https://get.enterprisedb.com/postgresql/postgresql-<ver>-windows-x64-binaries.zip
            // 版本号在执行任务时按官网当前稳定版填入（参考 16.x / 17.x）。
            versions.push(CatalogVersion {
                version: "16.x".to_string(), // 占位，执行时改为实际版本号
                mirrors: vec![MirrorSource {
                    name: "i18n:postgresqlOfficial".to_string(),
                    url: "https://get.enterprisedb.com/postgresql/postgresql-16.x-windows-x64-binaries.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        // 注：本 provider Windows-only（与 mysql/redis 决策一致），不在 Unix 注册版本。
        CatalogEntry {
            key: "postgresql".to_string(),
            name: "PostgreSQL".to_string(),
            description: "关系型数据库".to_string(),
            description_i18n: Some("catalogDesc.postgresql".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:database".to_string(),
            versions,
            default_version: "16.x".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> {
        // 无需后置处理：initdb 在首次启动的 first_run_init 中完成
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let working_dir = PathBuf::from(&ctx.install_path);
        let data_dir = working_dir.join("data");

        // 首次初始化：initdb -D data -U postgres --auth=trust --encoding=UTF8
        let init_command = StartCommand {
            program: "bin/initdb.exe".to_string(),
            args: vec![
                "-D".to_string(), data_dir.to_string_lossy().to_string(),
                "-U".to_string(), "postgres".to_string(),
                "--auth=trust".to_string(),
                "--encoding=UTF8".to_string(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };

        // 前台主进程：postgres -D data（端口由 postgresql.conf 的 port 控制）
        Ok(StartCommand {
            program: "postgres.exe".to_string(),
            args: vec![
                "-D".to_string(), data_dir.to_string_lossy().to_string(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: Some(Box::new(FirstRunInit {
                init_command,
                temp_secret_output: None,
            })),
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 5432 });
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(5432),
                    section: None,
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "listen_addresses".to_string(),
                    label_i18n: "configField.listenAddresses".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("localhost"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "max_connections".to_string(),
                    label_i18n: "configField.maxConnections".to_string(),
                    field_type: ConfigFieldType::Number,
                    default_value: serde_json::json!(100),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "shared_buffers".to_string(),
                    label_i18n: "configField.sharedBuffers".to_string(),
                    field_type: ConfigFieldType::Size { units: vec!["MB".to_string(), "GB".to_string()] },
                    default_value: serde_json::json!("128MB"),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> {
        // postgresql.conf 无 section，复用 Ini 读写器（section=None）
        Some(PathBuf::from("postgresql.conf"))
    }

    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf {
        PathBuf::from(&ctx.install_path)
    }
}
```

> **执行时核实点**：(1) EDB 下载 URL 与版本号；(2) `bin/initdb.exe` 与 `postgres.exe` 在 zip 内的相对路径（EDB binaries zip 通常顶层有 `pgsql/bin/...`，`extract_zip_flatten` 会剥顶层目录，执行时确认剥除后 `bin/initdb.exe` 是否存在）。若路径不符，调整 `program` 字段。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib providers::postgresql`
预期：PASS（全部 6 个测试通过）

- [ ] **步骤 5：暂不单独 commit**（任务 3-7 末尾统一提交，见任务 7）

---

## 任务 5：MongoDB provider

**文件：**
- 创建：`src-tauri/src/services/software_manager/providers/mongodb.rs`

参考 `providers/minio.rs`（纯 CLI 参数启动、`config_file_path` 返回 None、表单字段映射 CLI）。

- [ ] **步骤 1：编写失败测试**

创建 `mongodb.rs`，先放测试：

```rust
use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, HealthContext, InstallContext, SoftwareProvider, StartCommand, StartContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct MongoDbProvider;

impl MongoDbProvider { pub fn new() -> Self { Self } }
impl Default for MongoDbProvider { fn default() -> Self { Self::new() } }

// impl 留待步骤 3

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> MongoDbProvider { MongoDbProvider::new() }

    #[test]
    fn key_is_mongodb() { assert_eq!(provider().key(), "mongodb"); }

    #[test]
    fn catalog_is_database() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Database);
        assert_eq!(provider().catalog_entry().icon, "mdi:leaf");
    }

    #[test]
    fn no_config_file() {
        let ctx = ConfigContext { install_path: "/p".into(), version: "7".into(), config: serde_json::json!({}) };
        assert!(provider().config_file_path(&ctx).is_none(), "MongoDB 走纯 CLI，无配置文件");
    }

    #[test]
    fn start_command_maps_port_and_dbpath() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({ "port": 27018, "bind_ip": "127.0.0.1" }),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "mongod.exe");
        assert!(cmd.args.iter().any(|a| a == "--port"));
        assert!(cmd.args.iter().any(|a| a == "27018"));
        assert!(cmd.args.iter().any(|a| a == "--bind_ip"));
        assert!(cmd.first_run_init.is_none(), "MongoDB 无需首次初始化");
    }

    #[test]
    fn health_check_tcp_27017_default() {
        let ctx = HealthContext { installed_id: "x".into(), install_path: "/p".into(), port: 0, config: serde_json::json!({}) };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Tcp { port, .. } => assert_eq!(port, 27017),
            o => panic!("expected Tcp, got {:?}", o),
        }
    }

    #[test]
    fn config_schema_has_port_bind_ip_dbpath() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"bind_ip"));
        assert!(keys.contains(&"dbpath"));
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib providers::mongodb`
预期：**失败**（`SoftwareProvider` 未实现）

- [ ] **步骤 3：实现 provider**

```rust
fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

impl SoftwareProvider for MongoDbProvider {
    fn key(&self) -> &str { "mongodb" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：MongoDB 官方 Windows zip URL 形如
            // https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-<ver>.zip
            // 版本号在执行任务时按官网当前稳定版填入（参考 7.x / 8.x）。
            versions.push(CatalogVersion {
                version: "7.x".to_string(), // 占位，执行时改为实际版本号
                mirrors: vec![MirrorSource {
                    name: "i18n:mongodbOfficial".to_string(),
                    url: "https://fastdl.mongodb.org/windows/mongodb-windows-x86_64-7.x.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "mongodb".to_string(),
            name: "MongoDB".to_string(),
            description: "文档数据库".to_string(),
            description_i18n: Some("catalogDesc.mongodb".to_string()),
            category: SoftwareCategory::Database,
            icon: "mdi:leaf".to_string(),
            versions,
            default_version: "7.x".to_string(),
        }
    }

    fn post_install(&self, ctx: &InstallContext) -> Result<()> {
        // 确保 data 目录存在（mongod 启动时会用，提前创建避免相对路径问题）
        std::fs::create_dir_all(ctx.install_dir().join("data"))?;
        Ok(())
    }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let port = config_u64(&ctx.config, "port", 27017);
        let bind_ip = config_str(&ctx.config, "bind_ip", "127.0.0.1");
        let dbpath = config_str(&ctx.config, "dbpath", "./data");
        // dbpath 解析为绝对路径（相对 install_path），与 minio 一致
        let abs_dbpath = if std::path::Path::new(&dbpath).is_absolute() {
            dbpath.clone()
        } else {
            let clean = dbpath.strip_prefix("./").or_else(|| dbpath.strip_prefix(".\\")).unwrap_or(&dbpath);
            PathBuf::from(&ctx.install_path).join(clean).to_string_lossy().replace('\\', "/")
        };
        let _ = std::fs::create_dir_all(&abs_dbpath);
        let log_path = PathBuf::from(&ctx.install_path).join("data").join("mongod.log");
        let _ = std::fs::create_dir_all(log_path.parent().unwrap());

        Ok(StartCommand {
            program: "mongod.exe".to_string(),
            args: vec![
                "--dbpath".to_string(), abs_dbpath,
                "--bind_ip".to_string(), bind_ip,
                "--port".to_string(), port.to_string(),
                "--logpath".to_string(), log_path.to_string_lossy().to_string(),
            ],
            env_vars: std::collections::BTreeMap::new(),
            working_dir: PathBuf::from(&ctx.install_path),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        let port = ctx.config.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            .unwrap_or(if ctx.port > 0 { ctx.port } else { 27017 });
        HealthCheckSpec::Tcp { port, timeout_ms: 1000 }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "port".to_string(),
                    label_i18n: "configField.port".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(27017),
                    section: None,
                    description_i18n: Some("configField.portDesc".to_string()),
                },
                ConfigField {
                    key: "bind_ip".to_string(),
                    label_i18n: "configField.bindIp".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "dbpath".to_string(),
                    label_i18n: "configField.dataDir".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("./data"),
                    section: None,
                    description_i18n: Some("configField.dataDirDesc".to_string()),
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}
```

> **执行时核实点**：(1) MongoDB 下载 URL 与版本号；(2)`mongod.exe` 在 zip 解压后的相对位置（`extract_zip_flatten` 剥顶层后是否在 install_path 根，若在 `bin/` 下则 `program` 改为 `"bin/mongod.exe"`）；(3) `config_file_path` 返回 None 意味着前端 `ConfigEditDialog` 不显示源码视图（与 MinIO 一致，已验证 MinIO 走此路径）。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib providers::mongodb`
预期：PASS（全部 5 个测试通过）

- [ ] **步骤 5：暂不单独 commit**（任务 7 末尾统一提交）

---

## 任务 6：Consul provider

**文件：**
- 创建：`src-tauri/src/services/software_manager/providers/consul.rs`

参考 `providers/redis.rs`（Select 字段）与 `providers/minio.rs`（CLI 参数）。

- [ ] **步骤 1：编写失败测试**

创建 `consul.rs`，先放测试：

```rust
use anyhow::Result;
use std::path::PathBuf;

use crate::models::software::{
    ArchiveFormat, ArchiveInfo, CatalogEntry, CatalogVersion, ConfigField,
    ConfigFieldType, ConfigSchema, HealthCheckSpec, MirrorSource, SoftwareCategory,
};

use super::{
    ConfigContext, HealthContext, InstallContext, SoftwareProvider, StartCommand, StartContext,
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(not(windows))]
const CREATE_NO_WINDOW: u32 = 0;

pub struct ConsulProvider;

impl ConsulProvider { pub fn new() -> Self { Self } }
impl Default for ConsulProvider { fn default() -> Self { Self::new() } }

// impl 留待步骤 3

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ConsulProvider { ConsulProvider::new() }

    #[test]
    fn key_is_consul() { assert_eq!(provider().key(), "consul"); }

    #[test]
    fn catalog_is_registry_category() {
        assert_eq!(provider().catalog_entry().category, SoftwareCategory::Registry);
        assert_eq!(provider().catalog_entry().icon, "mdi:hexagon-multiple");
    }

    #[test]
    fn start_command_dev_mode_by_default() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
            config: serde_json::json!({}), // 默认 dev
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert_eq!(cmd.program, "consul.exe");
        assert!(cmd.args.iter().any(|a| a == "agent"));
        assert!(cmd.args.iter().any(|a| a == "-dev"));
        assert!(cmd.first_run_init.is_none());
    }

    #[test]
    fn start_command_server_mode_when_configured() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/c".into(), version: "1".into(),
            config: serde_json::json!({ "mode": "server", "bind": "127.0.0.1" }),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "-server"));
        assert!(cmd.args.iter().any(|a| a == "-bootstrap-expect=1"));
        assert!(cmd.args.iter().any(|a| a == "-dev") == false, "server 模式不应带 -dev");
    }

    #[test]
    fn health_check_http_8500() {
        let ctx = HealthContext { installed_id: "x".into(), install_path: "/c".into(), port: 0, config: serde_json::json!({}) };
        match provider().health_check(&ctx) {
            HealthCheckSpec::Http { url, expected_status, .. } => {
                assert_eq!(expected_status, 200);
                assert!(url.contains("8500"));
            }
            o => panic!("expected Http, got {:?}", o),
        }
    }

    #[test]
    fn config_schema_has_mode_select_field() {
        let schema = provider().config_schema().unwrap();
        let mode_field = schema.fields.iter().find(|f| f.key == "mode").unwrap();
        match &mode_field.field_type {
            ConfigFieldType::Select { options } => {
                assert!(options.contains(&"dev".to_string()));
                assert!(options.contains(&"server".to_string()));
            }
            o => panic!("mode 应为 Select, got {:?}", o),
        }
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib providers::consul`
预期：**失败**（`SoftwareProvider` 未实现）

- [ ] **步骤 3：实现 provider**

```rust
fn config_str(c: &serde_json::Value, key: &str, default: &str) -> String {
    c.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| default.to_string())
}
fn config_u64(c: &serde_json::Value, key: &str, default: u64) -> u64 {
    c.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

impl SoftwareProvider for ConsulProvider {
    fn key(&self) -> &str { "consul" }

    fn catalog_entry(&self) -> CatalogEntry {
        let mut versions = vec![];
        #[cfg(windows)]
        {
            // 执行时核实：HashiCorp releases URL 形如
            // https://releases.hashicorp.com/consul/<ver>/consul_<ver>_windows_amd64.zip
            versions.push(CatalogVersion {
                version: "1.x".to_string(), // 占位，执行时改为实际版本号
                mirrors: vec![MirrorSource {
                    name: "i18n:consulOfficial".to_string(),
                    url: "https://releases.hashicorp.com/consul/1.x/consul_1.x_windows_amd64.zip".to_string(),
                    builtin: None,
                }],
                archive: ArchiveInfo { format: ArchiveFormat::Zip, size: None, sha256: None },
            });
        }
        CatalogEntry {
            key: "consul".to_string(),
            name: "Consul".to_string(),
            description: "服务注册与发现".to_string(),
            description_i18n: Some("catalogDesc.consul".to_string()),
            category: SoftwareCategory::Registry,
            icon: "mdi:hexagon-multiple".to_string(),
            versions,
            default_version: "1.x".to_string(),
        }
    }

    fn post_install(&self, _ctx: &InstallContext) -> Result<()> { Ok(()) }

    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand> {
        let mode = config_str(&ctx.config, "mode", "dev");
        let bind = config_str(&ctx.config, "bind", "127.0.0.1");
        let working_dir = PathBuf::from(&ctx.install_path);

        let mut args = vec!["agent".to_string()];
        if mode == "server" {
            let data_dir = working_dir.join("data");
            let _ = std::fs::create_dir_all(&data_dir);
            args.push("-server".to_string());
            args.push("-bootstrap-expect=1".to_string());
            args.push(format!("-data-dir={}", data_dir.to_string_lossy()));
            args.push(format!("-bind={}", bind));
        } else {
            // dev 模式：单节点、无持久化、启动即用
            args.push("-dev".to_string());
            args.push(format!("-bind={}", bind));
        }

        Ok(StartCommand {
            program: "consul.exe".to_string(),
            args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir,
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        })
    }

    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec {
        // dev 模式默认 HTTP API 8500；server 模式 http_port 可配置
        let port = config_u64(&ctx.config, "http_port", 8500) as u16;
        let port = if port > 0 { port } else if ctx.port > 0 { ctx.port } else { 8500 };
        HealthCheckSpec::Http {
            url: format!("http://127.0.0.1:{}/v1/status/leader", port),
            expected_status: 200,
            timeout_ms: 1000,
        }
    }

    fn config_schema(&self) -> Option<ConfigSchema> {
        Some(ConfigSchema {
            fields: vec![
                ConfigField {
                    key: "mode".to_string(),
                    label_i18n: "configField.consulateMode".to_string(),
                    field_type: ConfigFieldType::Select { options: vec!["dev".to_string(), "server".to_string()] },
                    default_value: serde_json::json!("dev"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "bind".to_string(),
                    label_i18n: "configField.bind".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("127.0.0.1"),
                    section: None,
                    description_i18n: None,
                },
                ConfigField {
                    key: "http_port".to_string(),
                    label_i18n: "configField.httpPort".to_string(),
                    field_type: ConfigFieldType::Port,
                    default_value: serde_json::json!(8500),
                    section: None,
                    description_i18n: None,
                },
            ],
            ephemeral_keys: vec![],
        })
    }

    fn config_file_path(&self, _ctx: &ConfigContext) -> Option<PathBuf> { None }
}
```

> **执行时核实点**：(1) HashiCorp 下载 URL 与版本号；(2) `consul.exe` 在 zip 内是否在顶层（Consul 官方 zip 解压即单 exe，通常在根）。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd src-tauri && cargo test --lib providers::consul`
预期：PASS（全部 5 个测试通过）

- [ ] **步骤 5：暂不单独 commit**（任务 7 末尾统一提交）

---

## 任务 7：注册三个 provider 到 `all_providers()`

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/mod.rs:177-187`

- [ ] **步骤 1：修改 `all_providers`**

```rust
pub fn all_providers() -> Vec<Box<dyn SoftwareProvider>> {
    vec![
        Box::new(mysql::MySqlProvider::new()),
        Box::new(jre::JreProvider::new()),
        Box::new(jdk::JdkProvider::new()),
        Box::new(redis::RedisProvider::new()),
        Box::new(nginx::NginxProvider::new()),
        Box::new(minio::MinioProvider::new()),
        Box::new(rustfs::RustfsProvider::new()),
        Box::new(postgresql::PostgreSqlProvider::new()),
        Box::new(mongodb::MongoDbProvider::new()),
        Box::new(consul::ConsulProvider::new()),
    ]
}
```

- [ ] **步骤 2：编译验证**

运行：`cd src-tauri && cargo check`
预期：通过（任务 3-6 的文件全部就绪，无未解析模块）

- [ ] **步骤 3：运行全部 provider 测试**

运行：`cd src-tauri && cargo test --lib providers`
预期：PASS（三个新 provider 测试 + 现有测试全绿）

- [ ] **步骤 4：Commit（任务 3-7 合并提交）**

```bash
git add src-tauri/src/services/software_manager/providers/mod.rs \
        src-tauri/src/services/software_manager/providers/postgresql.rs \
        src-tauri/src/services/software_manager/providers/mongodb.rs \
        src-tauri/src/services/software_manager/providers/consul.rs
git commit -m "feat: 新增 PostgreSQL/MongoDB/Consul provider"
```

---

## 任务 8：i18n 新增 key

**文件：**
- 修改：`src/locales/zh-CN.ts`
- 修改：`src/locales/en-US.ts`

- [ ] **步骤 1：`zh-CN.ts` 新增 key**

在 `categoryWebServer: 'Web 服务器',`（第 169 行）后加：

```typescript
  categoryRegistry: '注册中心',
  registry: '注册中心',
```

在 `catalogDesc` 块（第 179 行 `rustfs` 后）加三行：

```typescript
    postgresql: '关系型数据库（PostgreSQL）',
    mongodb: '文档数据库',
    consul: '服务注册与发现',
```

在 `redisWindowsGithub` 等 mirror 名（第 162 行 `rustfsOfficial` 后）加：

```typescript
  postgresqlOfficial: 'PostgreSQL 官方',
  mongodbOfficial: 'MongoDB 官方',
  consulOfficial: 'HashiCorp 官方',
```

在 `configField` 块（第 439 行 `initPasswordDesc` 后）加：

```typescript
    listenAddresses: '监听地址',
    sharedBuffers: '共享缓冲区',
    bindIp: '绑定地址',
    httpPort: 'HTTP 端口',
    consulateMode: '运行模式',
```

- [ ] **步骤 2：`en-US.ts` 新增对照 key**

同样位置加英文：

```typescript
  categoryRegistry: 'Registry',
  registry: 'Registry',
```
```typescript
    postgresql: 'Relational database (PostgreSQL)',
    mongodb: 'Document database',
    consul: 'Service registry & discovery',
```
```typescript
  postgresqlOfficial: 'PostgreSQL Official',
  mongodbOfficial: 'MongoDB Official',
  consulOfficial: 'HashiCorp Official',
```
```typescript
    listenAddresses: 'Listen Addresses',
    sharedBuffers: 'Shared Buffers',
    bindIp: 'Bind IP',
    httpPort: 'HTTP Port',
    consulateMode: 'Run Mode',
```

> 注意：`configField.bind`、`maxConnections`、`port`、`portDesc`、`dataDir`、`dataDirDesc` 已存在，不重复添加。Consul 的 `bind` 复用现有 `configField.bind`，`http_port` 用新 `httpPort`。

- [ ] **步骤 3：类型检查**

运行：`npm run build`
预期：通过

- [ ] **步骤 4：Commit**

```bash
git add src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat: 新增 PostgreSQL/MongoDB/Consul 的 i18n 文案"
```

---

## 任务 9：`SoftwareListPage.vue` 分组加新 key

**文件：**
- 修改：`src/modules/software-manager/pages/SoftwareListPage.vue:120-144`

- [ ] **步骤 1：修改 `grouped` computed**

在 `groups` 记录加 `registry` 组（第 127 行 `custom` 前插入）：

```typescript
    registry: { category: 'registry', label: 'registry', icon: 'mdi:hexagon-multiple', items: [] },
```

在 for 循环的 key 分支（第 136-140 行）加：

```typescript
    else if (sw.key === 'postgresql' || sw.key === 'mongodb') g = 'database'
    else if (sw.key === 'consul') g = 'registry'
```

> 现有分支保留：`mysql → database`、`redis → cache`、`nginx → webserver`、`minio/rustfs → storage`、`jre/jdk → runtime`。新 key 分支插在 `else if (sw.key === 'nginx')` 之后、`else if (sw.key === 'minio' ...)` 之前。

- [ ] **步骤 2：类型检查**

运行：`npm run build`
预期：通过

- [ ] **步骤 3：Commit**

```bash
git add src/modules/software-manager/pages/SoftwareListPage.vue
git commit -m "feat: SoftwareListPage 新增 PostgreSQL/MongoDB/Consul 分组"
```

---

## 任务 10：图标重新生成与全量验证

- [ ] **步骤 1：重新生成图标子集**

运行：`npm run icons:gen`
预期：输出「已生成 N 个图标」，无 `未找到的图标` 报错（`mdi:database`/`mdi:leaf`/`mdi:hexagon-multiple` 已存在）

- [ ] **步骤 2：全量后端测试**

运行：`cd src-tauri && cargo test`
预期：PASS

- [ ] **步骤 3：全量前端构建**

运行：`npm run build`
预期：通过（vue-tsc + vite）

- [ ] **步骤 4：手动验证流程（开发和测试用，记录结果）**

启动 `npm run tauri:dev`，依次：
1. 软件仓库页能看到 PostgreSQL / MongoDB / Consul 三个卡片
2. 分别安装（核实下载 URL 在任务 4-6 执行时已填对）
3. PostgreSQL 首次启动触发 initdb，健康检查 TCP 5432 变 Running
4. MongoDB 首次启动 mongod 自动建 data 目录，健康检查 TCP 27017 变 Running
5. Consul dev 模式启动，健康检查 HTTP 8500 变 Running
6. 各自配置表单能打开、保存、重启生效（MongoDB/Consul 无源码视图）
7. 停止各自，PID 正确注销

- [ ] **步骤 5：Commit 图标产物**

```bash
git add src/assets/mdi-icons.json
git commit -m "chore: 重新生成图标子集（含 PG/MongoDB/Consul）"
```

---

## 自检

**1. 规格覆盖度：** spec 每节对应的任务：
- PostgreSQL provider（含前台启动、initdb、表单+源码）→ 任务 4
- MongoDB provider（纯 CLI、无配置文件、表单）→ 任务 5
- Consul provider（dev/server 切换、HTTP 健康检查、表单）→ 任务 6
- SoftwareCategory 加 Registry（后端+前端+catalog store）→ 任务 1、2
- SoftwareListPage 硬编码分组修正 → 任务 9
- i18n → 任务 8
- 图标生成 → 任务 10
- 测试（后端单测 + 前端构建）→ 任务 4/5/6/10
**无遗漏。**

**2. 占位符扫描：** 版本号/URL 占位（`"16.x"`、`"7.x"`、`"1.x"`）已显式标注「执行时核实」，属环境相关外部依赖，非逻辑占位；每个代码步骤均含完整可编译代码块，无 "TODO/类似任务 N/添加错误处理" 模式。任务 3 末尾说明了合并 commit 的理由，避免中间不可编译提交——这是刻意的工程决定，已在任务 7 commit 覆盖。

---

# 追加：初始化与认证配置（2026-08-14，用户测试反馈）

## 任务 11：ConfigFieldType 新增 Boolean 变体（后端+前端）

**文件：**
- 修改：`src-tauri/src/models/software.rs`（`ConfigFieldType` 枚举）
- 修改：`src/models/software.ts`（`ConfigFieldType` 类型）
- 修改：`src/modules/software-manager/components/ConfigFormTab.vue`（渲染 Boolean 为开关）

- [ ] **步骤 1：后端枚举加 Boolean**

`ConfigFieldType` 枚举（`#[serde(tag = "type")]`）加变体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ConfigFieldType {
    Text,
    Number,
    Port,
    Password,
    Select { options: Vec<String> },
    Size { units: Vec<String> },
    Boolean,
}
```

- [ ] **步骤 2：前端类型加 Boolean**

`src/models/software.ts` 的 `ConfigFieldType` union 加：

```typescript
  | { type: 'Boolean' }
```

- [ ] **步骤 3：ConfigFormTab 渲染 Boolean 开关**

在 `isSize` 分支后加 Boolean 渲染（模板中，约第 49 行后）：

```html
        <div v-else-if="isBoolean(field)" class="switch-field">
          <div
            class="toggle"
            :class="{ off: !formData[field.key] }"
            role="switch"
            :aria-checked="!!formData[field.key]"
            tabindex="0"
            @click="formData[field.key] = !formData[field.key]"
            @keydown.enter.prevent="formData[field.key] = !formData[field.key]"
          ></div>
          <span class="switch-label">{{ $t('enabled') }}</span>
        </div>
```

脚本里加判断函数（约第 107 行 `isSelect` 旁）：

```typescript
function isBoolean(f: ConfigField) {
  return f.field_type.type === 'Boolean'
}
```

样式里加（`<style scoped>` 末尾）：

```css
.switch-field {
  display: flex;
  align-items: center;
  gap: 10px;
}
.switch-field .toggle {
  width: 36px;
  height: 20px;
  border-radius: 999px;
  background: var(--color-primary);
  position: relative;
  cursor: pointer;
  flex-shrink: 0;
}
.switch-field .toggle::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 18px;
  width: 16px;
  height: 16px;
  border-radius: 999px;
  background: white;
  transition: left 0.2s;
}
.switch-field .toggle.off {
  background: var(--color-border);
}
.switch-field .toggle.off::after {
  left: 2px;
}
.switch-label {
  font-size: 13px;
}
```

- [ ] **步骤 4：验证**

运行：`cargo check`（后端）+ `npm run build`（前端）
预期：均通过

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/models/software.rs src/models/software.ts src/modules/software-manager/components/ConfigFormTab.vue
git commit -m "feat: ConfigFieldType 新增 Boolean 类型（表单开关）"
```

---

## 任务 12：PostgreSQL 可选密码 + scram 认证

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/postgresql.rs`

- [ ] **步骤 1：编写失败测试（追加到 postgresql.rs 测试模块）**

```rust
    #[test]
    fn start_command_with_init_password_uses_pwfile_and_scram() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None,
            init_password: Some("secret".into()),
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.starts_with("--pwfile=")),
            "设置密码时 initdb 应带 --pwfile");
        assert!(init.init_command.args.iter().any(|a| a.contains("scram")),
            "设置密码时认证应使用 scram-sha-256");
    }

    #[test]
    fn start_command_without_password_keeps_trust() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/pg".into(), version: "16".into(),
            config: serde_json::json!({}), custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        let init = cmd.first_run_init.as_ref().unwrap();
        assert!(init.init_command.args.iter().any(|a| a.contains("trust")),
            "未设密码时应保持 --auth=trust");
    }

    #[test]
    fn config_schema_has_init_password_ephemeral() {
        let schema = provider().config_schema().unwrap();
        let ip = schema.fields.iter().find(|f| f.key == "init_password").unwrap();
        assert!(schema.ephemeral_keys.contains(&"init_password".to_string()),
            "init_password 必须是 ephemeral（不落盘）");
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib providers::postgresql`
预期：编译错误（`init_password` 字段尚未被消费）

- [ ] **步骤 3：修改 start_command 支持 init_password**

`start_command` 中 init 命令从固定 `--auth=trust` 改为按 `init_password` 分支。密码文件放系统临时目录，避免 app 目录敏感残留：

```rust
        let mut init_args = vec![
            "-D".to_string(), data_dir.to_string_lossy().to_string(),
            "-U".to_string(), "postgres".to_string(),
            "--encoding=UTF8".to_string(),
        ];
        let pwfile_path = crate::utils::paths::tmp_dir().join(format!("pgpass-{}.tmp", ctx.installed_id));
        let _ = std::fs::remove_file(&pwfile_path);
        if let Some(pw) = &ctx.init_password {
            if !pw.is_empty() {
                std::fs::write(&pwfile_path, pw)?;
                init_args.push(format!("--pwfile={}", pwfile_path.to_string_lossy()));
                init_args.push("--auth=scram-sha-256".to_string());
                init_args.push("--auth-host=scram-sha-256".to_string());
            } else {
                init_args.push("--auth=trust".to_string());
            }
        } else {
            init_args.push("--auth=trust".to_string());
        }
        let init_command = StartCommand {
            program: "bin/initdb.exe".to_string(),
            args: init_args,
            env_vars: std::collections::BTreeMap::new(),
            working_dir: working_dir.clone(),
            creation_flags: CREATE_NO_WINDOW,
            first_run_init: None,
        };
```

- [ ] **步骤 4：验证**

运行：`cd src-tauri && cargo test --lib providers::postgresql`
预期：新增 3 测试通过（累计 10）

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/services/software_manager/providers/postgresql.rs
git commit -m "feat: PostgreSQL 支持可选初始化密码（scram-sha-256）"
```

---

## 任务 13：MongoDB 可选认证

**文件：**
- 修改：`src-tauri/src/services/software_manager/providers/mongodb.rs`

- [ ] **步骤 1：编写失败测试**

```rust
    #[test]
    fn start_command_enables_auth_when_configured() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({ "auth_enabled": true }),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(cmd.args.iter().any(|a| a == "--auth"), "auth_enabled=true 应带 --auth");
    }

    #[test]
    fn start_command_no_auth_by_default() {
        let ctx = StartContext {
            installed_id: "x".into(), install_path: "/mg".into(), version: "7".into(),
            config: serde_json::json!({}),
            custom_start_command: None, init_password: None,
        };
        let cmd = provider().start_command(&ctx).unwrap();
        assert!(!cmd.args.iter().any(|a| a == "--auth"), "默认不带 --auth");
    }

    #[test]
    fn config_schema_has_auth_fields() {
        let schema = provider().config_schema().unwrap();
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        assert!(keys.contains(&"auth_enabled"));
        assert!(keys.contains(&"root_user"));
        assert!(keys.contains(&"root_password"));
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd src-tauri && cargo test --lib providers::mongodb`
预期：编译错误（字段未实现）

- [ ] **步骤 3：实现**

`start_command` 中 `args` 末尾（`--logpath` 之后）加：

```rust
        if ctx.config.get("auth_enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("--auth".to_string());
        }
```

`config_schema` 加三个字段（`dbpath` 之后）：

```rust
                ConfigField {
                    key: "auth_enabled".to_string(),
                    label_i18n: "configField.authEnabled".to_string(),
                    field_type: ConfigFieldType::Boolean,
                    default_value: serde_json::json!(false),
                    section: None,
                    description_i18n: Some("configField.authEnabledDesc".to_string()),
                },
                ConfigField {
                    key: "root_user".to_string(),
                    label_i18n: "configField.rootUser".to_string(),
                    field_type: ConfigFieldType::Text,
                    default_value: serde_json::json!("root"),
                    section: None,
                    description_i18n: Some("configField.rootUserDesc".to_string()),
                },
                ConfigField {
                    key: "root_password".to_string(),
                    label_i18n: "configField.rootPassword".to_string(),
                    field_type: ConfigFieldType::Password,
                    default_value: serde_json::json!(""),
                    section: None,
                    description_i18n: Some("configField.rootPasswordDesc".to_string()),
                },
```

- [ ] **步骤 4：验证**

运行：`cd src-tauri && cargo test --lib providers::mongodb`
预期：新增 3 测试通过（累计 9）

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/services/software_manager/providers/mongodb.rs
git commit -m "feat: MongoDB 支持可选认证（--auth）"
```

---

## 任务 14：i18n 新增认证配置文案

**文件：**
- 修改：`src/locales/zh-CN.ts`（configField 块 `consulMode` 后）
- 修改：`src/locales/en-US.ts`（同位置）

- [ ] **步骤 1：zh-CN.ts configField 块加**

```typescript
    authEnabled: '启用认证',
    authEnabledDesc: '开启后需认证才能访问；首次需在 mongosh 手动创建 root 用户',
    rootUser: 'Root 用户名',
    rootUserDesc: '供 mongosh 手动创建用户时参考，不自动创建',
    rootPassword: 'Root 密码',
    rootPasswordDesc: '供 mongosh 手动创建用户时参考，不自动创建',
```

另在通用块（如 `running: '运行中'` 旁）加 `enabled`（Boolean 开关标签）：

```typescript
  enabled: '开启',
```

- [ ] **步骤 2：en-US.ts 对照**

```typescript
    authEnabled: 'Enable Auth',
    authEnabledDesc: 'Requires authentication to access; create root user in mongosh on first run',
    rootUser: 'Root Username',
    rootUserDesc: 'Reference for manually creating user in mongosh; not auto-created',
    rootPassword: 'Root Password',
    rootPasswordDesc: 'Reference for manually creating user in mongosh; not auto-created',
```

```typescript
  enabled: 'Enabled',
```

- [ ] **步骤 3：验证**

运行：`npm run build`
预期：通过

- [ ] **步骤 4：Commit**

```bash
git add src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat: 新增认证配置 i18n 文案"
```

---

# 追加自检（2026-08-14）

**1. 规格覆盖度：** 追加 spec「初始化与认证配置」每节对应任务：
- ConfigFieldType 加 Boolean（前后端 + 渲染）→ 任务 11
- PostgreSQL 可选密码 + scram → 任务 12
- MongoDB 可选认证 → 任务 13
- i18n 认证文案 → 任务 14
**无遗漏。**

**2. 占位符扫描：** 无 TODO/占位；每个步骤含完整代码。任务 12 的敏感 pwfile 放系统 tmp_dir，避免 app 目录残留。

**3. 类型一致性：** `ConfigFieldType::Boolean` 在任务 11 前后端一致；i18n key `configField.authEnabled/authEnabledDesc/rootUser/rootUserDesc/rootPassword/rootPasswordDesc` + 通用 `enabled` 在 provider（任务 13）与 i18n（任务 14）一致。init_password 复用现有 key `configField.initPassword/initPasswordDesc`。

**3. 类型一致性：** Provider 结构体名 `PostgreSqlProvider`/`MongoDbProvider`/`ConsulProvider` 在任务 4/5/6 定义并在任务 7 注册一致；`SoftwareCategory::Registry` 在任务 1（后端）与任务 2（前端）一致；i18n key `catalogDesc.postgresql`/`catalogDesc.mongodb`/`catalogDesc.consul`、`configField.listenAddresses`/`sharedBuffers`/`bindIp`/`httpPort`/`consulateMode`、mirror 名 `postgresqlOfficial`/`mongodbOfficial`/`consulOfficial` 在 provider 实现（任务 4-6）与 i18n（任务 8）中一致；`SoftwareListPage` 的 `registry` 组 label 用 `registry`（任务 9）对应 i18n `registry` key（任务 8）。无命名漂移。