# 操作记录二期（操作结果）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 给用户发起的操作补采「成功 / 失败（含原因）/ 进行中」，并在「操作记录」页可视化。

**Architecture:** 审计条目 `AuditEntry` 增 `result` / `error` 两字段（`#[serde(default)]`，一期数据零迁移）；新增记录宏供各命令在返回处写结果；后台任务（install / install_custom）复用既有终态信号 `emit_event` 收口；页面增「结果」列与结果过滤。

**Tech Stack:** Rust（Tauri v2 命令、`macro_rules!` 宏、serde）、Vue 3 + TypeScript + vue-i18n。

## Global Constraints

- 所有改动在从 `dev` 新建的分支 `feat/audit-result` 上完成；禁止直接在 `master` / `dev` 上改。
- 结果取值固定为四种：`""`（未采集）、`"running"`（进行中）、`"ok"`、`"fail"`。后端常量：`RESULT_OK` / `RESULT_FAIL` / `RESULT_RUNNING`。
- 后端测试：`cd src-tauri && cargo test --lib`（当前基线 151 passed，改动后只增不减）。
- 前端类型检查：仓库根目录 `npx vue-tsc --noEmit`。
- i18n 键必须同时加进 `src/locales/zh-CN.ts` 与 `src/locales/en-US.ts`。
- 提交信息用中文 conventional commits，不加 `Co-Authored-By`。
- 系统内部动作（watchdog / recorder / acme 续期调度）与 `quit_app` / `exit_app` 保持「未采集」（`result = ""`），不改。
- 不改一期 JSONL 存储格式，不迁移历史数据。

---

### Task 1: 审计存储层支持结果字段

**Files:**
- Modify: `src-tauri/src/services/software_manager/audit.rs`
- Test: `src-tauri/src/services/software_manager/audit.rs`（`#[cfg(test)] mod tests` 内）

**Interfaces:**
- Consumes: 无（本任务是最底层）
- Produces:
  - `pub const RESULT_OK: &str = "ok";` `pub const RESULT_FAIL: &str = "fail";` `pub const RESULT_RUNNING: &str = "running";`
  - `pub struct AuditEntry { ts, action, target, detail, result, error }`（`result` / `error` 均为 `String`，`#[serde(default)]`）
  - `pub fn record(action: &str, target: &str, detail: &str)`（签名不变，内部委托 `record_full(..., "", "")`）
  - `pub fn record_full(action: impl AsRef<str>, target: impl AsRef<str>, detail: impl AsRef<str>, result: &str, error: impl AsRef<str>)`
  - `pub fn classify<T, E: std::fmt::Display>(r: &Result<T, E>) -> (&'static str, String)`
  - `pub fn query(days: u64, action: Option<&str>, keyword: Option<&str>, result: Option<&str>, limit: usize, offset: usize) -> AuditQuery`
  - `pub fn export_csv(days: u64, action: Option<&str>, keyword: Option<&str>, result: Option<&str>, dest_path: &str) -> Result<(), String>`
  - `pub struct AuditStats { today, total, failed, by_action, last_ts }`

- [ ] **Step 1: 写失败测试**

在 `audit.rs` 的 `mod tests` 末尾追加（`uniq()` 辅助函数已存在）：

```rust
    #[test]
    fn record_full_persists_result_and_error() {
        let u = uniq();
        record_full(
            "__qa_full",
            &format!("{u}_target"),
            "detail",
            RESULT_FAIL,
            &format!("{u}_boom"),
        );
        let q = query(1, Some("__qa_full"), Some(&u), None, 100, 0);
        assert_eq!(q.entries.len(), 1);
        assert_eq!(q.entries[0].result, RESULT_FAIL);
        assert_eq!(q.entries[0].error, format!("{u}_boom"));
    }

    #[test]
    fn legacy_line_without_result_fields_parses() {
        let line = r#"{"ts":"2026-01-01T00:00:00+08:00","action":"a","target":"t","detail":"d"}"#;
        let e: AuditEntry = serde_json::from_str(line).expect("一期旧行可解析");
        assert_eq!(e.result, "");
        assert_eq!(e.error, "");
    }

    #[test]
    fn classify_maps_ok_and_fail() {
        let ok: Result<(), String> = Ok(());
        assert_eq!(classify(&ok), (RESULT_OK, String::new()));
        let bad: Result<(), String> = Err("端口 8080 已被占用".to_string());
        assert_eq!(
            classify(&bad),
            (RESULT_FAIL, "端口 8080 已被占用".to_string())
        );
    }

    #[test]
    fn query_filters_by_result() {
        let u = uniq();
        record_full("__qa_res", &format!("{u}_t"), "", RESULT_FAIL, &format!("{u}_e"));
        record_full("__qa_res", &format!("{u}_t"), "", RESULT_OK, "");
        // 按 fail 过滤只命中一条
        assert_eq!(query(1, Some("__qa_res"), Some(&u), Some(RESULT_FAIL), 100, 0).entries.len(), 1);
        // 按「未采集」空串过滤：两条都不是空串，命中 0
        assert_eq!(query(1, Some("__qa_res"), Some(&u), Some(""), 100, 0).entries.len(), 0);
        // 不过滤：两条都命中
        assert_eq!(query(1, Some("__qa_res"), Some(&u), None, 100, 0).entries.len(), 2);
    }

    #[test]
    fn stats_counts_failures() {
        let u = uniq();
        record_full("__qa_stat", &format!("{u}_t"), "", RESULT_FAIL, "e1");
        record_full("__qa_stat", &format!("{u}_t"), "", RESULT_OK, "");
        let s = stats(1);
        assert!(s.failed >= 1, "failed 至少计入本次写入的 1 条");
        assert_eq!(
            s.by_action.iter().find(|a| a.action == "__qa_stat").map(|a| a.count),
            Some(2)
        );
    }

    #[test]
    fn export_csv_appends_result_and_escapes_error() {
        let u = uniq();
        record_full(
            "__qa_csv",
            &format!("{u}_t"),
            "d",
            RESULT_FAIL,
            "错误，含逗号 \" 引号 和换行\n",
        );
        let dest = std::env::temp_dir().join(format!("{u}_audit.csv"));
        let dest_str = dest.to_string_lossy().to_string();
        export_csv(1, Some("__qa_csv"), Some(&u), Some(RESULT_FAIL), &dest_str).unwrap();
        let out = std::fs::read_to_string(&dest).unwrap();
        assert!(out.starts_with('\u{feff}'), "保留 BOM 供 Excel 识别中文");
        assert!(out.contains("时间,操作,目标,详情,结果,错误"), "表头含结果/错误两列");
        assert!(out.contains("__qa_csv"), "命中本次写入的条目");
        assert!(out.contains("\"错误，含逗号 \"\" 引号 和换行"), "错误文本被 CSV 转义");
        let _ = std::fs::remove_file(&dest);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --lib audit:: 2>&1 | tail -30`
