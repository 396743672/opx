# 简单 PRD：A 扩展 — 消息队列 / 搜索 / 时序 Provider

- **文档类型**：简单 PRD（software-product-manager-2 产出）
- **日期**：2026-08-17
- **语言**：中文
- **技术栈**：Tauri (Rust) + React + MUI + Tailwind（与既有 software-manager 一致）
- **原始需求复述**：在现有软件管理框架中新增三类中间件 Provider —— Kafka（消息队列）、Elasticsearch（搜索）、InfluxDB（时序数据库），支持目录注册、下载安装、配置、健康检查、启停；Kafka / ES 复用用户已配置的 JDK 启动，InfluxDB 为原生二进制免 JVM。

---

## 1. 产品目标（Product Goals）

- **G1 — 能力补齐**：在既有 `SoftwareProvider` 框架内新增 Kafka / Elasticsearch / InfluxDB 三类中间件的完整生命周期（catalog 注册 → 下载解压 → 配置 → 启停 → 健康检查 → 日志/数据目录），不另起炉灶。
- **G2 — JDK 复用**：Java 类中间件（Kafka、ES）复用用户已安装的 JDK/JRE（与 Nacos 同一机制：`StartContext.jdk_install_path` + `fill_jdk_options`），避免重复打包 JDK；InfluxDB 作为 Go 原生二进制，免 JVM 直接运行。
- **G3 — 统一体验**：三类软件接入统一的目录聚合、安装进度、运行状态可视化、配置表单、日志查看与数据备份能力，前端按分类展示并可配置。

---

## 2. 用户故事（User Stories）

- 作为本地开发者，我希望一键安装并启动 Kafka，并复用我已装的 JDK，无需手动设置 `JAVA_HOME`。
- 作为本地开发者，我希望一键安装并启动 Elasticsearch 做全文检索/日志分析，且端口、堆内存、数据目录可配。
- 作为本地开发者，我希望一键启动 InfluxDB 做时序数据存储，免 JVM、启动轻量、端口可配。
- 作为使用者，我希望在统一的软件仓库/列表里按分类（消息队列 / 搜索 / 时序）浏览这三类软件并查看运行状态。
- 作为使用者，我希望在配置面板里为 Kafka/ES 选择已安装的 JDK，未安装时得到明确提示。

---

## 3. 需求池（Requirements Pool）

优先级：**P0** 必须 / **P1** 应当 / **P2** 可选。P0 覆盖三个 provider 的 catalog 注册 + 下载安装 + 配置 + 健康检查 + 启停，以及 Kafka/ES 用已配 JDK、InfluxDB 免 JVM。

### P0 — 必须

**A-P0-1 Kafka 目录注册（catalog_entry）**
- 描述：实现 `KafkaProvider`，在 `providers/mod.rs` 新增 `pub mod kafka;` 并在 `all_providers()` 注册。提供 `catalog_entry()`：key=`kafka`，category 待定（见 Open Questions），icon=`mdi:message-text-outline`，至少 1 个默认版本（如 3.7.x），`versions` 含 `mirrors` 指向官网 `.tar.gz`，`archive` 用 `ArchiveFormat::TarGz`。
- 验收：构建内置 catalog 后 `kafka` 出现在列表中；`all_providers()` 含 `KafkaProvider`；`key()` 返回 `"kafka"`。

**A-P0-2 Kafka 下载 / 解压安装**
- 描述：利用既有 `installer.rs` 的 `ArchiveFormat::TarGz` 路径（下载 → `cache_dir()/kafka/{ver}.tar.gz` → `extract_tar_gz` 到 `apps_dir()/kafka/{ver}`）。Kafka 官方包为 `.tgz`，无需 `post_install` 特殊处理。
- 验收：安装后 `install_path` 下存在 `bin/kafka-server-start.sh`（及 `bin/windows/kafka-server-start.bat`）；安装进度事件（`downloading`/`extracting`/`completed`）正确触发；SHA256 若填则校验通过。

**A-P0-3 Kafka 配置 Schema + JDK 复用 + 配置文件**
- 描述：`config_schema()` 含字段：`port`（默认 9092，Port 类型）、`jdk`（Select，options/labels 由命令层填充，复用 `fill_jdk_options`）、`heap`（Size 单位 m/g，默认如 `1g`）、`log_dirs`（Text，默认 `data/kafka-logs`）；`config_file_path()` 返回 `config/server.properties`。配置写入由既有 `config_editor` 处理。
- 验收：`get_config_schema` 对 kafka 返回含 JDK 下拉的 schema；未装 JDK 时 `jdk` 选项为空、前端提示先装 JDK。

