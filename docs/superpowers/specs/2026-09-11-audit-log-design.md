# 操作记录（审计日志）可视化 —— 一期

> 状态：已确认设计
> 日期：2026-09-11
> 范围：一期（存储 + 查看页，不含操作结果）；二期见文末。

## 背景与现状

- `oplog!` 宏（`src-tauri/src/services/software_manager/audit_log.rs`）已被 **28 处**命令调用，覆盖安装/卸载/启停/重启/配置保存/备份恢复/回滚/SpringBoot CRUD/网站增删改/退出/设置等。
- 宏当前实现是 `tracing::info!(action, target, detail, "")`，落到 `logs/software-manager.log.YYYY-MM-DD`（按日滚动、`DEFAULT_RETAIN_DAYS = 7` 天清理），与普通运行日志混在同一文件。
- **前端零引用**：这些操作记录目前在 UI 中完全不可见（既有日志查看器读的是各实例日志，不含应用侧审计）。
- 调用点均在命令**开头**，因此现有语义是「记录操作发起」，无成功/失败结果。

## 一期已确认决策

1. **入口**：侧边栏新增独立页面「操作记录」（路由 `/audit`）。
2. **展示范围**：仅结构化操作审计条目，不展示普通应用运行日志。
3. **能力**：基础过滤 + 关键字搜索、导出、概览统计。
4. **分两期**：一期只做存储 + 查询 + 页面，**不含操作结果**；二期再引入 `result`/`error`。
5. 存储用**专用 JSONL**（不解析 tracing 文本、不引入 SQLite）。

## 设计

### 1. 审计存储模块 `src-tauri/src/services/software_manager/audit.rs`（新）

- 条目结构：

  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct AuditEntry {
      pub ts: String,      // RFC3339 本地时间
      pub action: String,  // install / start / stop / ...
      pub target: String,  // 软件名、站点名等
      #[serde(default)]
      pub detail: String,  // 版本、id 等补充
  }
  ```

  一期**不设** `result`/`error` 字段（避免用假值污染语义）。二期新增时用 `#[serde(default)]` 保持 JSONL 向后兼容。

- 落盘：`logs/audit-YYYY-MM-DD.jsonl`，每行一个 JSON 对象，追加写。
- `record(action: &str, target: &str, detail: &str)`：**同步** append（打开→写一行→关闭）。相比 tracing 非阻塞队列，可保证 `quit_app`/`exit_app` 这类退出前调用不丢尾部记录。
- 查询/统计返回结构：

  ```rust
  pub struct AuditQuery {
      pub entries: Vec<AuditEntry>, // 按 ts 倒序
      pub truncated: bool,          // 是否因 limit 被截断
  }
  pub struct ActionCount { pub action: String, pub count: u64 }
  pub struct AuditStats {
      pub today: u64,               // 今日操作数
      pub total: u64,               // 近 days 天总数
      pub by_action: Vec<ActionCount>,
      pub last_ts: Option<String>,  // 最近一条时间
  }
  ```

- `query(days, action, keyword, limit) -> AuditQuery`：
  - 读最近 `days` 天的 `logs/audit-YYYY-MM-DD.jsonl`（审计文件始终带日期后缀，不存在无后缀的「活动文件」）；
  - `action` 精确匹配；`keyword` 对 `target`/`detail`/`action` 做不区分大小写子串匹配；
  - 按 `ts` 倒序，最多返回 `limit` 条，被截断时 `truncated = true`（避免超大 payload）。
- `stats(days: u64) -> AuditStats`。
- `export_csv(days, action?, keyword?, dest_path) -> Result<(), String>`：按当前过滤条件写 CSV（列：时间,操作,目标,详情），字段做 CSV 转义（含逗号/引号/换行时用双引号包裹并转义内部引号）。
- 时间解析容错：无法解析 `ts` 的行跳过，不 panic。

### 2. `oplog!` 宏改造（`audit_log.rs`）

