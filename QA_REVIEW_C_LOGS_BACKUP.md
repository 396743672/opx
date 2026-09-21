# QA 审查报告 · opx C 扩展（日志查看器 + 备份/恢复）

- **分支**: `feat/c-logs-backup`
- **HEAD**: `0892a10` (fix(c): 接线实例卡片 log/backup/reset 事件并新增重置入口)
- **审查人**: 严过关（Yan / software-qa-engineer）
- **审查方式**: 静态代码审查 + 实写 `#[cfg(test)]` 单元测试 + 本机 `cargo check`/`cargo test` 实证（cargo 1.96.1 可用，target 已缓存）
- **结论路由**: **Engineer（源码 Bug）**

> 说明：本仓 C 扩展后端**当前无法编译**（见 P0）。为验证我写入的测试代码本身正确，我曾在临时修复 P0 源码后跑通 `cargo test --lib software_manager`（结果 11 测试：7 通过 / 4 失败，4 个失败均为有意复现的源码 Bug 演示），随后已 `git checkout` 还原源码，仅保留测试模块。故交付物中源码处于原始（broken）状态，供工程师修复。

---

## 一、分级问题清单

### P0 · 阻断（crate 无法编译，必须先行修复）

**P0-1 · `src-tauri/src/services/software_manager/log_viewer.rs:219`**
- 现象：`let has_nl = byte_pos + line_len as u64 < total;` 被 Rust 解析为泛型参数——`<` 被当作 `u64` 的泛型实参，编译报：
  - `error: < is interpreted as a start of generic arguments for u64, not a comparison` (`:219:49`)
  - `error: expected one of ! ( + , :: : < = or >, found ;` (`:219:56`)
- 触发：任何 `cargo build` / `cargo test` 都会失败。
- 修复：`(byte_pos + line_len as u64) < total`（或 `byte_pos + (line_len as u64) < total`）。

**P0-2 · `src-tauri/src/services/software_manager/providers/mysql.rs:45-52`（redis/nginx/postgresql/nacos 同构）**
- 现象：`fn log_sources(&self, ctx: &LogContext) -> Vec<LogSource>` 被写在 `impl XxxProvider { … }` 块的右花括号之后（即模块顶层），带 `&self` 参数 → 编译错误。当前仅 `mysql.rs:52` 报 `unexpected closing delimiter: }`，**因为 rustc 在遇到首个解析错误后停止**，其余 4 个 provider 结构完全一致，修复 mysql 后会依次暴露。
- 结构根因：各 provider 在 `impl XxxProvider { pub fn new() … }` 之后、 trait impl 之前，错误地多放了一个 `}` 把 impl 提前关闭，导致 `log_sources` 泄漏到模块顶层。
- 触发：任何编译/测试都会失败（5 个 provider 全部如此，目前被 mysql 的首错掩盖）。
- 修复方向：将 `fn log_sources` 移入 `impl SoftwareProvider for XxxProvider { … }` 块内（参考正确范例 `mongodb.rs:130`）。⚠️ 仅移回 inherent `impl XxxProvider` 仍不生效，见 P1-1。

### P1 · 严重（可编译但功能错误 / 安全缺陷）

**P1-1 · 5 个 provider 的 `log_sources` 未覆盖 trait 默认方法（级别筛选静默失效）**
- 文件：`providers/{mysql,redis,nginx,postgresql,nacos}.rs`（正确范例：`mongodb.rs:130`）
- 现象：`log_sources` 即便在 inherent `impl XxxProvider` 内，也**不覆盖** `trait SoftwareProvider` 的默认 `log_sources`（`providers/mod.rs:60`，默认返回 `has_levels=false` 的来源）。`get_log_sources` 命令走 trait 默认方法 → `has_levels` 恒为 `false` → 前端 `LogViewerDialog` 的“级别”筛选下拉**永不出现**。仅 mongodb 正确（其 `log_sources` 在 `impl SoftwareProvider for MongoDbProvider` 内）。
- 影响：C 扩展的招牌功能“结构化日志级别筛选”对 mysql/redis/nginx/postgresql/nacos 四者静默失效。
- 修复：把 `fn log_sources` 移入各自的 `impl SoftwareProvider for XxxProvider { … }` 块。