Expected: 编译失败（`record_full` / `classify` / `RESULT_FAIL` / `failed` 字段 / `query` 第 4 参数 不存在）。

- [ ] **Step 3: 改 `AuditEntry` + 常量 + `record` / `record_full` / `classify`**

`audit.rs` 顶部（`use crate::utils::paths;` 之后）加常量：

```rust
/// result 字段取值：空串 = 未采集（系统内部动作 / 一期历史条目）
pub const RESULT_OK: &str = "ok";
pub const RESULT_FAIL: &str = "fail";
pub const RESULT_RUNNING: &str = "running";
```

`AuditEntry` 改为：

```rust
/// 单条操作记录。result / error 为二期新增，须带 #[serde(default)] 以兼容一期 JSONL。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub ts: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: String,
    /// "" = 未采集 | "running" | "ok" | "fail"
    #[serde(default)]
    pub result: String,
    /// result == "fail" 时非空
    #[serde(default)]
    pub error: String,
}
```

`record` 改为委托，并新增 `record_full` 与 `classify`：

```rust
/// 追加一条「未采集结果」的操作记录（一期语义，签名保持不变）。
pub fn record(action: &str, target: &str, detail: &str) {
    record_full(action, target, detail, "", "");
}

/// Result → (result 取值, 错误文本)；Ok 时错误文本为空串。
pub fn classify<T, E: std::fmt::Display>(r: &Result<T, E>) -> (&'static str, String) {
    match r {
        Ok(_) => (RESULT_OK, String::new()),
        Err(e) => (RESULT_FAIL, e.to_string()),
    }
}

/// 追加一条完整操作记录（同步写盘：保证 quit/exit 等退出前调用不丢尾部）。
pub fn record_full(
    action: impl AsRef<str>,
    target: impl AsRef<str>,
    detail: impl AsRef<str>,
    result: &str,
    error: impl AsRef<str>,
) {
    // 串行化同进程内的追加：O_APPEND 不保证进程内多线程并发写的原子性，
    // 不加锁时并发调用会交错截断 JSONL 行。
    static WRITE_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let entry = AuditEntry {
        ts: chrono::Local::now().to_rfc3339(),
        action: action.as_ref().to_string(),
        target: target.as_ref().to_string(),
        detail: detail.as_ref().to_string(),
        result: result.to_string(),
        error: error.as_ref().to_string(),
    };
    let path = file_for(chrono::Local::now().date_naive());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}
```

（即把原 `record` 函数体整体搬进 `record_full`，只把三个入参改成 `as_ref()` 并补两个新字段。）

- [ ] **Step 4: 给 `query` / `matches_filters` / `stats` / `export_csv` 加 result 支持**

`matches_filters` 签名与体首加 result 精确匹配：

```rust
fn matches_filters(
    e: &AuditEntry,
    action: Option<&str>,
    keyword: Option<&str>,
    result: Option<&str>,
) -> bool {
    if let Some(a) = action {
        if !a.is_empty() && e.action != a {
            return false;
        }
    }
    // Some("") 表示筛选「未采集」；None 表示不筛选
    if let Some(r) = result {
        if e.result != r {
            return false;
        }
    }
    if let Some(k) = keyword {
        let k = k.trim().to_lowercase();
        if !k.is_empty() {
            let haystack = format!("{} {} {} {}", e.action, e.target, e.detail, e.error).to_lowercase();
            if !haystack.contains(&k) {
                return false;
            }
        }
    }
    true
}
```

`query` 改为：

```rust
pub fn query(
    days: u64,
    action: Option<&str>,
    keyword: Option<&str>,
    result: Option<&str>,
    limit: usize,
    offset: usize,
) -> AuditQuery {
    let mut entries: Vec<AuditEntry> = read_days(days)
        .into_iter()
        .filter(|e| matches_filters(e, action, keyword, result))
        .collect();
    entries.sort_by(|a, b| b.ts.cmp(&a.ts));
    let total = entries.len();
    let truncated = offset.saturating_add(limit) < total;
    let entries = entries.into_iter().skip(offset).take(limit).collect();
    AuditQuery { entries, total: total as u64, truncated }
}
```

`AuditStats` 加字段并统计：

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AuditStats {
    pub today: u64,
    pub total: u64,
    /// 近 days 天 result == "fail" 的条数
    pub failed: u64,
    pub by_action: Vec<ActionCount>,
    pub last_ts: Option<String>,
}
```

`stats` 中在 `let last_ts = ...` 之前插入：

```rust
    let failed = all.iter().filter(|e| e.result == RESULT_FAIL).count() as u64;
```

并把返回值改为 `AuditStats { today, total: all.len() as u64, failed, by_action, last_ts }`。

`export_csv` 改为：

```rust
pub fn export_csv(
    days: u64,
    action: Option<&str>,
    keyword: Option<&str>,
    result: Option<&str>,
    dest_path: &str,
) -> Result<(), String> {
    let q = query(days, action, keyword, result, usize::MAX, 0);
    let mut out = String::from("\u{feff}时间,操作,目标,详情,结果,错误\n");
    for e in q.entries {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_field(&fmt_ts(&e.ts)),
            csv_field(&e.action),
            csv_field(&e.target),
            csv_field(&e.detail),
            csv_field(&e.result),
            csv_field(&e.error)
        ));
    }
    std::fs::write(dest_path, out).map_err(|e| e.to_string())
}
```

- [ ] **Step 5: 修既有测试调用（多出 `result` 参数）**

`mod tests` 中以下位置需在 `keyword` 参数之后插入 `None`：

- `query_filters_by_action_and_keyword`：`query(1, Some("__qa_q_action"), Some(&u), 100, 0)`
- `query_is_case_insensitive_on_keyword`：两处 `query(1, Some("__qa_q_case"), Some(...), 100, 0)`
- `query_marks_truncated_when_over_limit`：`query(1, Some("__qa_q_trunc"), Some(&u), 2, 0)`
- `query_pages_with_total_and_next_flag`：三处（`2, 0` / `2, 4` / `2, 99`）
- `export_csv_writes_header_and_rows`：`export_csv(1, Some("__qa_csv"), Some(&u), None, dest.to_str().unwrap())`

例如：

```rust
let q = query(1, Some("__qa_q_action"), Some(&u), None, 100, 0);
```

Run: `cd src-tauri && cargo test --lib audit:: 2>&1 | tail -20`
Expected: 全部 PASS（含 6 个新测试）。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/services/software_manager/audit.rs
git commit -m "feat(audit): 存储层支持操作结果（result/error + 过滤/统计/CSV）"
```

