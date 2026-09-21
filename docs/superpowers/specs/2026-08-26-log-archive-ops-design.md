# OPX 日志归档查看 · 服务组增强 · Node.js 设计文档

> 日期：2026-08-26
> 分支：`dev`（任务分支从 dev 新建）
> 前序：roadmap-2（R1-R8 已完成合入 dev）、extensions（7 项已合并）。本批为第三批扩展，聚焦「日志归档无缝查看」与「服务组运行增强」。

## 1. 背景与目标

1. **日志归档不可见**：Spring Boot 应用日志为 logback 按 level 拆分（`debug.log`/`info.log`/`error.log`/`warn.log`），归档为 `logs/<日期目录>/<type>.<日期>.<n>.log.gz`；软件日志（nginx/MySQL 等）经 logrotate daily + compress 产生 `xxx-YYYYMMDD.gz`。当前 `log_viewer.rs` / `read_springboot_log` 只按单个文件读字节，**看不到任何历史归档**。
2. **服务组运行增强缺位**：`StackRunPanel` 有实时成员状态，但缺「最近一次启动报告」（成员耗时/结果）、端口链接与日志快捷入口。
3. **Node.js 缺失**：无 Node.js Runtime 安装项，未来 Node 应用管理无基座。
4. **Nacos 集群是硬报错**：`mode=cluster` 直接 `Err`，无扩展点铺垫。

## 2. 范围

**In（本期实现）：**
- A. 栈启动报告（持久化最近一次到 stack 记录，RunPanel 展示）。
- B. 运行面板：成员端口链接 + 日志快捷入口（复用两套现有日志查看器）。
- C. 日志归档无缝续接：
  - C-1 软件日志（`LogViewerDialog` + `log_viewer.rs`）：主文件 + 同目录滚动归档（`*.gz` / `*.N`）历史无缝续接。
  - C-2 应用日志（`springboot LogViewer.vue` + `read_springboot_log`）：按 level 多源（debug/info/error/warn）+ 日期目录 `.gz` 归档续接。
- D. Nacos 集群扩展点预留：`mode` 保留 cluster 选项但**前置置灰 + 提示**（不实现集群逻辑）。
- E. Node.js Runtime provider：目录注册 + 内置 LTS + 动态版本拉取，纯安装不启动。

**Out（本期不做）：**
- 快照保留自动清理（`backup.rs` 每实例 5 个滚动保留**已实现**），仅评估是否需要「上限可配置」。
- 日志时间范围过滤、导出合并、文件监听推送（c-ops-logs-backup P2）。
- 应用日志按「日期目录」的**未压缩 text** 归档（logback 通常 gz；若识别规则天然覆盖则顺带支持，不刻意）。
- Nacos 集群真实实现。

## 3. 总体方案

- 日志侧：把「一个日志源」抽象为「主文件 + 有序历史归档序列」。`read` 时**历史翻页到当前文件头自动切到更旧归档尾部**，前端「加载更早」无需改交互即无缝续接；`.gz` 读取用 `flate2` 解压到内存后复用现有字节 offset / 过滤逻辑。软件与应用共用同一套通用读取函数。
- 服务组侧：`Stack.last_run_report` 持久化最近一次启动结果；RunPanel 增加报告区与成员操作按钮。
- Node：参照 `jre.rs` 的 Runtime 定位新增 provider。

## 4. 子功能设计

### A. 栈启动报告（持久化最近一次）

**现状**：`StackMemberRuntime { ref_id, status, message }`，`start()` 有逐层编排/并发/回滚/外部依赖管理，但无耗时与结果聚合。

**后端改动**
- `models/stack.rs`：
  ```rust
  pub struct StackRunReport {
      pub started_at: String,          // RFC3339 启动发起时刻
      pub total_elapsed_ms: u64,       // 整栈启动总耗时（发起→最终 Running/Failed）
      pub members: Vec<StackMemberReport>,
  }
  pub struct StackMemberReport {
      pub ref_id: String,
      pub status: StackMemberStatus,   // Running / Failed / Stopped
      pub elapsed_ms: u64,             // 该成员达到终态耗时（0 表示未启动/跳过）
      pub message: String,             // 终态 message（失败原因等）
  }
  ```
  `Stack` 增 `#[serde(default, skip_serializing_if = "Option::is_none")] last_run_report: Option<StackRunReport>`。
