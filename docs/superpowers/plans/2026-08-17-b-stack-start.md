# B 扩展（一键启动栈 Stack）实施计划

> 文档类型：实施计划（Plan）
> 分支：`feat/b-stack-start`
> 对应 PRD：`2026-08-17-b-stack-start-prd.md`
> 对应设计：`2026-08-17-b-stack-start-design.md`
> 文档语言：中文

## ⚠️ 依赖与包约束（关键）

- **不引入任何新 crate**：后端沿用现有 `tauri`、`serde`、`std`；复用 `SoftwareManager` / `SpringBootManager` / `health_check`。
- **不引入任何新 npm 包**：前端沿用现有 `vue` / `pinia` / `tailwindcss` / `@tauri-apps/api` / `i18n`。
- 所有新增能力均通过**新增文件 + 注册到现有 `invoke_handler!`** 实现，不改动既有依赖清单（Cargo.toml / package.json 无需改动）。

---

## 文件清单（File Manifest）

### 后端（新增 / 修改）

| 文件 | 动作 | 说明 |
| --- | --- | --- |
| `src-tauri/src/models/stack.rs` | 新增 | `Stack` / `StackItem` / 运行态 数据模型 + serde |
| `src-tauri/src/models/mod.rs` | 修改 | 增加 `pub mod stack;` |
| `src-tauri/src/commands/stack.rs` | 新增 | 8 个 CRUD/编排命令（+ R7 导出导入） |
| `src-tauri/src/commands/mod.rs` | 修改 | 增加 `pub mod stack;` |
| `src-tauri/src/services/stack_manager.rs` | 新增 | 持久化 + 拓扑排序 + 逐批编排 + 状态聚合 |
| `src-tauri/src/lib.rs` | 修改 | 注册 `StackManager` State + 注册命令到 `invoke_handler!` |

### 前端（新增 / 修改）

| 文件 | 动作 | 说明 |
| --- | --- | --- |
| `src/models/stack.ts` | 新增 | 前端类型声明（对应后端 serde） |
| `src/stores/stack.ts` | 新增 | Pinia store：封装命令 invoke + 运行态 + 订阅事件 |
| `src/modules/stack/StackList.vue` | 新增 | 栈列表页（卡片/表格 + 操作按钮） |
| `src/modules/stack/StackEditDialog.vue` | 新增 | 创建/编辑对话框（勾选成员、排序、`depends_on`） |
| `src/modules/stack/StackRunPanel.vue` | 新增 | 运行面板（依赖图 + 实时状态 + 进度） |
| `src/router/index.ts` | 修改 | 新增 `/stacks` 路由 |
| `src/layouts/Sidebar.vue` | 修改 | 新增「栈 / Stacks」侧边栏入口 |
| `src/locales/*` | 修改 | 新增 `stack` 相关 i18n 词条 |

---

## 任务列表（Tasks，复选框 + 依赖 + 验收标准）

> 格式：`T{n} — 名称 [依赖] (覆盖需求 R*)`
> 验收标准均为"可编译 / 可运行 / 行为符合 PRD 与设计的客观判定"。

### T1 — 后端数据模型与持久化骨架
**依赖**：无
**覆盖需求**：R1
**涉及文件**：`src-tauri/src/models/stack.rs`（新增）、`src-tauri/src/models/mod.rs`（修改）、`src-tauri/src/services/stack_manager.rs`（新增：`StackManager` 结构 + `load`/`save` `data_dir/stacks.json` + `new(Arc<SoftwareManager>, Arc<SpringBootManager>)`）、`src-tauri/src/lib.rs`（修改：注册 `StackManager` State）
- [ ] `models/stack.rs` 定义 `Stack` / `StackItem` / `StackItemRefType` / 运行态枚举，`#[derive(Serialize, Deserialize)]` 完整，字段与 PRD §4 R1 一致（`id/name/description/items/created_at`）。
- [ ] `StackManager` 能在 `data_dir/stacks.json` 不存在时创建空文件、存在时正确加载；保存为格式化 JSON。
- [ ] `lib.rs` 中以 `Arc<StackManager>` 注册 State，携带已存在的 `Arc<SoftwareManager>` / `Arc<SpringBootManager>`。
- [ ] `cargo build`（src-tauri）通过，无新增警告（不破坏现有 `Cargo.toml`）。