---

### Task 2: 记录宏（`audit_log.rs`）

**Files:**
- Modify: `src-tauri/src/services/software_manager/audit_log.rs`

**Interfaces:**
- Consumes: Task 1 的 `audit::record_full` / `audit::classify` / `RESULT_*` 常量
- Produces（均 `#[macro_export]`，调用方 `use crate::{...}` 引入）：
  - `oplog!(action, target)` / `oplog!(action, target, detail)` — 不变，写「未采集」
  - `oplog_begin!(action, target)` / `oplog_begin!(action, target, detail)` — 写 `result = "running"`
  - `oplog_result!(action, target, detail, res)` — `res` 为 `Result<T, E: Display>`，成功写 `ok`，失败写 `fail` + 错误文本
  - `oplog_fail!(action, target, detail, err)` — 直接写 `fail` + `err`
  - `audited!(action, target, detail, { body })` — 同步函数体包装
  - `audited_async!(action, target, detail, { body })` — async 函数体包装

- [ ] **Step 1: 在 `oplog!` 宏之后追加五个宏**

```rust
/// 后台任务发起：记一条「进行中」（result = running）。
#[macro_export]
macro_rules! oplog_begin {
    ($action:expr, $target:expr) => {
        $crate::oplog_begin!($action, $target, "");
    };
    ($action:expr, $target:expr, $detail:expr) => {
        $crate::services::software_manager::audit::record_full(
            $action,
            $target,
            $detail,
            $crate::services::software_manager::audit::RESULT_RUNNING,
            "",
        );
    };
}

/// 同步操作完成：按 Result 记 ok / fail + 错误原因。
#[macro_export]
macro_rules! oplog_result {
    ($action:expr, $target:expr, $detail:expr, $res:expr) => {{
        let (__result, __err) = $crate::services::software_manager::audit::classify(&$res);
        $crate::services::software_manager::audit::record_full(
            $action, $target, $detail, __result, __err,
        );
    }};
}

/// 已知失败原因时直接记 fail（如命令前的同步校验被拒）。
#[macro_export]
macro_rules! oplog_fail {
    ($action:expr, $target:expr, $detail:expr, $err:expr) => {
        $crate::services::software_manager::audit::record_full(
            $action,
            $target,
            $detail,
            $crate::services::software_manager::audit::RESULT_FAIL,
            $err,
        );
    };
}

/// 用闭包包住同步命令体：`?` 与早 `return` 都归属闭包，退出时统一记结果。
/// 用法：`audited!("action", target, "", { ...body... })`（须作为函数尾表达式）。
#[macro_export]
macro_rules! audited {
    ($action:expr, $target:expr, $detail:expr, $body:block) => {{
        // 先求值为 owned String：避免 action/target 的借用与体内的 move 冲突
        let __act = ($action).to_string();
        let __tgt = ($target).to_string();
        let __det = ($detail).to_string();
        let __r = (|| $body)();
        $crate::oplog_result!(__act, __tgt, __det, __r);
        __r
    }};
}

/// 同 `audited!`，用于 async 命令体（体内 `?` / `return` 归属 async 块）。
#[macro_export]
macro_rules! audited_async {
    ($action:expr, $target:expr, $detail:expr, $body:block) => {{
        let __act = ($action).to_string();
        let __tgt = ($target).to_string();
        let __det = ($detail).to_string();
        let __r = (async $body).await;
        $crate::oplog_result!(__act, __tgt, __det, __r);
        __r
    }};
}
```