**A-P0-4 Kafka 启动 / 停止**
- 描述：`start_command()` 设 `env_vars["JAVA_HOME"] = jdk_install_path`，程序为 `bin/kafka-server-start.sh`（Windows 用 `bin/windows/kafka-server-start.bat`），`args = ["config/server.properties"]`，`working_dir = install_path`，`creation_flags = CREATE_NO_WINDOW`。从 `ctx.jdk_install_path` 解析（None 时返回明确错误「请先安装 JDK 并在配置中选择」）。停止走既有 lifecycle（杀 PID）。
- 验收：选中已装 JDK 时可前台启动；状态变为 Running；停止后进程退出、状态 Stopped。

**A-P0-5 Kafka 健康检查**
- 描述：`health_check()` 返回 `HealthCheckSpec::Tcp { port: 9092, timeout_ms: 1000 }`（取 `config.port` 或默认）。
- 验收：Kafka 监听 9092 时健康检查通过；未监听时失败。

**A-P0-6 Elasticsearch 目录注册（catalog_entry）**
- 描述：实现 `ElasticsearchProvider`，`providers/mod.rs` 新增 `pub mod elasticsearch;` 并注册。key=`elasticsearch`，icon=`mdi:magnify`，至少 1 个默认版本（如 8.x），`archive` 用 `TarGz`，mirror 指向 elastic.co 官方 `.tar.gz`。
- 验收：catalog 含 `elasticsearch`；`key()` 返回 `"elasticsearch"`。

**A-P0-7 ES 下载 / 解压安装**
- 描述：复用 `ArchiveFormat::TarGz` 下载解压到 `apps_dir()/elasticsearch/{ver}`。官方包含 `bin/elasticsearch`（Unix）/ `bin/elasticsearch.bat`（Windows）与捆绑 `jdk/`。
- 验收：安装后 `install_path/bin/elasticsearch` 存在；进度事件正确。

**A-P0-8 ES 配置 Schema + ES_JAVA_HOME 复用 + 配置文件**
- 描述：`config_schema()` 含 `port`（默认 9200，Port）、`jdk`（Select，复用 `fill_jdk_options`）、`heap`（Size，默认 `1g`）、`network_host`（Text，默认 `127.0.0.1`，单机需 `network.host` 或 `discovery.type=single-node`）、`data_path`（Text，默认 `data`）；`config_file_path()` 返回 `config/elasticsearch.yml`。启动前置 `ES_JAVA_HOME = jdk_install_path`（满足「复用已配 JDK」要求；ES 自带 `jdk/` 的取舍见 Open Questions）。
- 验收：ES 用所选 JDK 启动；单机模式（`discovery.type=single-node`）可正常启动，不触发集群引导报错。

**A-P0-9 ES 启动 / 停止**
- 描述：`start_command()` 设 `env_vars["ES_JAVA_HOME"] = jdk_install_path`，程序为 `bin/elasticsearch`（Windows `bin/elasticsearch.bat`），`args = []`，`working_dir = install_path`，`creation_flags = CREATE_NO_WINDOW`。None JDK 时返回明确错误。
- 验收：选中已装 JDK 可启动；状态正确切换。

**A-P0-10 ES 健康检查**
- 描述：`health_check()` 返回 `HealthCheckSpec::Tcp { port: 9200, timeout_ms: 1000 }`（或 `Http` 指向 `http://127.0.0.1:9200`，期望 200；TCP 更稳，建议 TCP）。
- 验收：9200 监听时通过。

**A-P0-11 InfluxDB 目录注册（catalog_entry）**
- 描述：实现 `InfluxdbProvider`，`providers/mod.rs` 新增 `pub mod influxdb;` 并注册。key=`influxdb`，icon=`mdi:chart-line`，至少 1 个默认版本（如 2.x），`archive` 用 `TarGz`，mirror 指向 `get.influxdb.org` 官方 `.tar.gz`。
- 验收：catalog 含 `influxdb`；`key()` 返回 `"influxdb"`。

**A-P0-12 InfluxDB 下载 / 解压安装**
- 描述：复用 `ArchiveFormat::TarGz` 到 `apps_dir()/influxdb/{ver}`。官方包解压后为单一可执行 `influxd`（及 `influx` CLI）。
- 验收：安装后 `install_path/influxd[.exe]` 存在。

**A-P0-13 InfluxDB 配置 Schema（免 JVM）**
- 描述：`config_schema()` 含 `port`（默认 8086，Port）、`bolt_path`（Text，默认 `data/influxd.bolt`）、`engine_path`（Text，默认 `data/engine`）、`auth_enabled`（Boolean，默认 true）、`admin_user` / `admin_password`（Password，`ephemeral_keys` 标记，不落盘）。**不**包含 `jdk` 字段（免 JVM）。
- 验收：配置表单无 JDK 下拉；可设置端口与数据目录。

