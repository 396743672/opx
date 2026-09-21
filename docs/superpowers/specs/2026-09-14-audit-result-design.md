# 操作记录（审计日志）—— 二期：操作结果

> 状态：已确认设计
> 日期：2026-09-14
> 范围：二期（给用户发起的操作补采结果）；承接一期 `2026-09-11-audit-log-design.md`

## 背景

一期落地了「存储 + 查看页」，`oplog!` 调用点均在命令**开头**，语义只有「操作发起」，看不到成败。本期给**用户发起的命令**补采结果，让失败（含原因，如「端口 8080 已被占用」）在页面上可见。

## 已确认决策

1. **采集范围 = 用户发起的命令**（app / config / software / springboot / website 侧）。
2. 系统内部动作（watchdog 自动重启、recorder 告警、acme 定时续期）**保持现状**，结果列显示「—」。
3. **同步命令**：在返回处记一条，直接带结果。
4. **后台任务**（install / install_custom / upgrade）：发起记一条（标「进行中」）+ 完成再记一条（带结果）。
5. `quit_app` / `exit_app` 保持「未采集」（进程随即退出，不存在完成点）。
6. 后台任务的完成记录**复用既有终态信号 `emit_event(phase: failed|completed)` 收口**，不逐个改失败分支。

## 设计