**P1-2 · `backup.rs:231-233`（`create_snapshot`）快照滚动保留“保留 5 个”未实现**
- 现象：`create_snapshot` 仅 `manifest.push(meta)` 后 `write_manifest`，未按设计文档 §3.3 的“保留最近 5 个”做排序+裁剪，manifest 会无限膨胀。
- 触发/复现：连续 `create_snapshot` 6 次，`list_snapshots` 返回 6 条（期望 ≤5）。
- 测试实证：测试 `test_snapshot_retention_keeps_five` 当前 **FAIL** 证实。
- 修复：push 后按 `created_at` 排序，保留最近 5 条并删除多余 zip 文件。

**P1-3 · `backup.rs:122/126/129-130`（`extract_zip_to_data_dirs`）zip-slip 路径穿越护栏失效**
- 现象：条目名 `file.name()` 未规范化，直接用 `install_path.join(&name)` 拼出 `target`，再用 `target.starts_with(install_path)` 做护栏。`Path::join` **不规范化** `..`，形如 `sub/../../evil.txt` 的条目拼出的字面量仍以 `install_path` 为前缀 → `starts_with` 通过，但真正 `File::create(target)` 时路径被文件系统规范化，写出到 `install_path` 之外 → **路径穿越**。
- 触发/复现：构造含 `../` 穿透条目的 zip，`restore_snapshot` 不拒绝。
- 测试实证：测试 `test_restore_rejects_path_traversal` 当前 **FAIL** 证实。
- 修复：先规范化 `target`（用 `components()` 过滤 `ParentDir`，或 `canonicalize` 后比较）；护栏改为“规范化后仍以 `install_path`/某 `data_dir` 为前缀且无 `..`”；绝对条目必须落在某 `data_dir` 内。

**P1-4 · `log_viewer.rs:207`（`read_backward_filtered`）历史分页偏移算错**
- 现象：`let content_start = total - all.len() as u64;` 应为 `from_byte - all.len() as u64;`。`all` 是从 `from_byte` 向前回溯收集的字节，其首偏移应为 `from_byte - all.len()`；用 `total` 仅在 `from_byte == total`（tail 模式）时巧合正确，在 history/before 模式（`before=true`，`from_byte < total`）会算错 `start_offset` → “加载更早”分页错位。
- 触发/复现：`read_backward_filtered(path, total, from_byte<total, limit, filter)`，`start_offset` 不等于 `from_byte - all.len()`。
- 测试实证：测试 `test_read_backward_before_mode_offset` 当前 **FAIL**（期望 0，实际 9）证实。
- 修复：`let content_start = from_byte - all.len() as u64;`

### P2 · 建议（防御性 / 死代码 / 健壮性）

**P2-1 · `lifecycle.rs:120-122`（`stdout_log_path`）installed_id 未做文件名安全化**
- 现象：`install_path.join("logs").join(format!("opx-{}.log", installed_id))` 未过滤 `: / \ ..` 等非法字符。当前 `installed_id` 为 UUID 故实际安全，但属防御性缺失。
- 触发/复现：`stdout_log_path(p, "a/../b:c")` 返回 `…/opx-a/../b:c.log` 未净化。
- 测试实证：`test_stdout_log_path_sanitizes_illegal_chars` 当前 **FAIL** 证实（期望净化，实际 `b:c.log`）。
- 修复：对 `installed_id` 做 sanitize（替换非法字符为 `_`，去除 `..`）。