- `stack_manager.rs::start()`：
  - 进入时记录 `started_at`（`chrono::Local::now().to_rfc3339()`）与 `Instant`。
  - 每个成员（含外部依赖）启动前取 `t0`，到达终态（Running/Failed/Stopped）时取 `t1`，`elapsed_ms = t1 - t0`。
  - 结束（成功 / 外部依赖失败回滚 / 组内失败回滚）统一在 `finally` 聚合写 `last_run_report` 并 `self.save_dirty(stack)`。
  - 失败回滚的成员已停止，`status` 记录为最终观测值；`total_elapsed_ms` 为 start() 全程。
- 不新增命令：RunPanel 已持有完整 stack 对象（含 `last_run_report`），前端直接读。

**前端改动（`StackRunPanel.vue`）**
- 在成员状态区下方/上方加「最近一次启动报告」区块：
  - 有 `last_run_report` 时：显示整栈耗时 `total_elapsed_ms` + 每成员耗时条（`elapsed_ms`，按相对最大耗时的百分比宽度）与终态着色、失败成员 message。
  - 无报告时不渲染该区块。
- 实时进度（当前次）与「最近一次」并存：实时状态仍用顶部整体进度条，报告区展示上次结果。

### B. 运行面板：端口链接 + 日志快捷入口

**现状**：`StackRunPanel` 成员卡仅状态/depends/message，无可操作项。

**端口来源**（已确认）
- `ref_type == software`：`InstalledSoftware.port`（number，0 视为无端口）。
- `ref_type == springboot`：`SpringBootApp.port`（`Option<u16>`，jar 未配置 `server.port` 时为 null）。

**前端改动（`StackRunPanel.vue`）**
- `mc-actions` 行加两个按钮（仅当有端口/可看日志时显示）：
  - **端口**：`openUrl('http://127.0.0.1:{port}')`。Tauri v2 用 `@tauri-apps/plugin-opener`；若工程未接入则改用 `invoke('open_external')`（后端 `open` crate / `opener`）。实现时二选一（已有依赖优先）。
  - **日志**：维护一个打开状态。
    - software 成员 → 打开现有 `LogViewerDialog`（props `software` = 该 installed 实例）。
    - springboot 成员 → 打开 `springboot-manager` 的 `LogViewer`（props `appId/appName/logPath`）。
- RunPanel 顶部增加暂存弹窗 mount 位置（`v-if` 渲染对应查看器）。

### C. 日志归档无缝续接

#### C-1 软件日志（`log_viewer.rs` + `LogViewerDialog.vue`）

