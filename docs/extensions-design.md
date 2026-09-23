# OPX 扩展功能设计文档

> 分支：`feat/extensions`
> 日期：2026-08-18
> 目标：分批实现 7 项扩展功能，每项遵循「设计 → 实现 → 验收」流程。

## 完成状态

| 功能 | 状态 |
|---|---|
| 1 软件启停防抖 | ✅ 已存在（SoftwareListPage actingStates），验证闭环 |
| 2 外部依赖持久化 | ✅ 已实现（540c728） |
| 3 服务组自启 | ✅ 已实现（1cac184） |
| 4 服务组模板 | ✅ 已实现（9fb5136） |
| 5 日志增强（正则高亮） | ✅ 已实现（16a4051）；时间过滤（`fromAt`/`toAt`）、导出合并（`exportCombined`）亦已完成（2026-09-21 复核） |
| 6 运行面板增强 | ✅ 已实现：状态 / 失败原因 / 依赖 chip；端口链接已完成（`StackRunPanel.openPort`、`SoftwareInstanceRow` 端口 chip） |
| 7 Dashboard 集成 | ✅ 已实现（878e143） |

## 批次划分（成本从低到高）

| 批次 | 功能 | 说明 |
|---|---|---|
| 1 | **软件启停防抖** | 根治 rustfs 重复 spawn 多实例残留 |
| 1 | **服务组外部依赖持久化** | 组拉起的组外依赖关系落盘，重启后仍正确停止 |
| 2 | **服务组自启** | 应用启动时按顺序自动拉起指定服务组 |
| 2 | **服务组模板** | 预设模板一键套用，降低配置成本 |
| 3 | **日志增强** | 正则高亮、时间过滤、日志导出合并 |
| 3 | **运行面板增强** | 启动耗时、失败原因、LogViewer 快捷入口、端口链接 |
| 4 | **首页 Dashboard 集成** | 服务组/关键服务运行概览汇总 |

---

## 批次 1

### 功能 1：软件启停防抖

**问题**：软件管理卡片的「启动/停止」按钮无 busy 防护，连续点击会多次 `invoke start_software`，后端每次独立 spawn 一个进程 → 多实例残留、端口冲突。服务组页已加 busy 防护，软件管理页未统一。

**方案**：为软件管理页（SoftwareListPage / SoftwareInstanceRow）引入与 StackList 相同的 per-instance busy 状态：

- `stores/lifecycle.ts`（前端软件状态 store）增加 `actingStates: Record<id, 'start'|'stop'>`（已存在，核实复用）。
- `SoftwareInstanceRow.vue` 按钮绑定 `:disabled` 当 `actingStates[id]` 为非空时置灰；`canStart/canStop` 的 `props.actingStates?.[id] !== 'start'` 已实现。
- 关键是**触发端**（SoftwareListPage 的 onStart/onStop）点按置位 actingStates，invoke 结束后清除（含失败）。

**改动文件**：
- `src/modules/software-manager/pages/SoftwareListPage.vue`（onStart/onStop 加 busy）
- `src/stores/lifecycle.ts`（若 actingStates 未维护，补增）
- `src/modules/software-manager/components/SoftwareInstanceRow.vue`（微调）

**验收**：连续快点击「启动」，同一软件实例只 spawn 一次进程（观察日志无重复 `spawned`）；按钮在编排中显示转圈并禁用。

---

### 功能 2：服务组外部依赖持久化

**问题**：`managed_externals`（组拉起的组外依赖清单）仅存于内存 `StackManagerInner`，应用重启后丢失；重启后再 stop 服务组不会停止之前拉起的组外依赖。

**方案**：
- `Stack` 增加可选字段 `managed_externals: Option<Vec<String>>`（serde default 空，向后兼容旧 stacks.json）。
- `start()` 成功时把本次 `managed_external` 写入该栈记录并 `save()`。
- `stop()` 读取该字段（而非仅内存 map），清空并 `save()`。
- 内存 `managed_externals` map 可移除，改由栈记录承载（单数据源）。

**改动文件**：
- `src-tauri/src/models/stack.rs`（Stack 加字段）
- `src-tauri/src/services/stack_manager.rs`（读写逻辑改为落盘）
- `src/models/stack.ts`（前端类型同步，可选字段）

**验收**：启动含外部依赖的服务组 → 重启应用 → 点停止，组拉起的组外依赖仍被自动停止；未拉起的（用户独立启动的）不误杀。

---

## 批次 2

### 功能 3：服务组自启

利用现有 `stackAutoStart` 占位与 `auto_start_on_app_start` 机制（软件已支持），扩展到服务组级别：

- Stack 增加 `auto_start: bool` 字段（默认 false）。
- 应用启动时（`lib.rs` 初始化后），遍历启用了自启的服务组，按依赖拓扑 `start()`。
- 配置入口：StackEditDialog 增加「应用启动时自启」选项。

**改动文件**：`models/stack.rs`、`stack_manager.rs`（自启遍历）、`lib.rs`（启动钩子）、`StackEditDialog.vue`、locales。

### 功能 4：服务组模板

预设模板（如「联调环境」「微服务」）一键创建：

- 后端 `stack_manager` 增加 `list_templates` / `create_from_template` 命令；模板为内置 JSON 骨架（items + depends_on 骨架 + 占位名称）。
- 前端 StackList「新建服务组」旁增「模板」快捷入口，点击模板 → 用模板 items 预填 StackEditDialog。

**改动文件**：新 `templates.rs`、`commands/stack.rs`、`StackList.vue`、`StackEditDialog.vue`、locales。

---

## 批次 3

### 功能 5：日志增强

在 C 扩展 log_viewer 基础上：

- 正则过滤已有 → 增**匹配高亮**（读取端返回匹配位置或前端二次高亮）。
- 时间范围过滤：基于日志行时间戳前缀过滤（需 provider 日志格式统一或启发式）。
- 日志导出合并：多日志源合并为一个下载文件。

**改动文件**：`log_viewer.rs`、`LogViewerDialog.vue`、`commands/software.rs`、locales。

### 功能 6：运行面板增强

`StackRunPanel` 增强：

- 成员启动耗时（从 Starting→Running 时间差，需事件带时间戳）。
- 失败原因展开显示。
- 成员端口 → 点击用 opener 打开。
- 每成员 LogViewer 快捷入口（复用 C 扩展 read_log）。

**改动文件**：`stack_manager.rs`（事件带时长）、`StackRunPanel.vue`、`models/stack.ts`、locales。

---

## 批次 4

### 功能 7：首页 Dashboard 集成

把服务组/关键服务运行概览汇总到 `DashboardPage.vue`：

- 只读展示：各服务组聚合状态、各组 running 成员数、异常项（failed）。
- 数据来源：复用 `stackManager` 列表 + `softwareManager` 状态，前端聚合。
- 不新增后端接口，仅前端聚合 + 事件刷新。

**改动文件**：`DashboardPage.vue`、可能的 store 辅助。

---

## 验证流程（每批）

1. `npm run tauri:dev` 启动。
2. Rust：`cargo test --lib`（相关模块回归）。
3. 前端：`npx vue-tsc --noEmit`。
4. 按各功能验收清单在 GUI 走查。
