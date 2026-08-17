# C 扩展方向：实例运维能力（日志查看器 + 备份/恢复）简单 PRD

> 文档类型：简单 PRD（产品目标 / 用户故事 / 需求池 P0-P2 / UI 设计稿 / 待确认问题）
> 所属扩展方向：opx 三大扩展方向之 **C（第一项）**——为已安装/运行中的实例提供运维能力
> 关联范围：**B**（一键启动栈编排）、**A**（新增 provider Kafka/ES/InfluxDB）不在本 PRD 内，但本能力按"实例通用"设计，B/A 可直接复用。

## 变更记录

- 2026-08-17：初版简单 PRD（基于现有 `SoftwareProvider` 架构与实例数据模型梳理）。

## 概述

opx 当前已能对中间件实例做安装、初始化、启动、健康检查、改配置、停止、卸载，但**缺失排障与数据保护能力**：用户无法在桌面端查看运行日志、无法对实例数据做快照备份与恢复、无法一键把实例数据重置回初始状态。本 PRD 面向**已安装/运行中的实例**，新增两类能力：

1. **日志查看器**：历史日志浏览、实时刷新（tail）、关键字/级别过滤、下载。
2. **备份/恢复**：data 目录快照（压缩归档）、快照列表、恢复、删除，以及一键重置（清空 data 回初始）。

## 范围

- 面向**全部常驻运行实例**（Database / Cache / Registry / WebServer / ObjectStorage 等分类），按实例通用。
- **Runtime 类（jdk/jre）排除**：它们由业务应用拉起、不作为独立常驻进程，无独立日志/可备份 data，能力入口对其禁用。
- **不在 C 范围**：B=批量启停栈编排（复用现有 auto-start）；A=新增 provider。本 PRD 仅依赖现有 `SoftwareProvider` trait 与 `InstalledSoftware` 模型，不新增软件类型。

### 与现有架构的对齐（设计约束）

- **日志来源差异（已确认现状）**：
  - `lifecycle.rs` 当前 `spawn_process` 将 `stdout/stderr → Stdio::null()`（**直接丢弃**）；因此"进程 stdout 重定向文件"目前不存在，日志查看器要覆盖此来源，**必须先改 lifecycle 把 stdout/stderr 重定向落盘**。
  - 部分 provider 自带日志文件：如 MongoDB `start_command` 含 `--logpath <install>/data/mongod.log`；MySQL 用 `--log-error` 落文件。这些属于"provider 写日志文件"来源。
  - ⇒ 日志能力需**统一抽象两类来源**，不能只认一种。
- **data 目录约定（已确认现状）**：多数 provider 用 `<install_path>/data`（MySQL/PostgreSQL/MongoDB/MinIO/RustFS），其中 MinIO/RustFS 的 `data_dir` 可由表单配置（相对/绝对路径）。已有 `wipe_data_dir_if_nonempty(data_dir)` 辅助函数，一键重置可复用其思路并泛化到可配置 data_dir。
- **通用性要求**：日志来源与 data 目录均通过 **provider trait 新增方法 + 默认实现** 暴露，新 provider（A 方向）无需改动即自动获得日志/备份能力；B 方向编排启停时也可调用同一快照/重置接口。

## 产品目标

1. **排障自助化**：用户无需命令行即可查看任意已装实例的运行日志（历史 + 实时），按关键字/级别过滤，快速定位启动失败、连接、配置类故障。
2. **数据可保护**：用户能对实例 data 目录做快照备份与一键恢复，降低升级、改配置、误操作导致的数据丢失风险。
3. **状态可重置**：实例数据被玩坏或需重新初始化时，可一键清空 data 回到干净初始状态，免去重装软件。

## 用户故事

- 作为开发者，我想查看正在运行的 MySQL/Redis 实例的实时日志并按 `ERROR` 过滤，以便快速排查启动失败或连接被拒。
- 作为开发者/运维，我想在升级版本或改动重大配置前对某实例创建快照，出错时一键恢复，避免手动备份目录的繁琐与遗漏。
- 作为开发者，我想在实例 data 被测试数据污染后"一键重置"回初始状态，而不必卸载重装。
- 作为多实例用户，我想在日志查看器中切换/对比多个实例的日志（如 Nacos 连不上 MySQL 时对照两边），以便定位依赖问题。

## 需求池

### P0（必须有 / 本次交付核心）