- 宏体由 `tracing::info!(...)` 改为调用 `audit::record(action, target, detail)`。
- **只改宏这一处，28 个调用点零改动**，语义保持「操作发起时记录」。
- 保持宏签名不变（两参/三参两种形态）。

### 3. 保留清理（`audit_log.rs::cleanup_old_logs`）

- 在现有清理 `software-manager.log.<date>` 的基础上，同时清理超期 `audit-<date>.jsonl`；保留天数沿用 `DEFAULT_RETAIN_DAYS = 7`。
- 清理函数已有调用点（`lib.rs` setup），无需新增调度。

### 4. 后端命令（`src-tauri/src/commands/` 新 `audit.rs`）

- `list_audit_entries(days: u64, action: Option<String>, keyword: Option<String>, limit: Option<usize>) -> AuditQuery`
- `audit_stats(days: u64) -> AuditStats`
- `export_audit_entries(days: u64, action: Option<String>, keyword: Option<String>, dest_path: String) -> Result<(), String>`
- 在 `lib.rs` 的 `invoke_handler` 注册以上三个命令（`commands::audit::*`），并新增 `commands::audit` 模块。

### 5. 前端

- 新增页面 `src/modules/audit/pages/AuditLogPage.vue`：
  - **概览卡**：今日操作数、近 7 天总数、Top 操作类型（前 3）、最近操作时间。
  - **过滤栏**：时间范围（今天 / 近 3 天 / 近 7 天）、操作类型下拉（选项取自 `audit_stats.by_action`，即近 7 天实际出现过的操作）、关键字输入。
  - **表格**：时间、操作、目标、详情；空状态与截断提示（`truncated` 时提示「仅显示最近 N 条」）。
  - **导出按钮**：`save()`（`@tauri-apps/plugin-dialog`）选路径 → 调 `export_audit_entries`；复用 `LogViewerDialog` 既有导出交互模式。
- 路由：`src/router/index.ts` 新增 `/audit`。
- 侧边栏：`src/layouts/Sidebar.vue` 新增入口（图标 `mdi:history`）。
- i18n：`zh-CN.ts` / `en-US.ts` 新增页面标题、过滤标签、表头、空状态、导出提示等键。

### 6. 测试

- `audit.rs` 单测（`cargo test --lib`）：
  - `record` 后文件存在且追加一行可解析；
  - `query` 按天过滤、按 action 过滤、按关键字过滤（大小写不敏感）、`limit` 截断标记；
  - `stats` 聚合正确（today/total/by_action/last_ts）；
  - CSV 转义：含逗号、双引号、换行的字段。
- 前端 `npx vue-tsc --noEmit`。

## 二期（本次不做，明确延后）

- 采集操作**结果**：同步命令在返回处记录 `result`，后台任务（install/upgrade/uninstall/rollback）在任务完成处记录；
- `AuditEntry` 增加 `result: "ok" | "fail"` 与 `error: Option<String>`（`#[serde(default)]` 兼容一期数据）；
- 页面增加「结果」列 + 结果过滤；失败红标。

## 边界（不做）

- 无用户/账户维度（本地单用户工具）。
- 无实时推送：进入页面拉取 + 手动刷新。
- 不做审计条目的编辑/删除（只读，随 7 天保留自动过期）。
- 不把普通应用运行日志纳入本页。

## 改动文件清单

- `src-tauri/src/services/software_manager/audit.rs`（新）
- `src-tauri/src/services/software_manager/audit_log.rs`（宏改落点 + 清理扩展）
- `src-tauri/src/services/software_manager/mod.rs`（注册 `pub mod audit;`）
- `src-tauri/src/commands/audit.rs`（新，三个命令）
- `src-tauri/src/commands/mod.rs`（注册模块）
- `src-tauri/src/lib.rs`（注册命令）
- `src/modules/audit/pages/AuditLogPage.vue`（新）
- `src/router/index.ts`、`src/layouts/Sidebar.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
