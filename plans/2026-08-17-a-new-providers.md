# A 扩展（消息队列 / 搜索 / 时序 Provider）实施计划

> 文档类型：实施计划（Plan）
> 分支：`feat/a-new-providers`
> 对应 PRD：`2026-08-17-a-new-providers-prd.md`
> 对应设计：`2026-08-17-a-new-providers-design.md`
> 文档语言：中文

## ⚠️ 依赖与包约束（关键）

- **不引入任何新 crate**：后端沿用现有 `tauri` / `serde` / `anyhow` / `sha2` / `chrono`；复用 `SoftwareProvider` 框架、`installer`、`commands/software.rs`、`health_check`、`lifecycle`、`config_editor`（仅支持 Ini/Json 等既有格式）。
- **不引入任何新 npm 包**：前端沿用 `vue` / `pinia` / `tailwindcss` / `@tauri-apps/api` / `vue-i18n` / `@iconify/vue`。
- 所有新增能力通过**新增 Provider 文件 + 注册进 `all_providers()` + 复用现有命令/组件**实现，不改变既有依赖清单（Cargo.toml / package.json 无需改动）。
- **架构师不编译**（用户已关闭沙箱）；编译/测试由工程师在物理机实跑（T6）。

---

## 文件清单（File Manifest）

### 后端（新增 / 修改）

| 文件 | 动作 | 说明 |
| --- | --- | --- |
| `src-tauri/src/services/software_manager/providers/kafka.rs` | 新增 | `KafkaProvider` 实现 `SoftwareProvider`（catalog/start/health/schema/log/data_dirs + 生成 server.properties） |
| `src-tauri/src/services/software_manager/providers/elasticsearch.rs` | 新增 | `ElasticsearchProvider`（ES_JAVA_HOME 复用 + 生成 elasticsearch.yml） |
| `src-tauri/src/services/software_manager/providers/influxdb.rs` | 新增 | `InfluxdbProvider`（原生二进制免 JVM + FirstRunInit 钩子 P1） |
| `src-tauri/src/services/software_manager/providers/mod.rs` | 修改 | 3×`pub mod` + `all_providers()` 追加 3 个 `::new()` + trait 新增默认方法 `min_jdk_version` |
| `src-tauri/src/models/software.rs` | 修改 | `SoftwareCategory` 新增 `MessageQueue`/`Search`/`TimeSeries`/`Storage` |
| `src-tauri/src/commands/software.rs` | 修改 | `get_config_schema` JDK 触发条件改为「schema 含 key=jdk」；`fill_jdk_options` 增加 `min_jdk_version` 过滤 |
| `src-tauri/src/services/software_manager/providers/minio.rs` | 修改 | `category` 由 `Database` 改为 `Storage`（治理不一致） |
| `src-tauri/src/services/software_manager/providers/rustfs.rs` | 修改 | 同上 |

### 前端（新增 / 修改）

| 文件 | 动作 | 说明 |
| --- | --- | --- |
| `src/models/software.ts` | 修改 | `SoftwareCategory` 枚举同步新增 4 个值 |
| `src/modules/software-manager/stores/catalog.ts` | 修改 | `groupedEntries.groups` 增加 4 个空分组 |
| `src/modules/software-manager/pages/SoftwareListPage.vue` | 修改 | `grouped` 新增 messagequeue/search/timeseries 分组；minio/rustfs→storage |
| `src/locales/zh-CN.ts` | 修改 | 新增 `messageQueue`/`search`/`timeSeries` 及 `configField.*` 词条 |
| `src/locales/en-US.ts` | 修改 | 同上（英文） |

---

## 任务列表（Tasks，复选框 + 依赖 + 验收标准）

> 格式：`T{n} — 名称 [依赖] (P级, 覆盖需求)`
> 验收标准均为「可编译 / 可运行 / 行为符合 PRD 与本设计 §3~§7 的客观判定」。