**日志查看器**
- P0-L1 历史日志浏览：按实例列出其全部 `LogSource`，打开后默认读取末尾 N 行（默认 2000），支持"向前加载更多历史"分页（步长可配，默认 2000）。
- P0-L2 关键字过滤：对当前日志做大小写不敏感子串匹配；提供"正则模式"开关（可选）。
- P0-L3 下载日志：将当前选中的日志源文件下载到用户指定位置。
- P0-L4 多实例入口：实例卡片新增「日志」动作（Runtime 类禁用），点击进入日志查看器并默认选中该实例；支持在查看器内切换其他实例。
- P0-L5 后端前置改造：`spawn_process` 将 `stdout+stderr` 重定向到 `<install_path>/logs/opx-<pid>.log`（保证"stdout 重定向"来源可查）；provider trait 新增 `log_sources(ctx) -> Vec<LogSource>`，默认实现返回该 stdout 文件，provider 可追加自带日志文件。

**备份 / 恢复**
- P0-B1 创建快照：对实例 `data_dirs()` 做压缩归档（默认 `.zip`，跨平台用 Rust `zip`/`flate2`），存入 `<app_data>/backups/<installed_id>/<timestamp>.zip`，并写入快照元数据 json（来源 key、来源 version、大小、格式、创建时间）。
- P0-B2 快照列表：展示每实例的快照（时间 / 大小 / 来源版本 / 备注），支持下载快照。
- P0-B3 从快照恢复：选择快照覆盖当前 data 目录（恢复前提示停服与覆盖风险；运行实例须先停止）。
- P0-B4 删除快照：从列表删除指定快照，二次确认。
- P0-B5 一键重置：清空实例 data 目录回初始空态（泛化复用 `wipe_data_dir_if_nonempty` 思路，支持可配置 `data_dir`），强二次确认（勾选风险 + 输入/确认）。

**通用 / 入口**
- P0-C1 实例卡片新增「日志」「备份」动作入口；Runtime 类（jdk/jre）对两者禁用；按钮风格与现有 start/stop/config 一致。
- P0-C2 后端命令：`get_log_sources`、`read_log`、`download_log`、`create_snapshot`、`list_snapshots`、`restore_snapshot`、`delete_snapshot`、`reset_instance`。
- P0-C3 跨层：新增 `LogSource` / `SnapshotMeta` 模型（后端 `models/software.rs` + 前端 `models/software.ts`）；i18n 补充 `logs`/`backup`/`restore`/`reset`/`snapshot` 等文案；图标使用现有 mdi（如 `mdi:file-document-outline` / `mdi:backup-restore`）。

### P1（应该有）

- P1-L1 tail 实时刷新：默认轮询（1-2s）拉取日志增量并追加、自动滚动到底；提供"实时"开关。
- P1-L2 日志级别过滤：仅对 `has_levels=true` 的日志源显示级别下拉（如 ERROR/WARN/INFO）；级别由 provider 提供提取规则，无则由默认正则解析。
- P1-B1 自定义快照名称 / 备注（创建时可选填）。
- P1-B2 恢复后自动重新初始化提示：对 PG/MySQL 等需要 initdb 重建的场景，恢复后提示用户执行首次初始化。
- P1-B3 恢复一致性校验：恢复时校验快照来源 `key` 与**大版本**一致，不一致弹警告但仍允许（高级覆盖）。

### P2（锦上添花）

- P2-L1 文件监听：后端 `notify` 监听日志文件变更并增量推前端，替代/增强轮询。
- P2-L2 多实例同屏对比 / 分屏、跨实例关键字搜索。
- P2-B1 保留策略自动清理（每实例上限 N 个 / 总占用上限，清理最旧）。
- P2-B2 增量备份（仅差异块）。
- P2-L3 日志高亮、行号、时间线折叠。

## 技术规范要点（供架构师/开发参考，非最终方案）

- **新增类型**：
  - `LogSource { path: PathBuf, kind: LogSourceKind, has_levels: bool }`，`LogSourceKind::{ ProviderFile, StdoutRedirect }`。
  - `LogContext { installed_id, install_path, version, config, pid }`。
  - `SnapshotMeta { id, created_at, source_key, source_version, size, format, note: Option<String> }`。
- **trait 扩展（默认实现即可，新 provider 零改动获得能力）**：
  - `fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource>` —— 默认 `[StdoutRedirect(<install_path>/logs/opx-<pid>.log)]`；MongoDB 等追加 `ProviderFile(<install>/data/mongod.log)`。
  - `fn data_dirs(&self, ctx: &DataDirContext) -> Vec<PathBuf>` —— 默认 `[<install_path>/data]`；MinIO/RustFS 解析配置 `data_dir` 返回实际路径。