**A-P0-14 InfluxDB 启动 / 停止（原生二进制）**
- 描述：`start_command()` 直接运行 `install_path/influxd`（Windows `influxd.exe`），`args = ["--http-bind-address=:8086", "--bolt-path=...", "--engine-path=..."]`（或读 `config.yml`），`env_vars` 为空，`working_dir = install_path`。**不使用 `jdk_install_path`**。停止走 lifecycle 杀 PID。
- 验收：可启动并监听 8086；状态正确切换；无需 JDK。

**A-P0-15 InfluxDB 健康检查**
- 描述：`health_check()` 返回 `HealthCheckSpec::Http { url: "http://127.0.0.1:8086/ping", expected_status: 204, timeout_ms: 1000 }`（InfluxDB v2 `/ping` 返回 204）。
- 验收：监听 8086 时 `/ping` 通过。

**A-P0-16 注册三新模块到 all_providers()**
- 描述：在 `providers/mod.rs` 的 `all_providers()` 向量中追加 `kafka::KafkaProvider::new()`、`elasticsearch::ElasticsearchProvider::new()`、`influxdb::InfluxdbProvider::new()`，并补齐 `pub mod`。
- 验收：单元测试/代码编译通过，`all_providers().len()` 增加 3。

**A-P0-17 扩展 get_config_schema 的 JDK 选项填充**
- 描述：将 `commands/software.rs` 中 `get_config_schema` 的 `if software.key == "nacos"` 触发条件扩展为覆盖 `kafka` 与 `elasticsearch`（即凡 schema 含 key=`jdk` 的 Select 字段，均调用 `fill_jdk_options`）。`fill_jdk_options` 本身可复用。
- 验收：kafka/elasticsearch 的配置 schema 的 `jdk` 字段被填充为已装 JDK/JRE 列表（options=installed_id，labels=`name (version) [JDK/JRE]`）。

### P1 — 应当

**A-P1-1 新增分类枚举 + 前端 + i18n**
- 描述：在 `models/software.rs` 的 `SoftwareCategory` 枚举新增 `MessageQueue`、`Search`、`TimeSeries`；前端 `stores/catalog.ts` 分组与 `pages/SoftwareListPage.vue` 的 category map 同步扩展；`locales/zh-CN.ts`、`en-US.ts` 补 `categoryMessageQueue`/`categorySearch`/`categoryTimeSeries`。
- 验收：三类软件分入对应分类区块展示；i18n 有对应文案。

**A-P1-2 日志来源接入**
- 描述：Kafka/ES/InfluxDB 除默认 stdout 重定向外，覆写 `log_sources()` 追加自带日志文件（如 Kafka 的 `logs/server.log`、ES 的 `logs/elasticsearch.log`、InfluxDB 输出重定向）；需结构化级别日志者覆写 `log_level_pattern()`。
- 验收：运行面板日志页可切换并查看对应日志源。

**A-P1-3 数据目录备份接入**
- 描述：覆写 `data_dirs()` 返回真实数据目录（Kafka `log.dirs`、ES `path.data`、InfluxDB `bolt/engine` 路径），使既有备份/恢复能力生效。
- 验收：备份快照包含正确数据目录。

**A-P1-4 InfluxDB 首次初始化（可选）**
- 描述：利用 `FirstRunInit` 钩子，在首次启动后通过 `influx setup` 创建初始组织/桶/管理员（消费 `admin_user`/`admin_password`，一次性、不落盘）。
- 验收：首次启动后可用管理员账号访问；二次启动跳过。

### P2 — 可选

**A-P2-1 动态版本拉取**
- 描述：三个 provider 实现 `fetch_remote_versions()`（Kafka/ES 解析官网/Artifacts API，InfluxDB 解析 GitHub Releases），与内置版本合并（复用 `merge_versions`）。
- 验收：联网时版本列表含动态版本且去重。

**A-P2-2 启动顺序 / 自动启动**
- 描述：支持 `auto_start_on_app_start` 与 `startup_order`（如 Kafka 依赖 Zookeeper 内嵌则无需外部顺序；如外置依赖可配置）。
- 验收：应用启动时按序自动拉起已勾选软件。

**A-P2-3 文档 / 外链**
- 描述：配置面板提供「官方文档」外链按钮。
- 验收：点击跳转对应官网文档。

---

## 4. UI 设计稿（UI Design Draft）