### T1 — 分类枚举扩展与 storage 治理
**依赖**：无　**P0**　**覆盖**：OpenQ1、A-P1-1（枚举）
**涉及文件**：`src-tauri/src/models/software.rs`（修改）、`src/models/software.ts`（修改）
- [ ] 后端 `SoftwareCategory` 枚举新增 `MessageQueue` / `Search` / `TimeSeries` / `Storage`（serde 自动派生）。
- [ ] 前端 `src/models/software.ts` 枚举同步新增四个值。
- [ ] `cargo build` 与 `vite build` 通过（枚举仅增加变体，不影响既有序列化）。

### T2 — 三个 Provider 实现 + 注册
**依赖**：T1　**P0**　**覆盖**：A-P0-1~A-P0-16、A-P0-3/8/13、A-P0-4/9/14、A-P0-5/10/15、A-P1-2、A-P1-3
**涉及文件**：`providers/kafka.rs`（新）、`providers/elasticsearch.rs`（新）、`providers/influxdb.rs`（新）、`providers/mod.rs`（修改）
- [ ] 三文件按设计 §3.3~§3.6 实现全部 trait 方法；版本与官方 URL 写入 `catalog_entry().versions/mirrors`（Kafka 3.9.2、ES 8.19.0、InfluxDB 2.9.1，平台 `#[cfg]` 选取）。
- [ ] `config_file_path()` 返回 `None`；Kafka/ES 在 `start_command` 内生成 `config/server.properties` / `config/elasticsearch.yml`（§1.4）。
- [ ] `elasticsearch.rs` 覆写 `min_jdk_version()->Some(17)`；`kafka.rs` `->Some(11)`；`influxdb.rs` 默认 `None`。
- [ ] `mod.rs` 增加 `pub mod kafka/elasticsearch/influxdb` 并在 `all_providers()` 追加 `::new()`。
- [ ] `cargo build` 通过；`all_providers().len()` 增加 3；`build_builtin_catalog` 目录含 kafka/elasticsearch/influxdb。

### T3 — 命令层 JDK 填充扩展 + 最低版本过滤
**依赖**：T1（需 `min_jdk_version` 默认方法）　**P0**　**覆盖**：A-P0-17、OpenQ5
**涉及文件**：`src-tauri/src/commands/software.rs`（修改）
- [ ] `get_config_schema` 触发条件：`if software.key == "nacos"` → 「任意 `field.key == "jdk"` 的 `ConfigField` 存在即填充」。
- [ ] `fill_jdk_options(schema, manager, min_jdk_version: Option<u32>)`：解析已装 JDK 主版本（处理 `1.8.0`→8、`17.0.x`→17），过滤 `< min_jdk_version`（Kafka 隐藏 <11，ES 隐藏 <17）；`options=installed_id`、`labels="name (ver) [JDK/JRE]"`。
- [ ] 调用处传入 `provider.min_jdk_version()`。
- [ ] 验证：kafka/elasticsearch 配置 schema 的 `jdk` 字段被填充；未装 JDK 时 options 为空（前端提示先装 JDK）。

### T4 — 前端分类分组 + i18n
**依赖**：T1　**P1**　**覆盖**：A-P1-1（前端）
**涉及文件**：`src/modules/software-manager/stores/catalog.ts`、`src/modules/software-manager/pages/SoftwareListPage.vue`、`src/locales/zh-CN.ts`、`src/locales/en-US.ts`（均修改）
- [ ] `catalog.ts` 的 `groupedEntries.groups` 增加 `Storage`/`MessageQueue`/`Search`/`TimeSeries` 空分组。
- [ ] `SoftwareListPage.vue` 的 `grouped` 新增 `messagequeue`/`search`/`timeseries` 分组（icon 见 §3.3）；`kafka→messagequeue`、`elasticsearch→search`、`influxdb→timeseries`、`minio/rustfs→storage`。
- [ ] 两个 locale 新增 `messageQueue`/`search`/`timeSeries` 分组词条与 `configField.*` 字段词条（沿用 `categoryRegistry`/`objectStorage` 风格）。
- [ ] `vite build` 通过；三类软件分入对应区块展示。