- [ ] **Step 2: 确认编译（此时无调用点）**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 编译通过（宏未使用不报错）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/services/software_manager/audit_log.rs
git commit -m "feat(audit): 新增结果记录宏（begin/result/fail + 函数体包装）"
```

---

### Task 3: 审计命令透传 result 参数

**Files:**
- Modify: `src-tauri/src/commands/audit.rs`

**Interfaces:**
- Consumes: Task 1 的 `audit::query` / `audit::export_csv` 新签名
- Produces: `list_audit_entries(days, action, keyword, result, limit, offset)`、`export_audit_entries(days, action, keyword, result, dest_path)`

- [ ] **Step 1: 改两个命令**

```rust
/// 查询操作记录（分页：默认每页 50 条；total 为过滤后总数，truncated 表示还有下一页）
#[tauri::command]
pub fn list_audit_entries(
    days: u64,
    action: Option<String>,
    keyword: Option<String>,
    result: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> audit::AuditQuery {
    let days = days.clamp(1, 31);
    audit::query(
        days,
        action.as_deref(),
        keyword.as_deref(),
        result.as_deref(),
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
}

/// 按过滤条件导出 CSV 到 dest_path
#[tauri::command]
pub fn export_audit_entries(
    days: u64,
    action: Option<String>,
    keyword: Option<String>,
    result: Option<String>,
    dest_path: String,
) -> Result<(), String> {
    let days = days.clamp(1, 31);
    audit::export_csv(
        days,
        action.as_deref(),
        keyword.as_deref(),
        result.as_deref(),
        &dest_path,
    )
}
```

- [ ] **Step 2: 编译验证**

Run: `cd src-tauri && cargo build 2>&1 | tail -20`
Expected: 通过（前端旧调用缺 `result` 参数不报错：Tauri `Option<T>` 参数缺省即 `None`）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/commands/audit.rs
git commit -m "feat(audit): 查询/导出命令支持结果过滤"
```

---

### Task 4: springboot 命令补采结果（10 处）

**Files:**
- Modify: `src-tauri/src/commands/springboot.rs`

**Interfaces:**
- Consumes: Task 2 的 `oplog_result!` / `audited_async!`
- Produces: 无（终端命令）

**目标/详情取值（沿用一期 `oplog!` 的拆分，不要合并）：**

| 函数 | action | target | detail |
|---|---|---|---|
| `create_springboot_app` | `springboot_create` | `params.name` | `""` |
| `update_springboot_app` | `springboot_update` | `format!("{} ({})", name, id)` | `""` |
| `delete_springboot_app` | `springboot_delete` | 同上 | `""` |
| `start_springboot_app` | `springboot_start` | 同上 | `""` |
| `stop_springboot_app` | `springboot_stop` | 同上 | `""` |
| `restart_springboot_app` | `springboot_restart` | 同上 | `""` |
| `replace_springboot_jar` | `springboot_replace_jar` | `format!("{} ({})", app.name, id)` | `""` |
| `replace_springboot_jar_and_restart` | `springboot_replace_restart` | 同上 | `""` |
| `save_springboot_groups` | `springboot_save_groups` | `format!("{} groups", groups.len())` | `""` |
| `set_springboot_global_env_vars` | `springboot_set_global_env` | `format!("{} vars", env_vars.len())` | `""` |

- [ ] **Step 1: 改 import**

把 `use crate::oplog;` 改为：

```rust
use crate::{audited_async, oplog_result};
```

- [ ] **Step 2: 尾委托型（start / stop / restart）改写**

`start_springboot_app` 由：

```rust
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    oplog!("springboot_start", &format!("{} ({})", name, id));
    crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await
```

改为：

```rust
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await;
    oplog_result!("springboot_start", target, "", r);
    r
```

`stop_springboot_app` / `restart_springboot_app` 同构替换（`stop_app` / `restart_app`，action 分别 `springboot_stop` / `springboot_restart`）。

- [ ] **Step 3: 无 `?` 的尾返回型（update / delete / save_groups / set_global_env）改写**

`update_springboot_app` 由 `manager.update_app(&id, params).map_err(|e| e.to_string())` 改为：

```rust
    let name = manager.find_app(&id).map(|a| a.name).unwrap_or_default();
    let target = format!("{} ({})", name, id);
    let r = manager.update_app(&id, params).map_err(|e| e.to_string());
    oplog_result!("springboot_update", target, "", r);
    r
```

`delete_springboot_app` 同构（action `springboot_delete`）。

`save_springboot_groups` 改为：

```rust
    let target = format!("{} groups", groups.len());
    let r = manager.save_groups(groups).map_err(|e| e.to_string());
    oplog_result!("springboot_save_groups", target, "", r);
    r
```

`set_springboot_global_env_vars` 改为：

```rust
    let target = format!("{} vars", env_vars.len());
    let r = manager.set_global_env_vars(env_vars).map_err(|e| e.to_string());
    oplog_result!("springboot_set_global_env", target, "", r);
    r
```

- [ ] **Step 4: 含 `?` 的函数体用 `audited_async!` 包装（create / replace_jar / replace_restart）**

`create_springboot_app` 整函数体改为：

```rust
    let target = params.name.clone();
    audited_async!("springboot_create", target, "", {
        // ponytail: 有指定端口才查重，None 表示动态端口不校验
        if let Some(port) = params.port {
            let apps = manager.list_apps();
            if apps.iter().any(|a| a.port == Some(port)) {
                return Err(format!("端口 {} 已被其他应用占用", port));
            }
        }
        manager.create_app(params).map_err(|e| e.to_string())
    })
```

`replace_springboot_jar`：保留 `find_app(...)?` 在外（与一期一致：找不到应用不产生审计条目），其后整段包进包装宏：

```rust
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    let target = format!("{} ({})", app.name, id);
    audited_async!("springboot_replace_jar", target, "", {
        if app.status == crate::models::springboot::AppStatus::Running {
            return Err("运行中的应用不可换包".to_string());
        }
        let old_jar = std::path::PathBuf::from(&app.jar_path);
        let new_jar = std::path::Path::new(&new_jar_path);
        let (backup_path, new_version) =
            replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;
        manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;
        Ok(ReplaceResult {
            backup_path: backup_path.to_str().unwrap_or("").to_string(),
            old_version: app.version,
            new_version,
        })
    })
```

`replace_springboot_jar_and_restart` 同构：`use crate::models::springboot::AppStatus;` 保留在包装块内，`find_app(...)?` 在外，其余（停止 → 换包 → `update_version` → 启动 → 构造 `ReplaceResult`）包进去，action 用 `"springboot_replace_restart"`。

- [ ] **Step 5: 格式化 + 编译**

Run: `cd src-tauri && cargo fmt && cargo build 2>&1 | tail -20`
Expected: 编译通过，无 `unused import` 警告。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/commands/springboot.rs
git commit -m "feat(audit): SpringBoot 命令补采操作结果"
```

---

### Task 5: website 命令补采结果（4 处）

**Files:**
- Modify: `src-tauri/src/commands/website.rs`

**Interfaces:**
- Consumes: `audited!` / `audited_async!`
- Produces: 无

**目标/详情取值：**

| 函数 | action | target | 备注 |
|---|---|---|---|
| `save_website` | `website_save` | `format!("{} ({})", site.name, site.id)` | 同步 fn → `audited!` |
| `delete_website` | `website_delete` | 站点名；无站点时退回 `id` | 同步 fn → `audited!` |
| `set_website_enabled` | `website_toggle` | `format!("{} ({})", name, action)` | 同步 fn → `audited!` |
| `issue_site_certificate` | `acme_issue` | `format!("{} ({})", site.name, domain)` | async fn → `audited_async!`；**删除 `acme_issue_failed` 与 `acme_issue` 两个旧 `oplog!`**，失败原因由 error 字段承载 |

- [ ] **Step 1: 改 import**

`website.rs` 现有 `use crate::oplog;`（以及 `issue_site_certificate` 内对 `crate::oplog!` 的全限定调用）。把文件头 import 改为：

```rust
use crate::{audited, audited_async};
```

- [ ] **Step 2: `save_website`（同步 fn）**

```rust
#[tauri::command]
pub fn save_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    mut site: Site,
) -> Result<(), String> {
    let target = format!("{} ({})", site.name, site.id);
    audited!("website_save", target, "", {
        // 先验证 nginx 可用（失败则立即返回，不碰内存和磁盘）
        let nginx = resolve_nginx(&sm)?;
        resolve_pending_zips(&mut site, &Path::new(&nginx.install_path)).map_err(|e| e.to_string())?;
        // 写入内存（不持久化），regenerate 验证后再持久化
        wm.upsert_mem(site).map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())
    })
}
```

- [ ] **Step 3: `delete_website`（同步 fn）**

```rust
#[tauri::command]
pub fn delete_website(
    sm: State<'_, Arc<SoftwareManager>>,
    wm: State<'_, Arc<WebsiteManager>>,
    id: String,
) -> Result<(), String> {
    let site = wm.get(&id);
    let target = site
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.clone());
    audited!("website_delete", target, "", {
        if let Some(ref s) = site {
            if s.enabled {
                return Err("ERR_SITE_RUNNING_DELETE:请先停用站点后再删除".to_string());
            }
        }
        // 记录名称用于清理上传文件（必须在 wm.remove 之前获取）
        let name_seg = site.as_ref().and_then(|s| {
            let n = sanitize_seg(&s.name);
            if n.is_empty() || n == "root" { None } else { Some(n) }
        });
        wm.remove_mem(&id).map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())?;
        // 删除站点对应的上传文件（sites-data/<name>/），避免下次同名站点文件残留
        if let Some(seg) = name_seg {
            if let Ok(nginx) = resolve_nginx(&sm) {
                let data_dir = PathBuf::from(&nginx.install_path).join("sites-data").join(&seg);
                let _ = std::fs::remove_dir_all(&data_dir);
            }
        }
        Ok(())
    })
}
```

- [ ] **Step 4: `set_website_enabled`（同步 fn）**

```rust
    let name = wm.get(&id).map(|s| s.name).unwrap_or_default();
    let action = if enabled { "enable" } else { "disable" };
    let target = format!("{} ({})", name, action);
    audited!("website_toggle", target, "", {
        wm.set_enabled_mem(&id, enabled).map_err(|e| e.to_string())?;
        regenerate(&sm, &wm, true)?;
        wm.persist().map_err(|e| e.to_string())
    })