**P2-2 · `providers/mod.rs:72`（`log_level_pattern`）死 API**
- 现象：trait 默认方法 `log_level_pattern()` 全仓库无任何调用点（grep 仅 `mod.rs:72` 一处定义）。前端用内置默认正则，后端该扩展点从未被命令/服务读取 → 死代码，易误导维护者以为存在“provider 自定义级别正则”通道。
- 修复：若确需，由 `read_log` 服务读取 `provider.log_level_pattern()` 并透传；否则删除该默认方法。

**P2-3 · `log_viewer.rs` 回溯块 UTF-8 边界（健壮性）**
- 现象：`read_backward_filtered`/`read_since` 用 `String::from_utf8_lossy(&all)` 后 `split('\n')`。若回溯块边界恰截断多字节 UTF-8 字符，lossy 会替换为 `U+FFFD`，可能导致该行匹配/偏移轻微偏差（不 panic，仅为健壮性建议）。
- 修复：按字符边界截断，或块边界对齐到 `\n` 后再 decode。

---

## 二、前后端契约一致性核对表

### 2.1 八个 Tauri 命令（前端 `stores/ops.ts` ↔ 后端 `commands/software.rs` ↔ `lib.rs:195-202` 注册）

| # | 命令 (snake) | 前端调用 (camel) | 后端入参 | 前端传参 | 返回 (Rust) | 前端类型 | 结论 |
|---|---|---|---|---|---|---|---|
| 1 | `get_log_sources` | `getLogSources` | `installed_id:String` | `installedId` | `Vec<LogSource>` | `LogSource[]` | ✅ |
| 2 | `read_log` | `readLog` | `installed_id`,`source_index:usize`,`offset:Option<u64>`,`before:Option<bool>`,`limit:Option<u64>`,`keyword:Option<String>`,`regex:bool`,`level:Option<String>` | `installedId,sourceIndex,offset,before,limit,keyword,regex,level` | `LogChunk` | `LogChunk` | ✅ 8 参全对 |
| 3 | `download_log` | `downloadLog` | `installed_id`,`source_index:usize`,`dest_path:String` | `installedId,sourceIndex,destPath` | `()` | `void` | ✅ |
| 4 | `create_snapshot` | `createSnapshot` | `installed_id`,`mode:BackupMode`,`name:Option<String>`,`note:Option<String>`(+`app:AppHandle`注入) | `installedId,mode,name,note` | `SnapshotMeta` | `SnapshotMeta` | ✅ |
| 5 | `list_snapshots` | `listSnapshots` | `installed_id` | `installedId` | `Vec<SnapshotMeta>` | `SnapshotMeta[]` | ✅ |
| 6 | `restore_snapshot` | `restoreSnapshot` | `installed_id`,`snapshot_id:String`,`force:bool` | `installedId,snapshotId,force` | `()` | `void` | ✅ |
| 7 | `delete_snapshot` | `deleteSnapshot` | `installed_id`,`snapshot_id:String` | `installedId,snapshotId` | `()` | `void` | ✅ |
| 8 | `reset_instance` | `resetInstance` | `installed_id` | `installedId` | `()` | `void` | ✅ |

- 8 个命令**全部在 `lib.rs:195-202` 注册** ✅
- `download_log` 命令层（`commands/software.rs:1367`）正确地将 `installed_id + source_index` 经 `list_log_sources` 解析为 `source.path`，再调用 `log_viewer::download_log(path, dest)`——前端只传 `installedId/sourceIndex/destPath` 的契约完全成立，**无缺口**。

### 2.2 i18n key 对称性（`zh-CN.ts` vs `en-US.ts`）
- 两文件各 **458** 个顶层 key，`diff` 结果为**空** → **完全对称**，无缺失/多余 key ✅

### 2.3 Rust 模型 ↔ 前端模型字段对齐（`models/software.rs` ↔ `src/models/software.ts`）

