# QA 审查报告 — OPX B 扩展（一键启动栈 Stack）

> 审查人：software-qa-engineer ｜ 审查轮次：Round 1（发现）→ Round 2（回归）
> 审查范围：`src-tauri/src/services/stack_manager.rs`（核心服务层）、`commands/stack.rs`、`models/stack.rs`、`src/stores/stack.ts`、`src/models/stack.ts`、`src-tauri/src/lib.rs`、commit `2aca2f0`
> 测试：Rust `cargo test --lib`（stack_manager 模块 29 用例 + 全量 lib 40 用例）
> 修复提交：`5adfa65`（feat/b-stack-start，含 2 个 P1 修复 + P2 回滚过杀修复 + 29 项回归测试）

---

## 1. 测试结论与现状态

| 阶段 | 基线 | 结果 | 说明 |
|------|------|------|------|
| Round 1 | `2aca2f0` | 29 run / **28 passed / 1 failed** | `test_update_rejects_cycle_preserves_in_memory_state` FAIL（断言 left=2, right=1），确认 `update()` 拒绝环时破坏内存状态 |
| Round 2（本机独立复跑） | `5adfa65` | **29 passed / 0 failed** | 编译 1.39s，全部通过；该 P1 用例转 `ok` |
| Round 2（物理机交叉核验） | `5adfa65` | `cargo test --lib` **40 passed / 0 failed**（EXIT=0） | 主理人实跑确认 |

**结论**：源码缺陷已全部修复并回归通过，测试代码自身无问题。

---

## 2. 问题分级清单（现状态）

### P0 阻断
无（无崩溃、happy path 无数据丢失）。

### P1 严重（已闭环 ✅）
- **P1-1 `update()` 拒绝环时破坏内存状态**
  - 根因：`src-tauri/src/services/stack_manager.rs` `StackManager::update`（原行 166–192），先覆写 `inner.stacks` 可变引用再做 `compute_plan`，失败经 `?` 提前返回但覆写已生效。
  - 修复（`5adfa65`）：改为基于 `candidate = stack.clone()` 应用变更 → 对 `candidate` 跑 `compute_plan`（行 193）→ 仅成功才 `*stack = candidate`（行 196）+ `save`（行 198），保证失败原子性。
  - 验证：单测 `test_update_rejects_cycle_preserves_in_memory_state` 现 `ok`。✅
- **P1-2 `depends_on` 指向 `enabled=false` 成员时行为不一致**
  - 根因：`compute_plan`（行 228–235）丢弃“依赖 disabled 成员”的边，但 `start_one`（原行 404–414）遍历 `depends_on` 全量仍强制校验该依赖就绪 → 必然 Err。
  - 修复（`5adfa65`）：`start_one` 中 `if !di.enabled { continue; }`（行 433–435）跳过禁用依赖，与 `compute_plan` 语义对齐。
  - 验证：计划“可启动”与运行时结论一致。✅

### P2 建议 / 需注意（非阻断）
- **P2-3 回滚过杀（已修复 ✅）**：`start` 回滚（原行 362–380）会强杀“本就已在运行、非本次编排启动”的依赖服务。`5adfa65` 入口先算 `pre_running` 集合（行 324–329），仅本次真正启动的成员计入 `started`（行 354 守卫），回滚不再误杀。已闭环。
- **P2-4 就绪探测边界（未改，文档化）**：`wait_ready`/`probe_ready` 仅用 pid 存活 + 端口监听（springboot 未做 HTTP health 探活），与 C 扩展日志检测启动不同；属设计一致边界，建议在文档/注释中明确，避免误判“已就绪”。
- **P2-5 文案 typo（仍待修）**：`src/locales/zh-CN.ts:145` `nacosMysqlNotRunning` 存在「未运行或连接失败，，请先启动」的「，，」双逗号——纯文案，非 `2aca2f0` 引入，建议另开小改。
- **P2-6 前端测试覆盖缺口（非 defect）**：`src/stores/stack.ts` 仅封装 Tauri IPC（`invoke`），缺 vitest+jsdom 单测 harness；纯函数 `resolveName`/`getRuntime` 可后续补单测，不影响本功能正确性。

---

## 3. 前后端契约核对表

> 注：实际为 **10 个** stack 命令（store 的 `loadCandidates` 另包 2 个既有非 stack 命令 `list_installed_software`/`list_springboot_apps`）。