```

- [ ] **Step 5: `issue_site_certificate`（async fn）**

保留 `wm.get` / `server_name` / `sanitize_domain` 三段在外（域名解析失败不属于「申请证书」操作），其后整体包进包装宏，并删掉原来两处 `oplog!`：

```rust
    let site = wm.get(&site_id).ok_or_else(|| format!("未找到站点: {}", site_id))?;
    let raw = site
        .server_name
        .clone()
        .ok_or_else(|| "请先填写 server_name（域名）".to_string())?;
    let domain = sanitize_domain(&raw)?;
    let target = format!("{} ({})", site.name, domain);

    audited_async!("acme_issue", target, "", {
        use tauri::Emitter;
        let settings = crate::commands::config::read_settings()?;
        let acme_settings = crate::services::acme::AcmeSettings {
            dns_provider: settings.dns_provider.clone(),
            cloudflare_api_token: settings.cloudflare_api_token.clone(),
            use_staging: settings.acme_use_staging,
        };

        let nginx = resolve_nginx(&sm)?;
        let cert_dir = PathBuf::from(&nginx.install_path).join("sites-data").join("certs");

        let app_for_progress = app.clone();
        let domain_for_progress = domain.clone();
        let on_progress = move |phase: &str, msg: &str| {
            let _ = app_for_progress.emit(
                "acme-progress",
                serde_json::json!({ "domain": domain_for_progress, "phase": phase, "message": msg }),
            );
        };

        crate::services::acme::issue_certificate(&domain, &acme_settings, &cert_dir, on_progress)
            .await
            // {:#} 展开 anyhow 的 error chain，否则前端只看到最外层 context
            // （如「创建 DNS 挑战记录失败」）而看不到真实原因（如 Cloudflare 权限不足）
            .map_err(|e| format!("{:#}", e))?;

        // 更新站点 ssl 并落盘 + 重新生成 nginx 配置并 reload
        let mut updated = site.clone();
        updated.ssl.enabled = true;
        updated.ssl.acme = true;
        // 与自签一致存**相对**路径（生成 conf 时会按 conf/ 基准补 ../）；
        // 存绝对路径会在 nginx 卸载/重装到别的目录后失效
        let base = format!("sites-data/certs/{}", domain);
        updated.ssl.cert_path = Some(format!("{base}.crt"));
        updated.ssl.key_path = Some(format!("{base}.key"));
        updated.ssl.cert_expires_at =
            Some((chrono::Local::now() + chrono::Duration::days(90)).to_rfc3339());
        wm.upsert_mem(updated).map_err(|e| e.to_string())?;
        if let Err(e) = regenerate(&sm, &wm, true) {
            // 回滚内存中的 ssl，避免内存/磁盘/nginx 三者不一致
            let _ = wm.upsert_mem(site.clone());
            return Err(e);
        }
        wm.persist().map_err(|e| e.to_string())?;
        Ok(())
    })
}
```

- [ ] **Step 6: 格式化 + 编译**

Run: `cd src-tauri && cargo fmt && cargo build 2>&1 | tail -20`
Expected: 通过，无 `unused import`。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/commands/website.rs
git commit -m "feat(audit): 网站命令补采操作结果（acme_issue 合并失败态）"
```

---

### Task 6: config 命令补采结果（2 处）

**Files:**
- Modify: `src-tauri/src/commands/config.rs`

**Interfaces:**
- Consumes: `audited!` / `audited_async!`
- Produces: 无

- [ ] **Step 1: 改 import**

```rust
use crate::{audited, audited_async};
```

（删掉原 `use crate::oplog;`。）

- [ ] **Step 2: `test_dns_token`（async）**

```rust
#[tauri::command]
pub async fn test_dns_token(provider: String, token: String, zone: String) -> Result<(), String> {
    let target = format!("{} ({})", provider, zone);
    audited_async!("test_dns_token", target, "", {
        let zone = crate::commands::website::sanitize_domain(&zone)?;
        let p = crate::services::acme::dns::provider_for(&provider, &token)
            .ok_or_else(|| format!("不支持的服务商: {}", provider))?;
        let fqdn = format!("_opx-token-test.{}", zone);
        let value = format!("opx-{}", chrono::Local::now().timestamp_millis());
        p.create_txt(&fqdn, &value)
            .await
            .map_err(|e| format!("{:#}", e))?;
        let _ = p.delete_txt(&fqdn, &value).await; // 清理探针记录（尽力而为）
        Ok(())
    })
}
```

- [ ] **Step 3: `save_settings`（同步）**

```rust
#[tauri::command]
pub fn save_settings(_app: AppHandle, settings: AppSettings) -> Result<(), String> {
    audited!("save_settings", "all", "", {
        let path = paths::settings_path();
        let tmp = path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
        fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
        // 立即刷新下载代理配置
        crate::utils::download::init_download_config(
            settings.github_proxy_url,
            settings.proxy_url,
        );
        Ok(())
    })
}
```

- [ ] **Step 4: 格式化 + 编译 + 提交**

Run: `cd src-tauri && cargo fmt && cargo build 2>&1 | tail -20`
Expected: 编译通过。

```bash
git add src-tauri/src/commands/config.rs
git commit -m "feat(audit): 设置命令补采操作结果"
```

---

### Task 7: software 同步命令补采结果（9 处）

**Files:**
- Modify: `src-tauri/src/commands/software.rs`

**Interfaces:**
- Consumes: `audited_async!` / `oplog_result!`
- Produces: 无

**目标/详情取值（沿用一期拆分）：**