| 类型 | Rust 字段 | 前端字段 | 结论 |
|---|---|---|---|
| `LogSource` | `path:String`, `kind:LogSourceKind`, `has_levels:bool`, `level_pattern:Option<String>` | `path`, `kind`, `has_levels`, `level_pattern:string\|null` | ✅ |
| `LogChunk` | `lines:Vec<String>`, `start_offset:u64`, `end_offset:u64`, `total_bytes:u64`, `has_more:bool`, `truncated:bool` | `lines`, `start_offset`, `end_offset`, `total_bytes`, `has_more`, `truncated` | ✅ |
| `SnapshotMeta` | `id`,`created_at`,`source_key`,`source_version`,`major_version:Option<u32>`,`size_bytes:u64`,`format`,`name:Option<String>`,`note:Option<String>` | 同（含 `major_version:number\|null`、`name/note:string\|null`） | ✅ |
| `BackupMode` | `StopAndBackup`,`Hot` | `'StopAndBackup'\|'Hot'` | ✅ |
| `LogSourceKind` | `StdoutRedirect`,`ProviderFile` | `'StdoutRedirect'\|'ProviderFile'` | ✅ |

> 全部逐字段对齐，序列化无障碍。

---

## 三、写入的测试

测试以 `#[cfg(test)] mod tests` 形式**直接写入对应 Rust 文件**，使用 `std::env::temp_dir()`（未引入任何新 crate / 未用 `tempfile`）。

| 文件 | 测试函数 | 覆盖点 | 当前结果 |
|---|---|---|---|
| `services/software_manager/backup.rs` | `test_parse_major_version` | 大版本号解析（"8.4.11"→8 / "RELEASE.2025"→None） | PASS |
| | `test_snapshot_retention_keeps_five` | **快照滚动保留（保留 5 个）** | **FAIL → P1-2** |
| | `test_restore_rejects_path_traversal` | **restore 路径穿越防护** | **FAIL → P1-3** |
| `services/software_manager/lifecycle.rs` | `test_stdout_log_path_shape` | 路径形态 `…/logs/opx-<id>.log` | PASS |
| | `test_stdout_log_path_sanitizes_illegal_chars` | **stdout 文件名安全（installed_id 净化）** | **FAIL → P2-1** |
| `services/software_manager/log_viewer.rs` | `test_line_filter_keyword` | 关键字过滤 | PASS |
| | `test_line_filter_regex` | 正则过滤 | PASS |
| | `test_line_filter_level_default_regex` | **日志级别过滤（默认正则）** | PASS |
| | `test_read_since_basic` | 增量读取（朝 EOF） | PASS |
| | `test_read_backward_tail_basic` | 末尾回溯（tail 模式） | PASS |
| | `test_read_backward_before_mode_offset` | **history/before 模式偏移正确** | **FAIL → P1-4** |

- **统计**：11 个测试 / 7 通过 / 4 失败。4 个失败均为**有意复现的源码 Bug 演示**（retention、zip-slip、before 偏移、installed_id 净化），验证测试代码本身正确。
- **团队要求的 4 个覆盖点全部命中**：快照滚动保留（`test_snapshot_retention_keeps_five`）、restore 路径穿越防护（`test_restore_rejects_path_traversal`）、日志级别过滤（`test_line_filter_level_default_regex` 及 keyword/regex 共 3 个）、文件名安全（`test_stdout_log_path_sanitizes_illegal_chars`）。

---

## 四、前端入口复核结论（基于 HEAD `0892a10`）

文件：`src/modules/software-manager/pages/SoftwareListPage.vue`

| 入口 | 绑定 | 行为 | 结论 |
|---|---|---|---|
| `@log="onLog(item)"` (L56) | `onLog` (L295-297) 设 `logTarget` | 渲染 `LogViewerDialog` (L86-91) | ✅ 已绑定，非死按钮 |
| `@backup="onBackup(item)"` (L57) | `onBackup` (L299-302) 设 `backupInitialTab='snapshots'` | 渲染 `BackupRestoreDialog` 且 `:initial-tab="snapshots"` (L92-98) | ✅ 打开“快照列表”Tab |
| `@reset="onReset(item)"` (L58) | `onReset` (L304-307) 设 `backupInitialTab='reset'` | 渲染 `BackupRestoreDialog` 且 `:initial-tab="reset"` (L92-98) | ✅ **一键重置直达 Tab B** |