- **lifecycle 改造（P0-L5 前提）**：`spawn_process` 将 stdout+stderr 重定向至 `<install_path>/logs/opx-<pid>.log`（注意：当前为 `Stdio::null()` 是为规避管道阻塞；改为落盘文件后须确保文件句柄在进程生命周期内持有，且进程退出后文件可安全读取）。
- **快照存储**：`<app_data>/backups/<installed_id>/`，`app_data` 复用现有 `paths::data_dir()`。
- **大文件/流式读取**：`read_log` 服务端按 offset/limit 读取，tail 模式返回文件末尾 + 当前字节偏移，前端持偏移轮询增量。

## UI 设计稿（文字描述，先走 ui-ux-pro-max 评审）

### 1. 实例卡片入口（`SoftwareInstanceRow.vue`）
- 在现有 `start / stop / config / startup-settings / uninstall` 动作后新增两个按钮：
  - 「日志」`mdi:file-document-outline`：打开日志查看器（Runtime 类禁用）。
  - 「备份」`mdi:backup-restore`：打开备份/恢复面板（Runtime 类禁用）。

### 2. 日志查看器（`LogViewerDialog.vue` / 或独立页）
- **顶栏**：实例选择（默认当前实例，可下拉切换其他常驻实例）+ 实例状态徽标 + 关闭。
- **日志源标签栏**：列出该实例的 `LogSource`（如「控制台输出」「mongod.log」），点击切换。
- **工具栏**：关键字输入框（含正则开关）、级别下拉（仅 `has_levels` 时显示）、「实时」开关、下载按钮、自动滚动开关。
- **主区（日志面板）**：
  - 默认渲染末尾 N 行（等宽字体、可滚动）。
  - 历史分页：顶部「加载更多历史」向上追加；tail 开启时底部持续追加新行并自动滚动。
  - 过滤命中高亮；超大文件不一次性载入（服务端分页/流式）。
- **多实例（P2）**：左侧实例列表或分屏对比。

### 3. 备份 / 恢复面板（`BackupRestoreDialog.vue`）
- **Tab A — 快照列表**：
  - 顶部「创建快照」按钮（可选名称/备注；勾选"先停止实例"）。
  - 表格：名称/时间/大小/来源版本/备注；行操作：恢复 / 下载 / 删除。
  - 恢复 → 确认弹窗（停服提示 + 覆盖当前 data 警告 + 大版本不一致警告 P1）。
  - 删除 → 二次确认。
- **Tab B — 一键重置**：
  - 说明文案（清空 data 目录、回到初始空态、需重新初始化）。
  - 「重置」按钮 → 强确认弹窗（勾选风险确认 + 输入实例名/确认），确认后执行。

## 待确认问题（需用户拍板）

1. **实时刷新机制**：默认用前端轮询（1-2s 拉增量，实现简单、桌面端足够），还是后端文件监听（notify 推增量，更复杂）？建议 P0 用轮询，文件监听放 P2。是否认可？
2. **备份是否强制停服**：停服保证 data 一致性（尤其 MySQL/PG 写放大中）；不停服快照可能不一致。建议提供"停服备份 / 热备"两种模式、默认提示停服。默认行为与是否允许热备请确认。
3. **备份存放位置与保留策略**：默认 `<app_data>/backups/<id>/`？是否需要自动清理（每实例上限 N 个 / 总占用上限）？上限数值与是否自动清理请确认（若先不做自动清理，删除全靠手动）。
4. **快照命名与备注**：默认 `时间戳.zip` + 记录来源版本；是否需要用户自定义名称/备注字段（P1）？
5. **一键重置确认强度**：是否与卸载等危险操作看齐（勾选风险 + 输入实例名确认）？建议强确认。
6. **日志级别过滤适用范围**：仅 provider 写结构化且有级别行（MySQL/PG/Mongo 等）才显示级别筛选；stdout 重定向/无级别日志隐藏该筛选。级别解析规则（provider 提供提取器 vs 默认正则）请确认。
7. **大文件读取默认行数**：默认展示末尾行数（建议 2000）与向前分页步长（建议 2000）是否合适？是否默认提供"仅错误"快捷视图？
8. **跨版本恢复兼容性**：快照仅存 data，跨大版本（如 PG15→16）恢复可能不兼容。是否在恢复时校验 key+大版本一致并警告（P1），还是完全不校验、由用户自负？