| 函数 | action | target | detail |
|---|---|---|---|
| `stop_software` | `stop` | `software.name` | `format!("{} ({})", software.version, software.id)` |
| `restart_software` | `restart` | 同上 | 同上 |
| `uninstall_software` | `uninstall` | `software.name` | `""` |
| `rollback_software` | `rollback` | `software.name` | `""` |
| `restore_config_backup` | `restore_backup` | `format!("{} ({})", software.name, installed_id)` | `""` |
| `write_config_form` | `config_form` | 同上 | `""` |
| `write_config_source` | `config_source` | 同上 | `""` |
| `save_custom_start_command` | `save_start_command` | `format!("{} ({})", name, installed_id)` | `""` |
| `save_startup_settings` | `save_startup` | 同上 | `""` |

统一手法：**把一期 `oplog!(...)` 那一行删掉，改为先算 `target` / `detail` 两个局部 `String`，再把该行之后的整个函数体包进 `audited_async!(action, target, detail, { ...原体... })`**（原体里的 `?`、早 `return` 都归属块内）。不变量：`find_installed(...)?` 这类「找不到记录」的前置查询保留在包装之外，与一期「不产生条目」的行为一致。

- [ ] **Step 1: 改 import**

文件头第 13 行的 `use crate::oplog;` 改为：

```rust
use crate::{audited_async, oplog};
```

（`oplog` 此时**必须保留**：`install` / `install_custom` / `start` / `do_upgrade` 四处要到 Task 8 才改造。Task 8 完成后这一行会被再次替换。）

- [ ] **Step 2: `stop_software` 示例（含早 `return Ok(true)` 的形态）**

```rust
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let target = software.name.clone();
    let detail = format!("{} ({})", software.version, software.id);

    audited_async!("stop", target, detail, {
        lifecycle::validate_stop_transition(software.status).map_err(|e| e.to_string())?;
        // …（原 1436 行起至函数末尾的全部代码，含 None 分支里的 `return Ok(true)`）
    })
```

- [ ] **Step 3: 按同一手法改其余 8 个函数**

每个函数要插入的「目标/详情」两行（放在原 `oplog!` 的位置，其后整段函数体进包装块）：

```rust
// restart_software
let target = software.name.clone();
let detail = format!("{} ({})", software.version, software.id);

// uninstall_software
let target = software.name.clone();
let detail = String::new();

// rollback_software
let target = software.name.clone();
let detail = String::new();

// restore_config_backup
let target = format!("{} ({})", software.name, installed_id);
let detail = String::new();

// write_config_form
let target = format!("{} ({})", software.name, installed_id);
let detail = String::new();

// write_config_source
let target = format!("{} ({})", software.name, installed_id);
let detail = String::new();

// save_custom_start_command
let target = format!("{} ({})", name, installed_id);
let detail = String::new();

// save_startup_settings
let target = format!("{} ({})", name, installed_id);
let detail = String::new();
```

各函数的包装块边界：

- `restart_software` / `rollback_software` / `restore_config_backup` / `write_config_form` / `write_config_source`：`find_installed(...)?`（或 `load_software_for_id(...)?`）**保留在包装块外**，块内是该行之后的全部代码，直到函数末尾的 `Ok(...)`。
- `uninstall_software`：`find_installed(...)?` 与 `oplog!` 之间的那几行保留在外；块内从 `uninstall_guard::check_uninstall_safety(&software)` 起到末尾 `Ok(true)`。
- `save_custom_start_command` / `save_startup_settings`：块内只有一处 `manager.xxx(...).map_err(|e| e.to_string())`（尾委托型）。

`save_custom_start_command` 完整示例：

```rust
    let name = manager.find_installed(&installed_id).map(|s| s.name).unwrap_or_default();
    let target = format!("{} ({})", name, installed_id);
    audited_async!("save_start_command", target, "", {
        manager
            .set_custom_start_command(&installed_id, cmd)
            .map_err(|e| e.to_string())
    })
```

`save_startup_settings` 同构，action 用 `"save_startup"`，调用 `manager.update_startup_settings(&installed_id, auto_start, order, auto_restart)`。

- [ ] **Step 4: 格式化 + 编译**

Run: `cd src-tauri && cargo fmt && cargo build 2>&1 | tail -30`
Expected: 编译通过。若某处出现借用冲突（多为「target 借用局部变量后又 move 该变量」），把该变量先 `.clone()` 进 `target` 再入块，不要改块内逻辑。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/software.rs
git commit -m "feat(audit): 软件同步命令补采操作结果"
```

---

### Task 8: 后台任务结果（install / install_custom / upgrade / start）

**Files:**
- Modify: `src-tauri/src/services/software_manager/installer.rs`
- Modify: `src-tauri/src/commands/software.rs`

**Interfaces:**
- Consumes: Task 1 `record_full` / `RESULT_*`；Task 2 `oplog_begin!` / `oplog_fail!` / `oplog_result!`
- Produces:
  - `pub fn register_install_audit(install_id: &str, action: &str, target: &str, detail: &str)`（`installer.rs`）
  - `start` / `install` / `install_custom` / `upgrade` 四条后台任务产生「进行中 + 完成（ok/fail）」两条记录

- [ ] **Step 1: `installer.rs` 加 install_id → 审计上下文小表**

文件头 `use` 区域加（若已存在同名 `use` 则跳过，不要重复引入）：

```rust
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
```

在 `emit_event` 之前加：

```rust
/// install_id → (action, target, detail)：供 emit_event 在终态补写审计完成记录。
/// ponytail: 命令侧与 emit_event 之间隔着 50 余处调用点，逐点透传审计上下文改动过大；
/// 用一张按 install_id 索引的小表收口，终态事件一次性 take 后即释放。
static INSTALL_AUDIT_CTX: LazyLock<Mutex<HashMap<String, (String, String, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 命令发起安装任务时登记审计上下文（与 oplog_begin! 同处调用）。
pub fn register_install_audit(install_id: &str, action: &str, target: &str, detail: &str) {
    INSTALL_AUDIT_CTX
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            install_id.to_string(),
            (action.to_string(), target.to_string(), detail.to_string()),
        );
}
```

- [ ] **Step 2: `emit_event` 收口终态**

```rust
fn emit_event(app: &AppHandle, payload: serde_json::Value) {
    // 终态（failed / completed）顺带补写审计完成记录：这是安装任务唯一的完成出口，
    // 覆盖 install_software / install_custom / install_from_builtin 的全部失败分支。
    let phase = payload.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    if phase == "failed" || phase == "completed" {
        if let Some(id) = payload.get("install_id").and_then(|v| v.as_str()) {
            let ctx = INSTALL_AUDIT_CTX
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(id); // take：同一 install_id 只落一条完成记录
            if let Some((action, target, detail)) = ctx {
                let (result, error) = if phase == "completed" {
                    (crate::services::software_manager::audit::RESULT_OK, String::new())
                } else {
                    (
                        crate::services::software_manager::audit::RESULT_FAIL,
                        payload
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    )
                };
                crate::services::software_manager::audit::record_full(
                    action, target, detail, result, error,
                );
            }
        }
    }
    let _ = app.emit("install-progress", payload);
}
```

- [ ] **Step 3: `commands/software.rs` 的 install / install_custom 发起记录**

`install_software` 命令体由：

```rust
    oplog!("install", &params.key, &params.version);
    let install_id = uuid::Uuid::new_v4().to_string();