### T2 — 栈 CRUD 命令
**依赖**：T1
**覆盖需求**：R2
**涉及文件**：`src-tauri/src/commands/stack.rs`（新增：`list_stacks`/`get_stack`/`create_stack`/`update_stack`/`delete_stack`）、`src-tauri/src/commands/mod.rs`（修改）、`src-tauri/src/lib.rs`（修改：`invoke_handler!` 注册 5 个命令）
- [ ] 5 个命令签名与设计 §3.3 一致，均返回 `Result<T, String>`。
- [ ] `create_stack` / `update_stack` 调用 `build_plan` 做**保存前环检测**，存在环时返回明确环路径错误（不写入）。
- [ ] 命令在 `invoke_handler!` 注册成功；`tauri dev` 启动后可在前端/REPL 调用返回正确数据。
- [ ] `delete_stack` 删除后落盘，`list_stacks` 数量相应减少。

### T3 — 一键启动 / 停止 / 重启编排
**依赖**：T1
**覆盖需求**：R3、R4、R9、R10
**涉及文件**：`src-tauri/src/services/stack_manager.rs`（新增：`build_plan` 拓扑排序 + 环检测、`start` 逐批启动 + 依赖就绪探测 + 重试 + 回滚、`stop` 逆序停止、`restart` 先停后起）、`src-tauri/src/commands/stack.rs`（新增：`start_stack`/`stop_stack`/`restart_stack`）、`src-tauri/src/lib.rs`（修改：注册 3 个命令）
- [ ] `build_plan` 用 Kahn 算法产出 `layers`；存在环返回 `Err(环路径)`，无环返回分层计划。
- [ ] `start_stack` 按 `depends_on` 拓扑序为主、`order` 同层 tie-break；逐批启动，批内并发（`spawn`）。
- [ ] 每个成员启动前**复用 `health_check::is_process_alive` / `is_port_free`** 探测其依赖是否就绪（R9）；依赖未就绪则标记 Failed 并**自动回滚已启动成员**。
- [ ] 单成员失败按 `retry` 重试（R10），默认不阻断整栈，除非其 `depends_on` 上游失败。
- [ ] `stop_stack` 逆序优雅停止；`restart_stack` 先停后起返回新 `StackStartPlan`。
- [ ] 启动过程 emit `stack-status-changed` 事件，含整体状态与成员状态。

### T4 — 导出 / 导入
**依赖**：T1、T2
**覆盖需求**：R7
**涉及文件**：`src-tauri/src/commands/stack.rs`（新增：`export_stack` / `import_stack`）、`src-tauri/src/services/stack_manager.rs`（读写栈 JSON 文件）、`src-tauri/src/lib.rs`（修改：注册 2 个命令）
- [ ] `export_stack(id)` 导出该栈 JSON（含 `items`/`depends_on`）到指定路径，供团队分享。
- [ ] `import_stack(path)` 解析 JSON 并重建（重新生成 `id`/`created_at`，保存落盘）；导入时同样做环检测。
- [ ] 导出的 JSON 可再导入还原等价栈（字段无损）。

### T5 — 前端状态层与导航
**依赖**：T2
**覆盖需求**：R5（数据层）、R6、R8（订阅基础）
**涉及文件**：`src/models/stack.ts`（新增）、`src/stores/stack.ts`（新增）、`src/router/index.ts`（修改）、`src/layouts/Sidebar.vue`（修改）、`src/locales/*`（修改）
- [ ] `models/stack.ts` 类型与后端 serde 一一对应。
- [ ] `stores/stack.ts` 封装 8 个命令的 `invoke`、缓存栈列表与运行态、订阅 `stack-status-changed` 并刷新。
- [ ] `router/index.ts` 新增 `/stacks` 路由；`Sidebar.vue` 新增「栈 / Stacks」入口，沿用现有视觉风格。
- [ ] i18n 新增 `stack` 词条；不引入新 npm 包。

### T6 — 前端栈管理页（列表 + 创建/编辑）
**依赖**：T5
**覆盖需求**：R5
**涉及文件**：`src/modules/stack/StackList.vue`（新增）、`src/modules/stack/StackEditDialog.vue`（新增）
- [ ] `StackList.vue` 卡片/表格展示栈名、成员数、最后运行状态，含启动/停止/编辑/删除/导出按钮，调用对应 store action。
- [ ] `StackEditDialog.vue` 左侧分别展示「已装软件」与「Spring Boot 应用」可勾选列表（复用现有查询命令），右侧已选成员支持拖拽排序（`order`）+ 每个成员下拉选择同栈其他成员作 `depends_on`。
- [ ] 保存时调用 `create_stack`/`update_stack`；后端返回环错误时对话框内提示用户修正（不关闭）。
- [ ] 交互与视觉风格复用 `StartupSettingsDialog.vue`。