- 三个按钮均有对应 handler 且渲染对应对话框，**无死按钮** ✅
- “一键重置直达 Tab B” 已确认：`onReset` → `backupInitialTab='reset'` → `BackupRestoreDialog :initial-tab="reset"`。
- 结论：HEAD `0892a10`（“fix(c): 接线实例卡片 log/backup/reset 事件并新增重置入口”）**实现正确**。

---

## 五、路由判定结论

**Engineer（源码 Bug）**。

理由：所有 P0 / P1 / P2 均位于**源码**（`providers/*.rs`、`log_viewer.rs`、`backup.rs`、`lifecycle.rs`、`providers/mod.rs`），**非测试代码**。测试代码本身正确（11 测试 7 通过 / 4 失败均为有意 Bug 演示；在临时修复源码后可运行验证）。当前 crate 因 P0 无法编译，测试暂不能在原始 HEAD 跑通——这本身就是 P0 的实证。测试代码无需我（QA）自行修改。

---

## 六、待用户本机验证项

> 以下必须在用户本机执行（沙箱无 GUI / 无 `npm run tauri:dev` 运行时）。

1. **`cargo build`（或 `cargo check`）**
   - 验证 P0-1 / P0-2 修复后 crate 可编译；
   - **重点确认** redis/nginx/postgresql/nacos 在 mysql 修复后不再出现模块级 `fn(&self)` 错误（rustc 早停会掩盖其余 4 个）。

2. **`cargo test --lib software_manager`**
   - 修复 P1-2 / P1-3 / P1-4 / P2-1 后，原 FAIL 的 4 个测试应转 PASS（快照保留、路径穿越拦截、before 偏移、installed_id 净化）；
   - 其余 7 个保持 PASS → 全绿 11/11。

3. **`npm run tauri:dev` 手动验证**
   - `LogViewerDialog`：mysql/redis/nginx/postgresql/nacos 实例应出现“级别”筛选下拉（验证 P1-1 修复后 `has_levels=true`；mongodb 本就正常）；
   - 级别/关键字/正则过滤、tail/增量/历史分页（before 模式）的“加载更早”是否正确（验证 P1-4）；
   - `BackupRestoreDialog`：连续创建 >5 个快照，列表最多保留 5 个（验证 P1-2）；恢复含 `../` 穿透条目的恶意 zip 应被拒绝（验证 P1-3）；
   - 一键重置（Tab B）：运行中实例应被拒（需先停服），停止实例执行空态重建；
   - 下载日志：选来源 + 目标路径，文件应被拷贝到目标（验证 `download_log` 契约）。

4. **跨平台验证**（Windows / Linux / macOS）
   - stdout 日志文件名与路径（P2-1 净化）、zip 恢复路径（P1-3 规范化）在目标平台的 `Path` 行为一致。

---

## 附：实证编译错误（原始 HEAD，未改源码）

```
error: expected one of `!`, `(`, `+`, `,`, `::`, `:`, `<`, `=`, or `>`, found `;`
   --> src/services/software_manager/log_viewer.rs:219:56
error: `<` is interpreted as a start of generic arguments for `u64`, not a comparison
   --> src/services/software_manager/log_viewer.rs:219:49
error: unexpected closing delimiter: `}`
  --> src/services/software_manager/providers/mysql.rs:52:1
error: could not compile `opx` (lib test) due to 3 previous errors
```
> 仅 mysql.rs:52 暴露，因 rustc 早停；redis/nginx/postgresql/nacos 结构同构，修复 mysql 后会依次暴露。

## 附：交付物文件清单