```

改为：

```rust
    let install_id = uuid::Uuid::new_v4().to_string();
    oplog_begin!("install", &params.key, &params.version);
    installer::register_install_audit(&install_id, "install", &params.key, &params.version);
```

`install_custom` 命令体同构（action `"install_custom"`、target `params.name`、detail `""`）。

- [ ] **Step 4: `start_software` 改为「进行中 + 完成」**

先把 Task 7 留下的第 13 行 import 换成（此时 `oplog` 已无调用点，删掉）：

```rust
use crate::{audited_async, oplog_begin, oplog_fail, oplog_result};
```

把 `oplog!` 行换成 `oplog_begin!`，并给两处同步拒绝补 `oplog_fail!`：

```rust
    let software = manager
        .find_installed(&installed_id)
        .ok_or_else(|| format!("未找到安装记录: {}", installed_id))?;
    let audit_target = software.name.clone();
    let audit_detail = format!("{} ({})", software.version, software.id);
    oplog_begin!("start", &audit_target, &audit_detail);

    if let Err(e) = lifecycle::validate_start_transition(software.status) {
        oplog_fail!("start", &audit_target, &audit_detail, &e);
        return Err(e.to_string());
    }

    // PID 残留校验：旧 PID 仍存活则拒绝启动
    if let Some(pid) = software.pid {
        if health_check::is_process_alive(pid) {
            let msg = format!("进程 {} 仍在运行，请先停止", pid);
            oplog_fail!("start", &audit_target, &audit_detail, &msg);
            return Err(msg);
        }
    }