复用既有 software-manager 前端（React + MUI + Tailwind），不新增页面，仅扩展既有组件：

1. **软件仓库页（RepositoryPage / SoftwareListPage）**
   - 分类区块新增「消息队列 / 搜索 / 时序」（取决于 A-P1-1 是否落地新枚举；落地前 P0 阶段可临时归入现有分类，见 Open Questions）。
   - 每个软件卡片：图标、名称、默认版本、已装版本、状态徽标（Running/Stopped/Error）。

2. **配置抽屉（ConfigSheet）**
   - Kafka / ES：渲染 `jdk` 为下拉（options=已装 JDK/JRE 的 installed_id，labels=`name (version) [JDK/JRE]`）；未装任何 JDK 时显示红字提示「请先安装 JDK/JRE」并禁用启动。
   - 端口、堆内存（m/g 单位下拉）、数据目录等按 schema 渲染。
   - InfluxDB：无 JDK 字段；显示端口、数据目录、认证开关、管理员账号（Password，红色 ephemeral 提示）。

3. **运行面板（RunPanel）**
   - 状态、启停按钮、日志页签（多日志源切换）、数据备份/恢复入口，全部复用现有组件，无需改动 UI 代码（能力由 provider 默认实现提供）。

---

## 5. 待确认问题（Open Questions）

1. **分类枚举**：现有 `SoftwareCategory` 仅 `Database/Runtime/Cache/WebServer/Registry`，无 MessageQueue/Search/TimeSeries。是否新增三个枚举值？新增需同步改 `models/software.rs`、`stores/catalog.ts`、`SoftwareListPage.vue` 的 category map 与 i18n。注意：`SoftwareListPage.vue` 已存在 `storage` 分类项但非枚举值，存在潜在不一致，需架构师核对现有分组契约。
2. **Kafka 启动方式**：采用官方 `bin/kafka-server-start.sh`（设 `JAVA_HOME` 前台运行）还是自行拼 `java -cp ...`？建议用脚本（前台、简单、便于复用已配 JDK）。需确认 Windows `bin/windows/*.bat` 路径与 daemon 化问题（参考 Nacos 不用 startup 脚本的原因）。
3. **ES 的 JDK 取舍**：需求要求「ES 用已配 JDK 启动」，但 ES 官方包自带 `jdk/` 捆绑 JDK。确认是强制 `ES_JAVA_HOME=已配 JDK`（满足复用、但版本需匹配 ES 要求 JDK 17+），还是优先用其自带 JDK？本 PRD P0 按需求采用 `ES_JAVA_HOME`。
4. **默认版本与下载源**：Kafka / ES / InfluxDB 的默认稳定版本号与官网 `.tar.gz` 直链待定（需在执行阶段核对当前稳定版、平台包名与体积，确认是否走 builtin 离线包或在线 mirror）。
5. **JDK 版本兼容**：Kafka 3.x 需 JDK 11+，ES 8.x 需 JDK 17+；`fill_jdk_options` 下拉是否应按软件需求过滤不兼容 JDK（如 Kafka 隐藏 JDK8、ES 隐藏 JDK8/11）？
6. **端口冲突**：Kafka 9092 / ES 9200(+9300 内部) / InfluxDB 8086 与既有软件（Nacos 8848 等）默认不冲突，但需确认 9300（ES 传输端口）是否纳入 `collect_configured_ports` 占用校验。

---

## 附：实现要点（基于真实代码，供架构师参考）

- Provider 骨架见 `src-tauri/src/services/software_manager/providers/mod.rs`（`SoftwareProvider` trait）与 `nacos.rs`（JDK 复用范例）。
- JDK 解析：`commands/software.rs` 的 `find_installed_jdk(manager, config)` —— 优先 `config.jdk`（installed_id），回退首个 jdk/jre；结果写入 `StartContext.jdk_install_path`。
- JDK 下拉填充：`commands/software.rs` 的 `fill_jdk_options`（当前仅 nacos 触发，需扩展到 kafka/es）。
- 安装/解压：`installer.rs` 的 `install_software` 已支持 `TarGz`（`extract_tar_gz`）、SHA256 校验、进度事件。
- 健康检查：`models/software.rs` 的 `HealthCheckSpec::{Tcp, Http, ProcessOnly}`；命令层 `health_check` 模块消费。
- 目录聚合：`catalog.rs` 的 `build_builtin_catalog()` 遍历 `all_providers()`；新增 provider 自动进入目录。
- 前端：`modules/software-manager/stores/catalog.ts`（按 `SoftwareCategory` 分组）、`pages/SoftwareListPage.vue`（分类展示）、`locales/*`（分类 i18n）。