### T5 — InfluxDB 首次初始化（FirstRunInit 钩子，P1）
**依赖**：T2　**P1**　**覆盖**：A-P1-4
**涉及文件**：`providers/influxdb.rs`（修改）
- [ ] 首次启动（`config` 无 `initialized`）时，`first_run_init` 构造 `influx setup --username=... --password=... --org=opx --bucket=opx --force`；消费 `admin_user`/`admin_password`（ephemeral，不落盘）。
- [ ] 二次启动跳过（复用 `do_start_software` 既有 `initialized` 判跳过逻辑）。
- [ ] 管理员凭据一次性传输通道见设计 §8 待明确（推荐 P0 不做、P1 扩展 `init_secrets`）。

### T6 — 编译 / 联调 / 回归验证（工程师物理机实跑）
**依赖**：T2、T3、T4（T5 可选）　**P0（验收）**　**覆盖**：A-P0-17、整体回归
**涉及文件**：无新增（验证既有流程）
- [ ] `cargo build`（src-tauri）通过，无新增警告；`vite build` 通过。
- [ ] 端到端：安装并启动 kafka / elasticsearch（需已装兼容 JDK）/ influxdb；健康检查通过；日志源可切换；数据目录备份生效。
- [ ] 端口冲突：ES 9200+9300 均被 `collect_configured_ports` 校验。
- [ ] 兼容性：Nacos/MySQL 等不受影响；`get_config_schema` 对 nacos 仍正常填充 JDK。

---

## 需求覆盖矩阵（PRD A-P0~A-P2 → 任务）

| 需求 | 优先级 | 归属任务 | 说明 |
| --- | --- | --- | --- |
| A-P0-1~A-P0-16（三类中间件全生命周期 + 注册） | P0 | T2 | catalog/安装/配置/启停/健康/日志/数据目录 + `all_providers` 注册 |
| A-P0-17（JDK 下拉扩展） | P0 | T3 | 触发条件改为「含 key=jdk」；`fill_jdk_options` 过滤 |
| A-P1-1（分类枚举 + 前端 + i18n） | P1 | T1、T4 | 枚举新增 + storage 治理 + 前端分组 + 词条 |
| A-P1-2（日志来源） | P1 | T2 | 覆写 `log_sources` 追加自带日志文件 |
| A-P1-3（数据目录备份） | P1 | T2 | 覆写 `data_dirs` 返回真实目录 |
| A-P1-4（InfluxDB 首次初始化） | P1 | T5 | `FirstRunInit` 钩子（凭据通道见 §8） |
| A-P2-1（动态版本拉取） | P2 | 预留 `fetch_remote_versions` | 本期不实现，框架已留默认 `None` |
| A-P2-2（自动启动顺序） | P2 | 复用既有 `auto_start_on_app_start` | 不在本期范围 |
| A-P2-3（文档外链） | P2 | 前端预留 | 配置面板外链按钮，逻辑后续 |

---

## 验收总览（Definition of Done）

- [ ] 全部 P0（A-P0-1~A-P0-17）完成并端到端可用：三类软件可安装/启动/健康检查/停止，Kafka/ES 复用已配 JDK，InfluxDB 免 JVM。
- [ ] P1（A-P1-1~A-P1-4）完成：分类枚举与前端分组一致（含 storage 治理）、日志源、数据目录备份、InfluxDB 首次初始化。
- [ ] `cargo build` 与 `vite build` 通过，无新增 crate / npm 包、无破坏现有能力。
- [ ] 设计 §6 六个 Open Questions 决策已全部落地（分类枚举+storage 治理、Kafka 脚本启动、ES_JAVA_HOME、联网核实版本 URL、JDK 版本过滤、ES 9300 端口校验）。
- [ ] 设计 §8 待明确事项均有明确默认值或推荐方案（InfluxDB 凭据通道推荐 P0 不做、P1 扩展）。