```

spawn 之前克隆审计标识（`async move` 需要 owned）：

```rust
    let audit_target_task = audit_target.clone();
    let audit_detail_task = audit_detail.clone();
    tauri::async_runtime::spawn(async move {
```

spawn 内两处结论各加一行记录（**`oplog_result!` 传 `result` 本身，不要加 `&`**——宏内部已取 `&`；记录放在 `if let Err(e) = result` 之前，避免值被 move）：

```rust
        if let Err(e) = ensure_dependencies(&manager_arc, &app_handle, &installed_id_for_task).await
        {
            let msg = format!("依赖编排失败：{}", e);
            oplog_fail!("start", &audit_target_task, &audit_detail_task, &msg);
            // …（原有 tracing::warn / update_runtime_fields / emit_status_changed，message 用 msg.clone()）
            return;
        }
        let result = do_start_software(&manager_arc, &app_handle, &installed_id_for_task, init_password).await;
        oplog_result!("start", &audit_target_task, &audit_detail_task, result);
        if let Err(e) = result {
            // …（原有 update_runtime_fields / emit_status_changed / tracing::error，一行都不改）
        }
```

- [ ] **Step 5: `upgrade` 发起 + 完成**

`upgrade_software` 命令体在 `let install_id = ...` 之后加：

```rust
    let audit_target = manager
        .find_installed(&installed_id)
        .map(|s| s.name)
        .unwrap_or_default();
    oplog_begin!("upgrade", &audit_target);
```

spawn 闭包改为（`oplog_result!` 同样传 `r`，不加 `&`；记录放在 error 分支之前）：

```rust
    let audit_target_for_task = audit_target.clone();
    tauri::async_runtime::spawn(async move {
        let r = do_upgrade(&manager_arc, &app, &installed_id_for_task, &install_id_for_task).await;
        oplog_result!("upgrade", &audit_target_for_task, "", r);
        if let Err(ref e) = r {
            let _ = app.emit(
                "install-progress",
                serde_json::json!({
                    "install_id": install_id_for_task,
                    "phase": "failed",
                    "error": format!("{}", e),
                    "stage": "upgrade"
                }),
            );
        }
    });
```

同时删掉 `do_upgrade` 内部第 192 行的 `oplog!("upgrade", &software.name);`。

- [ ] **Step 6: 格式化 + 编译 + 测试**

Run: `cd src-tauri && cargo fmt && cargo test --lib 2>&1 | tail -20`
Expected: 全部 PASS（≥157 passed）。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/services/software_manager/installer.rs src-tauri/src/commands/software.rs
git commit -m "feat(audit): 后台任务记录进行中与完成结果（install/upgrade/start）"
```

---

### Task 9: 前端结果列 / 过滤 / i18n

**Files:**
- Modify: `src/models/audit.ts`
- Modify: `src/modules/audit/pages/AuditLogPage.vue`
- Modify: `src/locales/zh-CN.ts`、`src/locales/en-US.ts`

**Interfaces:**
- Consumes: Task 3 的 `list_audit_entries` / `export_audit_entries` 新 `result` 参数；Task 1 的 `AuditStats.failed`
- Produces: 无

- [ ] **Step 1: 模型加字段**

`src/models/audit.ts`：

```typescript
export interface AuditEntry {
  ts: string
  action: string
  target: string
  detail: string
  /** '' = 未采集 | 'running' | 'ok' | 'fail' */
  result: string
  /** result === 'fail' 时非空 */
  error: string
}
```

`AuditStats` 加 `failed: number`：

```typescript
export interface AuditStats {
  today: number
  total: number
  /** 近 N 天失败条数 */
  failed: number
  by_action: ActionCount[]
  last_ts: string | null
}
```

- [ ] **Step 2: i18n 键（两个文件同位置追加）**

`zh-CN.ts`：

```typescript
  auditResultCol: '结果',
  auditResultOk: '成功',
  auditResultFail: '失败',
  auditResultRunning: '进行中',
  auditResultNone: '—',
  auditAllResults: '全部结果',
  auditResultUnset: '未采集',
  auditFailed: '失败 {n}',
```

`en-US.ts`：

```typescript
  auditResultCol: 'Result',
  auditResultOk: 'Success',
  auditResultFail: 'Failed',
  auditResultRunning: 'Running',
  auditResultNone: '—',
  auditAllResults: 'All results',
  auditResultUnset: 'Not recorded',
  auditFailed: '{n} failed',
```

- [ ] **Step 3: 页面加结果过滤状态与请求参数**

`AuditLogPage.vue` `<script setup>` 中：

- 新增 `const resultFilter = ref('')`；
- `loadEntries()` 的 invoke 参数里加：

```typescript
      // '' = 全部（传 null）；'__none' = 未采集（传空串）
      result: resultFilter.value === '' ? null : resultFilter.value === '__none' ? '' : resultFilter.value,
```

- `onExport()` 的 invoke 参数里加同样的 `result` 表达式（变量名同样叫 `result`）。
- 新增结果展示辅助（用 `switch` + 字面量 key，避免动态 key 过不了 vue-tsc）：

```typescript
type ResultKind = 'ok' | 'fail' | 'running' | 'none'

function resultKind(e: AuditEntry): ResultKind {
  if (e.result === 'ok' || e.result === 'fail' || e.result === 'running') return e.result
  return 'none'
}

function resultLabel(e: AuditEntry): string {
  switch (resultKind(e)) {
    case 'ok':
      return t('auditResultOk')
    case 'fail':
      return t('auditResultFail')
    case 'running':
      return t('auditResultRunning')
    default:
      return t('auditResultNone')
  }
}
```

- [ ] **Step 4: 过滤栏加结果下拉**

在操作类型 `<select>` 之后插入：

```html
      <select v-model="resultFilter" class="input" @change="reload">
        <option value="">{{ $t('auditAllResults') }}</option>
        <option value="ok">{{ $t('auditResultOk') }}</option>
        <option value="fail">{{ $t('auditResultFail') }}</option>
        <option value="running">{{ $t('auditResultRunning') }}</option>
        <option value="__none">{{ $t('auditResultUnset') }}</option>
      </select>
```

- [ ] **Step 5: 概览卡显示失败数**

「近 7 天操作」卡片加副行：

```html
      <StatCard
        :label="$t('auditTotal')"
        :value="String(stats?.total ?? 0)"
        :sub="stats ? $t('auditFailed', { n: stats.failed }) : undefined"
        icon="mdi:counter"
        accent="chart-2"
      />
```

- [ ] **Step 6: 表格加「结果」列 + 失败行红调**

表头 `auditDetailCol` 之后加：

```html
          <th>{{ $t('auditResultCol') }}</th>
```

行改成：

```html
        <tr v-for="(e, i) in entries" :key="e.ts + i" :class="{ 'row-fail': resultKind(e) === 'fail' }">
          <td class="tnum">{{ formatTs(e.ts) }}</td>
          <td><span class="action-chip">{{ e.action }}</span></td>
          <td class="truncate-cell">{{ e.target }}</td>
          <td class="truncate-cell">{{ e.detail || '—' }}</td>
          <td>
            <span class="result-chip" :class="`result-${resultKind(e)}`">{{ resultLabel(e) }}</span>
            <span v-if="e.error" class="result-error" :title="e.error">{{ e.error }}</span>
          </td>
        </tr>
```

样式追加：

```css
.row-fail {
  background: color-mix(in oklch, var(--color-destructive, #ef4444) 6%, transparent);
}
.result-chip {
  display: inline-block;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  font-weight: 600;
}
.result-ok {
  background: color-mix(in oklch, var(--color-chart-2, #22c55e) 16%, transparent);
  color: var(--color-chart-2, #22c55e);
}
.result-fail {
  background: color-mix(in oklch, var(--color-destructive, #ef4444) 16%, transparent);
  color: var(--color-destructive, #ef4444);
}
.result-running {
  background: color-mix(in oklch, var(--color-chart-3, #f59e0b) 16%, transparent);
  color: var(--color-chart-3, #f59e0b);
}
.result-none {
  color: var(--color-muted-foreground);
}
.result-error {
  display: inline-block;
  max-width: 260px;
  margin-left: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
  font-size: 11px;
  color: var(--color-destructive, #ef4444);
}
```

- [ ] **Step 7: 类型检查**

Run: `npx vue-tsc --noEmit`
Expected: 无错误。

- [ ] **Step 8: 提交**

```bash
git add src/models/audit.ts src/modules/audit/pages/AuditLogPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(audit): 操作记录页展示结果列与失败原因、结果过滤"
```

---

### Task 10: 全量验证与实机验证

**Files:** 无改动（仅验证；发现问题回到对应 Task 修）

- [ ] **Step 1: 后端全量测试**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -20`
Expected: 全部 PASS，数量 ≥ 157（基线 151 + 本计划 6 个新测试）。

- [ ] **Step 2: 后端静态检查**

Run: `cd src-tauri && cargo clippy --lib 2>&1 | tail -30`
Expected: 无 error；new warning 需处理。

- [ ] **Step 3: 前端构建**

Run: `npx vue-tsc --noEmit && npm run build 2>&1 | tail -20`
Expected: 构建成功。

- [ ] **Step 4: 实机验证（`npm run tauri dev`）**

逐项确认并记录实际观察结果：

1. 启动一个端口已被占用的软件 → 「操作记录」出现 `start` 且结果为**失败**，错误列可见原因文本（如「启动失败：…」）；该条之前还有一条 `start`「进行中」。
2. 停止同一软件 → 单条 `stop`「成功」（无「进行中」）。
3. 在 SpringBoot 页启动一个挂了的应用 → 出现 `springboot_start`「失败」+ 原因（进程意外退出 / 启动超时）。
4. 安装一个软件（正常成功）→ `install`「进行中」+ `install`「成功」两条；随后应用升级 → `upgrade`「进行中」+「成功」。
5. 安装一个不存在的版本（制造失败）→ `install`「进行中」+「失败」+ 原因。
6. 结果过滤下拉：选「失败」只剩失败条目；选「未采集」只剩一期历史条目（显示「—」）。
7. 导出 CSV → 含「结果」「错误」两列，Excel 打开中文不乱码。
8. 退出重进页面，一期历史条目仍显示「—」（证明旧数据兼容）。

- [ ] **Step 5: 收尾提交（若第 4 步有修复）**

```bash
git add -A
git commit -m "fix(audit): 实机验证发现的问题修复"
```

---

## 附：不需要改动的调用点（明确排除）

- `src-tauri/src/services/watchdog.rs`（6 处 `auto_restart*`）
- `src-tauri/src/services/system_monitor/recorder.rs`（`alert_high` / `alert_recovered`）
- `src-tauri/src/services/acme/renew_scheduler.rs`（`acme_renew` / `acme_renew_failed`）
- `src-tauri/src/commands/app.rs`（`quit_app` / `exit_app`）

这些保持一期语义（`result = ""`，页面显示「—」）。