**数据模型（`models/software.rs` / `src/models/software.ts`）**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveLog {
    pub path: String,
    pub label: String,   // 如 "2026-08-24" / "access.log-20260824.gz"
}
// LogSource 增：
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub archives: Vec<ArchiveLog>,   // 历史归档，时间倒序（最新在前）
// LogChunk 增：
pub archive_index: usize,       // 后端实际读取的归档索引（0=主文件；审计/续接靠它）
```

**归档识别（通用规则，provider 目录扫描）**
- 新增纯函数 `collect_archives(primary: &Path) -> Vec<ArchiveLog>`（放在 `log_viewer.rs`）：
  - 扫描 primary 所在目录（`*.gz`）与 primary 相同 basename **前缀**的文件：`{stem}-YYYYMMDD.gz`、`{stem}.{n}`、`{stem}.YYYY-MM-DD`、`{type}.YYYY-MM-DD.{n}.log.gz`。
  - 过滤条件：文件名含 primary 的文件名 stem，且以 `.gz` / 数字后缀 / 日期片段结尾（日志滚动特征）。
  - 按「文件名含日期」或文件修改时间降序排序，取最旧到最新？→ 取**最新在前**（靠近主文件）。
  - 上限 `MAX_ARCHIVES = 40`（防海量）。
- `default_log_sources`（providers/mod.rs）在返回 StdoutRedirect 时补 `archives`。
- provider 覆盖的 `log_sources`（nginx/mysql/redis/mongodb/kafka/es/nacos/postgresql）在其追加的文件源（如 access.log）上也补 `archives`；**统一封装**：在 `log_viewer.rs` 提供 `attach_archives(mut LogSource) -> LogSource`，provider 只需 `let s = attach_archives(第N源)`。
- 各软件已知滚动形态（实现时核对官方约定，识别规则已通用覆盖）：
  - nginx：logrotate `access.log-YYYYMMDD.gz`（运维侧配置，OPX 不代工 rotate，仅识别）。
  - mysql：`error.log` 或日志目录滚动；识别同规则。
  - mongodb：`mongod.log` 宿主 logrotate。
  - redis：`redis.log` 宿主 logrotate。
  - 以上统一由通用规则覆盖；不逐个写死模式。

**读取扩展（`read_log` / `read_backward_filtered`）**
- `read_log` 增参数 `archive_index: usize`（默认 0）。前端每次请求显式传 `archive_index`。
- 语义：
  - `archive_index == 0`：tail/增量/历史都作用于主文件（`offset` 为主文件字节）。
  - `archive_index > 0`：读取第 `archive_index` 个归档文件（`source.archives[archive_index-1]`）。
- **无缝续接**：`read_backward_filtered` 在读当前文件回溯到**文件头（`content_start <= 0`）**且存在更旧档（`source.archives.len() > archive_index`）时——不返回空，改为**切换到 `archive_index+1` 归档的尾部**继续读 `limit` 行；返回的 `chunk.archive_index = 新索引`、`start_offset = 新归档内真实偏移`、`has_more = 新归档还有更早 OR 还有更旧档`。前端只需：`has_more` 时继续点「加载更早」（自动带上一次返回的 `archive_index` + `start_offset`），**交互零改动**。
- `.gz` 读取：`read_backward_filtered` / `read_since` 对 `.gz` 路径用 `flate2::read::MultiGzDecoder` **整体解压到内存 `Vec<u8>`**，再套现有字节 offset / 行过滤逻辑。归档按天、单日体积可控（若后续遇超大归档再引入按需解压）。
- `total_bytes` / `end_offset` 语义仅在主文件有实时意义；归档返回该归档解压后字节数。

**前端（`LogViewerDialog.vue`）**
- 维护 `archiveIndex`（初始 0），每次 `loadHistory` 传当前值；从返回 chunk 同步 `archiveIndex = chunk.archive_index`。
- 过滤/级别/关键字/实时/自动滚动逻辑不变。

#### C-2 应用日志（Spring Boot，按 level 多源）

**现状**：`read_springboot_log(path, offset)` 单文件增量；`LogViewer.vue` 单源。

**后端改动**
- 新增命令 `list_springboot_log_sources(app_id: String) -> Vec<LogSource>`：
  - 解析应用 `log_path`（相对 data_dir → 绝对，复用 `paths::resolve_data_path`）。
  - 取 `log_path` 的 `logs/` 目录（文件时取父目录）。
  - 扫描顶层 `*.log`（`debug.log` / `info.log` / `error.log` / `warn.log` 等，**存在才加入**）：每个作为 `LogSource { kind: ProviderFile, has_levels: true, label: 文件名 stem 大写 }`。
  - 每源 `archives`：扫描 `logs/<日期目录>/*.gz` 与同目录 `<type>.<日期>.<n>.log.gz` 中 basename 与 `{stem}` 匹配者（复用 C-1 归档识别子集：需支持「日期子目录」）。
  - 兜底：若没有任何 level 文件且 `log_path` 指向的文件存在，退回单源旧行为（`console.log`），保证既有应用不退化。
- `read_springboot_log` 签名扩展：`read_springboot_log(app_id: String, source_index: usize, archive_index: usize, offset: u64)`；内部**复用 C-1 的通用读取函数**（`read_backward_filtered` / `read_since` 提升为 `pub(crate)`），README 级联面最小。为兼容既有调用，改造前端同步更新即可。
- `LogChunk`（springboot 用）统一收敛到模型的 `LogChunk`（含 `archive_index`），删除 `commands/springboot.rs` 自有的 `LogChunk { lines, offset }` 或做字段映射。

**前端（`springboot-manager/components/LogViewer.vue`）**
- 顶部加「源 tab」（debug/info/error/warn，来自 `list_springboot_log_sources`）。
- 每源支持「加载更早」历史按钮 + 归档无缝续接（逻辑与软件查看器对齐）；保留关键字/实时/自动滚动。
- 布局对齐软件 `LogViewerDialog`：加入过滤行（关键字、实时、自动滚动、加载更早）。

### D. Nacos 集群扩展点预留（不实现集群）

**现状**：`providers/nacos.rs` config_field mode（Select：standalone/cluster），`start_command` 对非 standalone 直接 `Err("集群模式暂未支持")`。

**改动**
- `ConfigFieldType::Select` 增 `disabled_options: Vec<String>` + `disabled_hint_i18n: Option<String>`（serde `default`），前端 Select 渲染时对这些 option `disabled` 且 `title` 显示 hint。
- nacos `mode` 字段：
  - options 仍为 `["standalone", "cluster"]`（**保留 cluster 枚举** = 扩展点）；
  - `disabled_options = ["cluster"]`、`disabled_hint_i18n = Some("configField.nacosModeClusterHint")`；
- `start_command` 的 cluster 报错**保留**（防直接注入配置绕过 UI）。
- 添加 `// ponytail: cluster 扩展点 —— 后续按 nacos 官方多节点 RAFT 接入，禁用为前置提示，不建实现`注释。

**前端（`ConfigFormTab.vue`）**
- Select 分支：`option` 落在 `disabled_options` 时添加 `disabled` 属性与 `title`（值取 `disabled_hint_i18n` 翻译）。

### E. Node.js Runtime provider

**新增 `providers/node.rs`（参照 `jre.rs` 定位：Runtime、仅安装不启动）**
- `key = "node"`，`SoftwareCategory::Runtime`，icon 沿用现有约定（前端卡片用目录 icon 键）。
- `catalog_entry`：
  - 内置版本（Windows）：`v20.11.1`、`v22.14.0`（LTS），mirror 用官方 `https://nodejs.org/dist/`，文件名 `node-v{ver}-win-x64.zip`（`ArchiveFormat::Zip`）。
  - 非 Windows 平台不注册 zip（或按平台给 tar.xz，参照 jre.rs 的 cfg 分支）。
- `fetch_remote_versions()`：GET `https://nodejs.org/dist/index.json`，解析数组 `{ "version": "v22.14.0", "lts": true | "..." }`：
  - 保留 `lts` 非 false/非 null + 纯 `X.Y.Z` 版本 → 去 `v` 前缀；
  - 与内置 `merge_versions` 合并（复用现有合并逻辑，见 kafka/es）。
  - `parse_node_index(json) -> Vec<String>` 纯函数 + 单测。
- `start_command`：Node 为 Runtime，**不提供启动**（同 JRE 行为：不出现在软件管理可启停列表，仅仓库安装）。

**Catalog/注册**
- `providers/mod.rs::all_providers()` push `Box::new(node::NodeProvider::new())`。
- 前端 `catalogDesc.node` 文案；仓库分组归 Runtime（category 驱动已自动归位）。

## 5. 数据模型变更汇总

| 位置 | 变更 |
|---|---|
| `models/software.rs` | `ArchiveLog` 新增；`LogSource.archives`；`LogChunk.archive_index`；`ConfigFieldType::Select.disabled_options/.disabled_hint_i18n` |
| `models/stack.rs` | `StackRunReport`、`StackMemberReport` 新增；`Stack.last_run_report` |
| `models/springboot.rs` | 无字段变更（读日志改用 LogSource/LogChunk） |
| `src/models/software.ts` | 对齐 ArchiveLog/LogSource/LogChunk/Select.disabled_options |
| `src/models/stack.ts` | 对齐 StackRunReport/Stack.last_run_report |
| `src/models/springboot.ts` | 对齐（如有 LogChunk 引用） |

## 6. Tauri 命令 / API 变更

| 命令 | 变更 |
|---|---|
| `get_log_sources` | 返回含 `archives`（无需签名改） |
| `read_log`（commands/software.rs） | 增 `archive_index: usize` 参数 |
| `list_springboot_log_sources` | **新增** |
| `read_springboot_log`（commands/springboot.rs） | 改为 `(app_id, source_index, archive_index, offset)`，复用通用读取 |
| `stack` CRUD | 无新增（`last_run_report` 随 stack 全量序列化返回） |
| `open_external`（或 `plugin-opener`） | 端口链接用；无则新增极简命令 |

## 7. 前端改动汇总

| 文件 | 改动 |
|---|---|
| `software-manager/components/LogViewerDialog.vue` | 维护 `archiveIndex`，`loadHistory` 传参并同步 |
| `springboot-manager/components/LogViewer.vue` | 源 tab 多源 + 加载更早 + 归档续接 + 过滤行 |
| `stack/StackRunPanel.vue` | 最近一次启动报告区 + 成员端口/日志按钮 |
| `software-manager/components/ConfigFormTab.vue` | Select disabled_options 渲染 |
| locales `zh-CN.ts` / `en-US.ts` | 新增 A-C/E 所需文案、`catalogDesc.node`、`configField.nacosModeClusterHint` 等 |

## 8. 多语言新增要点

- A：`stackReport`、`lastRunReport`、`totalElapsed` 等。
- B：`openPort`、`openLogs`（可能复用已有 `viewLogs`）。
- C：`archives`、`loadingEarlier`（复用 `loadEarlier`）/ `noArchives`。
- D：`configField.nacosModeClusterHint`（"集群模式后续版本支持"）。
- E：`catalogDesc.node`。

## 9. 测试清单

**Rust（`cargo test --lib`）**
1. `collect_archives`：logrotate 形态（`access.log-20260824.gz`）、logback 形态（`info.2026-08-26.0.log.gz`）、排序（最新在前）、上限 40、非日志文件过滤。
2. `.gz` 读取：写临时 `.gz`，`read_backward_filtered` 尾部/历史/`read_since` 正确。
3. 跨文件续接：主文件 3 行 + 归档 2 行，历史翻页跨文件返回 `archive_index` 变化与行拼接。
4. `parse_node_index`：LTS 过滤、`v` 前缀去除、排序。
5. 栈启动报告聚合：给定成员 (t0,t1) → `elapsed_ms` 正确；运行/失败态记录。
6. 归档上限、zip-slip 无关（既有）回归。

**前端（`npx vue-tsc --noEmit`）**
- 模型类型对齐；`LogViewerDialog` / springboot `LogViewer` / `StackRunPanel` / `ConfigFormTab` 类型通过。

## 10. 验证流程

1. `cargo test --lib`（相关模块）+ `npx vue-tsc --noEmit`。
2. `npm run tauri:dev` GUI 走查：
   - 软件日志：选 nginx/MySQL 实例日志，连续点「加载更早」跨过归档文件无断裂；`.gz` 内容可读。
   - 应用日志：有 logback level 拆分的应用显示 4 个源 tab，各源可翻历史归档。
   - 服务组：启动后 RunPanel 显示最近一次报告（成员耗时/结果）；成员端口可点击打开；日志按钮弹出对应查看器。
   - Node：仓库可安装 Node（内置/动态版本）；不出现在可管理列表。
   - Nacos：配置表单 cluster 选项置灰 + 提示。
3. 合并 dev，推送（按用户要求时机执行）。

## 11. 自审记录

- **占位符扫描**：无 TBD/TODO；Node 动态版本 URL 为已知官方接口。
- **一致性**：C-2 复用 C-1 通用读取，两模块语义一致；A 报告不新增命令、随 stack 全量返回。
- **范围**：聚焦 5 项，实测跨文件续接为唯一复杂点，已有单测覆盖。
- **歧义**：
  - 「无缝续接」定义为「历史翻页到文件头自动切更旧归档尾部」，交互零改动。
  - `.gz` 仅解压到内存方式读取（归档按天，量可控）；超大归档后续优化点不本期做。
  - Node 仅 Runtime 安装，不管理启动（同 JRE 定位）。