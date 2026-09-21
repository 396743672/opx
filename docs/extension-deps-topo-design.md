# 扩展设计 1：软件级依赖拓扑

> 状态：已确认设计
> 日期：2026-08-28
> 目标：给每个已装软件（`InstalledSoftware`）增加依赖声明，启动时按拓扑序自动拉起未运行的依赖。

## 已确认决策

1. **软件级依赖** — 不在既有 Stack 编排能力上扩展，而是给 `InstalledSoftware` 增加 `depends_on`（引用其他已装软件 id）。
2. **依赖语义「必须运行」** — 启动 A 时，若依赖的 B 已安装但未运行，先自动启动 B；B 已运行则跳过。
3. **配置入口** — 软件管理页实例卡片上编辑依赖（多选已装软件，排除自身）。
4. **停止语义** — 只停自身，不级联回收依赖（依赖可能被多软件共享，不擅自停）。

## 数据模型

`src-tauri/src/models/software.rs` 的 `InstalledSoftware` 增加字段：

```rust
/// 依赖的其他已装软件 id（启动时按拓扑序自动拉起，须处于运行态）
#[serde(default)]
pub depends_on: Vec<String>,
```

前端 `src/models/software.ts` 同步为可选字段。

## 复用拓扑算法

现有 `StackManager::compute_plan` 的 Kahn 分层 + 环检测逻辑抽出为共享工具函数
`src-tauri/src/utils/topo.rs`，Stack 与软件级依赖共同复用：

- `topological_layers(nodes, /* id -> deps */) -> (Vec<Vec<String>>, Option<Vec<String>> /* cycle */)`

顶层计划结构对齐 `StackStartPlan`：`SoftwareStartPlan { layers, cycle }`。

## 启动编排行为

给定目标软件 id：

1. 解析**依赖闭包**（含传递依赖），去重、忽略自引用。
2. 校验：任一依赖未安装 → 报错并列出缺失项；环 → 报错并返回环路径。
3. 生成分层计划，`layers[0]` 为第一批。
4. 逐层启动未运行的依赖，批内并行；已运行跳过。
5. 依赖全部就绪后启动目标自身。
6. 任一依赖启动失败 → 中止本次启动链；**已拉起的依赖保留运行，不回滚**。

## 持久化与命令

- `InstalledSoftware.depends_on` 随现有 installed.json 往返理持久化。
- 命令：
  - 现有的 `start_software` 改走依赖编排（先 resolve+deps 再启自身）。
  - 新增 `update_software_deps { id, depends_on }` 写依赖。
  - 新增 `resolve_deps { id }` 返回分层计划 + 环/缺失校验，供前端依赖图/启动预览。

## 前端

- 软件实例卡片新增「依赖」编辑入口：多选已装软件（排除自身）。
- 启动按钮触发依赖拉起，实时显示各依赖启动状态。
- 依赖环 / 未安装 以错误提示反馈。

## 错误处理表

| 场景 | 行为 |
|---|---|
| 依赖成环 | 阻止启动，返回「检测到依赖环：A -> B -> A」 |
| 依赖未安装 | 阻止启动，列出缺失项 |
| 依赖启动失败 | 中止启动链；已拉起依赖保留运行，不回滚 |
| 自引用 / 重复 id | 解析时去重 + 忽略自引用 |
| 目标自身未安装 | 照常报「未安装」 |

## 测试

抽出 `topo.rs` 后加单元测试：

- Kahn 分层正确（例 A→B→C 得 3 层）。
- 链式 / 菱形依赖去重。
- 环检测返回环路径。
- 自引用 / 重复忽略。

回归：`cargo test --lib`；前端 `npx vue-tsc --noEmit`。

## 改动文件清单

- `src-tauri/src/models/software.rs`（depends_on 字段）
- `src-tauri/src/utils/topo.rs`（新，抽出拓扑算法）
- `src-tauri/src/services/software_manager/lifecycle.rs`（启动编排接入依赖拉起）
- `src-tauri/src/commands/software.rs`（新命令 + start 改造）
- `src/models/software.ts`（前端类型）
- 软件管理页组件（依赖编辑 + 启动状态）
- locales