- 测试模块写入（仅新增，未改源码逻辑）：
  - `src-tauri/src/services/software_manager/backup.rs`（+`#[cfg(test)] mod tests`，3 函数）
  - `src-tauri/src/services/software_manager/lifecycle.rs`（+`#[cfg(test)] mod tests`，2 函数）
  - `src-tauri/src/services/software_manager/log_viewer.rs`（+`#[cfg(test)] mod tests`，6 函数）
- 本报告：`/d/object/opx/QA_REVIEW_C_LOGS_BACKUP.md`
- 仓库状态：`git status` 仅上述 3 文件 modified（+236 行，纯测试），源码无 diff，未 commit。

---

## 七、修复后实证验证（commit fe613af）

工程师于 `fe613af`（"fix(c): 修复 C 扩展后端编译与逻辑缺陷（P0/P1/P2#9）"）提交源码修复，并将 QA 写入的 3 个测试模块一并纳入提交。QA 在本机**实跑 `cargo test`**（cargo 1.96.1，target 已缓存）复核，而非依赖字节级推演。

- **编译**：crate 成功编译，无错误。此前被 rustc 早停掩盖的 redis/nginx/postgresql/nacos 4 个 provider 的模块级 `fn(&self)` 问题，随 5 处 `log_sources` 移入 `impl SoftwareProvider for XxxProvider` 一并解决（P0 全清）。
- **测试结果**：
  ```
  running 11 tests
  test services::software_manager::backup::tests::test_parse_major_version ... ok
  test services::software_manager::lifecycle::tests::test_stdout_log_path_shape ... ok
  test services::software_manager::lifecycle::tests::test_stdout_log_path_sanitizes_illegal_chars ... ok
  test services::software_manager::log_viewer::tests::test_line_filter_keyword ... ok
  test services::software_manager::log_viewer::tests::test_line_filter_regex ... ok
  test services::software_manager::log_viewer::tests::test_read_backward_tail_basic ... ok
  test services::software_manager::log_viewer::tests::test_read_backward_before_mode_offset ... ok
  test services::software_manager::log_viewer::tests::test_read_since_basic ... ok
  test services::software_manager::backup::tests::test_snapshot_retention_keeps_five ... ok
  test services::software_manager::log_viewer::tests::test_line_filter_level_default_regex ... ok
  test services::software_manager::backup::tests::test_restore_rejects_path_traversal ... ok

  test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```
  - 原 4 个 FAIL 用例（retention / zip-slip / before 偏移 / installed_id 净化）全部转 PASS → 证实 **P1-2 / P1-3 / P1-4 / P2-1 已修复**。
  - 原 7 个 PASS 用例保持 PASS。
- **残留告警（已消除）**：`log_viewer.rs:12` `unused import: PathBuf` 已由工程师在 `1a01a54` 删除；复跑 `cargo test --lib software_manager` 确认 **0 warning、11 passed; 0 failed（RC=0）**。

### 验证后状态
- 原路由 **Engineer** 的 P0 / P1 / P2#9 缺陷已全部修复，并经**实证验证通过**（11/11）。
- 仍开放的非阻断项（原 P2 建议，工程师本轮未处理，符合预期）：
  - **P2-2** `providers/mod.rs:72` `log_level_pattern` 死 API（不影响功能，删除或透传均可）。
  - **P2-3** `log_viewer.rs` 回溯块 UTF-8 边界 lossy 解码（不 panic，健壮性建议）。
- 仍需用户本机验证（沙箱无 GUI / 无 `npm run tauri:dev`）：见"六、待用户本机验证项"——`cargo build` 交叉确认、`npm run tauri:dev` 手动 UI 验证（级别下拉出现、分页正确、快照≤5、恶意 zip 被拒、重置护栏、下载契约）、跨平台路径行为。

### 本轮路由判定：NoOne（全部转绿，无需再路由）
源码缺陷已正确路由工程师并修复；测试代码本身正确（失败→通过转变符合预期），无需 QA 自改。