### T7 — 前端运行面板与状态可视化
**依赖**：T5、T3
**覆盖需求**：R8（P2：R11 栈模板 / R12 自启开关 / R13 启动报告 列为后续预留）
**涉及文件**：`src/modules/stack/StackRunPanel.vue`（新增）
- [ ] 展示栈依赖关系（简洁列表或 Mermaid 有向图）及各成员实时状态徽标（running/starting/failed/stopped）。
- [ ] 启动/停止进度可见，订阅 `stack-status-changed` 实时刷新。
- [ ] 启动失败时清晰展示哪个成员失败、是否因依赖未就绪（R8 / R9 信息透出）。
- [ ] P2 项（R11 栈级自启开关、R12 内置模板、R13 启动报告）在面板预留 UI 位置与 store 字段，但本期不实现逻辑，标注 `TODO(P2)`。

### T8 — 兼容性与回归验证
**依赖**：T3、T4、T7
**覆盖需求**：R6
**涉及文件**：`src-tauri/src/services/software_manager/lifecycle.rs`（复用不变更）、`src/modules/software-manager/components/StartupSettingsDialog.vue`（复用不变更）、`src-tauri/src/lib.rs`（仅新增注册）
- [ ] 现有"应用启动自动拉起"（`auto_start_all`）行为未被破坏；`StartupSettingsDialog` 仍可正常使用（R6）。
- [ ] 既有 `InstalledSoftware.auto_start_on_app_start` + `startup_order` 与 Spring Boot `auto_start`/`depends_on` 字段读写正常。
- [ ] 端到端联调：定义含 MySQL→Redis→Nacos→Spring Boot 的栈，一键启动按依赖顺序逐批拉起；一键停止逆序停止；环配置被拒绝。
- [ ] 无新增 crate / npm 包；`cargo build` 与 `vite build` 均通过。

---

## 需求覆盖矩阵（R1–R13 → 任务）

| 需求 | 优先级 | 归属任务 | 说明 |
| --- | --- | --- | --- |
| R1 数据模型 | P0 | T1 | `Stack`/`StackItem` + serde + `stacks.json` |
| R2 栈 CRUD 命令 | P0 | T2 | 5 个命令 + 注册 |
| R3 一键启动（拓扑+环检测+逐批） | P0 | T3 | 核心编排 |
| R4 一键停止/重启 | P0 | T3 | 逆序停止 / 先停后起 |
| R5 前端栈管理页 | P0 | T5,T6 | 列表 + 编辑对话框 + 按钮 |
| R6 兼容现有能力 | P0 | T5,T8 | 不破坏现有自启与对话框 |
| R7 导出/导入 | P1 | T4 | 栈 JSON 分享 |
| R8 运行可视化 | P1 | T5,T7 | 状态徽标 + 进度 + 事件订阅 |
| R9 依赖就绪预检+回滚 | P1 | T3 | 复用 health_check + 回滚 |
| R10 失败策略（重试） | P1 | T3 | `retry` 字段 + 默认不阻断 |
| R11 栈级自动拉起 | P2 | T7（预留） | 本期预留开关与字段，逻辑后续 |
| R12 栈模板 | P2 | T7（预留） | 内置模板 UI 预留 |
| R13 启动报告 | P2 | T7（预留） | 耗时/结果摘要预留 |

---

## 验收总览（Definition of Done）

- [ ] 全部 P0（R1–R6）完成并端到端可用。
- [ ] P1（R7–R10）完成。
- [ ] P2（R11–R13）在前端预留扩展点，逻辑标注 `TODO(P2)`，不影响主流程。
- [ ] `cargo build`（src-tauri）与 `vite build` 通过，无新增依赖、无破坏现有能力。
- [ ] 设计 §6 七个待确认问题决策已全部落地到实现（拓扑序为主、`stacks.json` 独立持久化、复用 health_check、环检测拒绝、并发逐批、栈级事件 `stack-status-changed`）。