| 前端 `stores/stack.ts` | invoke 命令 | 参数 | `commands/stack.rs` | `lib.rs` 注册 |
|------------------------|-------------|------|---------------------|---------------|
| `loadStacks` | `list_stacks` | — | 行 21 | 行 237 ✓ |
| `getStack` | `get_stack` | `{ id }` | 行 29 | 行 238 ✓ |
| `createStack` | `create_stack` | `{ payload }` | 行 40 | 行 239 ✓ |
| `updateStack` | `update_stack` | `{ id, payload }` | 行 49 | 行 240 ✓ |
| `deleteStack` | `delete_stack` | `{ id }` | 行 59 | 行 241 ✓ |
| `startStack` | `start_stack` | `{ id }` | 行 69 | 行 242 ✓ |
| `stopStack` | `stop_stack` | `{ id }` | 行 79 | 行 243 ✓ |
| `restartStack` | `restart_stack` | `{ id }` | 行 89 | 行 244 ✓ |
| `exportStack` | `export_stack` | `{ id, path }` | 行 99 | 行 245 ✓ |
| `importStack` | `import_stack` | `{ path }` | 行 109 | 行 246 ✓ |

全部 10 个命令名/参数前后端一致，`lib.rs` 均已注册。

**models serde 核对（`src/models/stack.ts` vs `src-tauri/src/models/stack.rs`）**
- `StackItemRefType`：Rust `#[serde(rename_all="snake_case")]` → `software`/`springboot`；TS `'software'|'springboot'` ✓
- `StackMemberStatus`：`pending/starting/running/stopping/stopped/failed` 两端一致 ✓
- `StackItem` / `Stack` / `StackMemberRuntime` / `StackStartPlan` / `CreateStackPayload` / `UpdateStackPayload` / `StackStatusEvent`：字段名逐一对应；Rust 的 `depends_on`(default `[]`)、`enabled`(default true)、`retry`(default 0)、`updated_at`(default `""`) 等 default 与 TS 类型匹配（前端始终下发这些字段）✓
- `UpdateStackPayload` 两端均为全可选 ✓

契约无差异。

---

## 4. 对 commit `2aca2f0` 的回归确认

- `git show --stat 2aca2f0` 实际仅 **2 个文件、+2/-2**：`BackupRestoreDialog.vue`、`LogViewerDialog.vue`，均把 `const emit = defineEmits<...>()` 改为 `defineEmits<...>()`（移除未使用变量以过 `noUnusedLocals`）。`defineEmits` 声明保留，模板仍用 `$emit('close')` → **纯编译修复，C 功能零回归**。
- 主理人描述的 locales 改动（nacos 逗号 / autoScroll 重复键）**不在本 commit**：`git show` 仅含上述 2 个 `.vue`。当前 locale 实测：
  - `autoScroll` 在 `en-US.ts:244`、`zh-CN.ts:244` 各出现 **1 次**（对称，无重复键可删）；
  - `nacosMysqlNotRunning`(`zh-CN.ts:145`) 仍有「，，」双逗号 typo（预存、非本 commit 引入、纯文案，见 P2-5）；
  - C 扩展日志/备份 i18n key 本 commit 未触碰 → 未误删，两份 locale 仍对称。
- **结论**：`2aca2f0` 仅修编译，无 C 功能回归；locales 相关描述与本 commit 不符，autoScroll 已无重复、nacos 双逗号仍待修。

---

## 5. 路由判定

- **Round 1**：发现 P1-1（实测 FAIL，确为源码缺陷）+ P1-2（源码逻辑缺陷）→ 路由 **Engineer**，已发 software-engineer 并附修复建议。
- **Round 2（`5adfa65` 回归）**：本机 29/29 + 物理机 40/40 全通过，无遗留 fail；测试代码本身无 bug。

**→ 路由 = NoOne（源码缺陷已闭环，无需再派发）。**

---

## 6. 用户本机 `npm run tauri:dev` 验收项

以下依赖本机已装实例与真实进程，无法单元测试覆盖，建议合并前由用户本机走查：

1. **真实编排**：MySQL / Redis / Nginx + SpringBoot 应用的一键启动 / 停止 / 重启，验证拓扑分层与批内并发顺序正确。
2. **事件推送**：编排过程中 `stack-status-changed` 事件按层推送，前端运行面板状态可视化正确刷新。
3. **导出 / 导入**：通过文件对话框导出栈 JSON、再导入（应生成新 id、保留 items/depends_on）。
4. **disabled 成员编排**：禁用某成员后，其依赖方与上游的 UI 行为与启动表现符合“禁用即退出编排”语义（对应 P1-2 修复）。
5. **回滚表现**：构造一个会阻断失败的栈（如某 enabled 成员依赖一个启动必失败的成员），确认回滚仅停本次启动的成员、不误杀预运行服务（对应 P2-3 修复）。
6. **文案**：`nacosMysqlNotRunning` 双逗号 typo（P2-5）顺手核对。

---

## 7. 最终结论（给主理人）

路由 = NoOne 成立；B 扩展后端核心逻辑单测 29/29（全量 lib 40/40）通过，两个 P1 源码缺陷（update 原子性、disabled 依赖不一致）与 P2 回滚过杀均已闭环并回归，**可合并 dev**。合并前建议由用户本机 `npm run tauri:dev` 走查第 6 节 6 项验收（尤其真实编排与回滚表现）；`2aca2f0` 仅编译修复、无 C 功能回归，可随本次一并合入。