### 1. 数据模型（`services/software_manager/audit.rs`）

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub ts: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: String,
    /// "" = 未采集 | "running" = 进行中 | "ok" | "fail"
    #[serde(default)]
    pub result: String,
    /// result == "fail" 时非空
    #[serde(default)]
    pub error: String,
}
```

- 取值常量：`RESULT_OK = "ok"`、`RESULT_FAIL = "fail"`、`RESULT_RUNNING = "running"`；空串表示未采集。
- `record(action, target, detail)` 签名与语义不变，内部委托 `record_full(action, target, detail, "", "")`
  → 一期全部老调用点零改动。
- 新增 `record_full(action, target, detail, result, error)`。
- 新增纯函数，供宏与单测复用（泛型覆盖 `Result<_, String>` 与 `anyhow::Result`）：

  ```rust
  /// Result → (result 取值, 错误文本)。Ok 时错误文本为空串。
  pub fn classify<T, E: std::fmt::Display>(r: &Result<T, E>) -> (&'static str, String)
  ```

### 2. 宏（`services/software_manager/audit_log.rs`）

- `oplog!`（两参/三参）语义不变，仍写「未采集」。
- 新增：

  ```rust
  // 后台任务发起：记一条 result="running"
  oplog_begin!(action, target)            / oplog_begin!(action, target, detail)

  // 同步操作完成：res 为 Result<_, String> 或 anyhow::Result
  oplog_result!(action, target, detail, res)
  ```

- 新增函数体包装宏，用于含 `?` / 早 `return` 的命令体（宏内建闭包，`?` 归属闭包）：

  ```rust
  audited!("action", target, detail, { ...body... })        // 同步
  audited_async!("action", target, detail, { ...body... })  // async（体内 async 块）
  ```

  实现要点：先求值 action/target/detail 为 owned `String`（避免与体内 move 冲突），再执行体，最后 `oplog_result!` 记录并返回原 `Result`。

### 3. 同步命令改造（用户命令共 ~26 处，其中 2 处保持未采集）

- **尾委托型**（`oplog!` 后直接是 `service(...).await` / `service(...)`）：

  ```rust
  let r = service(...).await;
  oplog_result!("springboot_start", &target, "", r);
  r
  ```

- **含 `?` 的函数体**：整体包进 `audited!` / `audited_async!`。
- **保持「—」**：`quit_app`、`exit_app`。
- **`acme_issue` 合并**：`website.rs` 现有 `acme_issue` / `acme_issue_failed` 两个 action 统一为 `acme_issue`，失败原因进 `error`（页面靠「结果」列区分，不再靠 action 名区分）。
- 覆盖清单（实现计划中逐条落地）：
  - `commands/app.rs`：quit_app、exit_app（保持未采集）
  - `commands/config.rs`：test_dns_token、save_settings
  - `commands/software.rs`：start、stop、restart、config_form、config_source、save_start_command、save_startup、restore_config_backup、rollback、uninstall
  - `commands/springboot.rs`：create、update、delete、start、stop、restart、replace_jar、replace_restart、save_groups、set_global_env
  - `commands/website.rs`：save、delete、toggle、acme_issue

### 4. 后台任务（install / install_custom / upgrade）

- **发起**：命令处把 `oplog!("install", key, version)` 改为 `oplog_begin!(...)` →「进行中」。
- **完成（install / install_custom）**：installer 内的终态信号只有 `emit_event(phase: "failed" | "completed")` 一处出口，
  但该函数是自由函数、拿不到命令侧的 action/target。因此：
  - 命令发起时把 `(action, target, detail)` 按 `install_id` 登记进 installer 内的 `LazyLock<Mutex<HashMap<String, (String,String,String)>>>` 小表；
  - `emit_event` 遇到 `phase` 为 `failed` / `completed` 时，按 payload 的 `install_id` **取出（take，一次性）**上下文并写审计：completed → `ok`；failed → `fail` + payload 的 `error`。
  - 覆盖 install_software / install_custom 全部早返回失败分支与 builtin 分支（约 20 个失败点），无需逐个改。
  - take 语义保证同一 install_id 只落一条完成记录（builtin 分支与外层都发终态事件时不会重复）。
- **完成（upgrade）**：`do_upgrade` 已返回 `Result`，在 spawn 包装里 `match` 结果：`Err` 分支保留既有 `install-progress` failed 事件，并 `oplog_result!` 记 fail + `{:#}` 展开的错误链；`Ok` 记 ok。
- 若中途崩溃 / 进程退出 → 该条停留在「进行中」，如实反映「已发起但未见完成」。

### 5. 查询 / 统计 / 导出（`audit.rs` + `commands/audit.rs`）

- `query(days, action, keyword, result, limit, offset)`：`result` 精确匹配（`Some("")` 表示「未采集」）。
- `matches_filters` 增 `result` 比较。
- `AuditStats` 增 `failed: u64`（近 days 天 `result == "fail"` 条数）。
- `export_csv` 增 `result` 参数；CSV 列变为：时间, 操作, 目标, 详情, 结果, 错误。
- `list_audit_entries` / `export_audit_entries` 增 `result: Option<String>`。

### 6. 前端

- `src/models/audit.ts`：`AuditEntry` 增 `result: string`、`error: string`；`AuditStats` 增 `failed: number`。
- `AuditLogPage.vue`：
  - 表格增「结果」列：`ok` 绿 / `fail` 红 / `running` 琥珀 / 空「—」灰；
  - 失败行整行红调，错误文本截断显示（`title` 悬停看全文），失败原因同时作为该行的错误提示；
  - 过滤栏增「结果」下拉：全部 / 成功 / 失败 / 进行中 / 未采集；
  - 概览「近 N 天总数」卡副行显示失败数；
  - 导出时带上 `result` 过滤条件。
- i18n：`zh-CN.ts` / `en-US.ts` 新增「结果」列头、四种结果文案、结果过滤项、失败数副行等键。

### 7. 兼容性

- 一期 JSONL 行没有 `result` / `error` 字段 → `#[serde(default)]` 读为空串 → 页面显示「—」。**无需数据迁移**。

### 8. 测试

- `audit.rs` 单测（`cargo test --lib`）：
  - `record_full` 落盘后可回读 result / error；
  - `classify`：Ok → `("ok", "")`，Err → `("fail", 原因)`；
  - `query` 按 result 过滤（含「未采集」空串）；
  - `stats` 的 `failed` 计数正确；
  - `export_csv` 含「结果」「错误」列，且错误文本中的逗号 / 双引号 / 换行被正确转义；
  - **一期旧行兼容**：无 result/error 的 JSON 行解析成功且两字段为空串。
- 前端：`npx vue-tsc --noEmit`。
- 实机验证：制造一次失败（如启动端口被占用的软件）→ 页面出现红色「失败」+ 原因；一次后台安装失败 → 有「进行中」+「失败」两条。

## 边界（不做）

- 不采集系统内部动作的结果（保持一期行为）。
- 不改一期已有条目的存储格式，不迁移历史数据。
- 不做失败重试、告警联动（失败仅记录，不推送）。
- 不做按结果维度的趋势图表。

## 改动文件清单

- `src-tauri/src/services/software_manager/audit.rs`（模型 + record_full + classify + 查询/统计/CSV 增 result）
- `src-tauri/src/services/software_manager/audit_log.rs`（oplog_begin! / oplog_result! / audited! / audited_async!）
- `src-tauri/src/services/software_manager/installer.rs`（install_id 上下文表 + emit_event 收口）
- `src-tauri/src/commands/audit.rs`（list / export 增 result 参数）
- `src-tauri/src/commands/software.rs`、`springboot.rs`、`website.rs`、`config.rs`（各命令改造）
- `src/models/audit.ts`
- `src/modules/audit/pages/AuditLogPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
