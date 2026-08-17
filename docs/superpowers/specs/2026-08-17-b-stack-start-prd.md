# B 扩展（一键启动栈 Stack）PRD

> 文档类型：简单 PRD（PRD - Simple）
> 分支：`feat/b-stack-start`
> 版本：v0.1（初稿）

## 0. 项目信息

- **Language（文档语言）**：中文
- **Programming Language（技术栈）**：前端 Vite + React + MUI + Tailwind CSS；后端沿用现有 Rust + Tauri（不引入新框架）
- **Project Name**：`b_stack_start`
- **原始需求复述**：新增「B 扩展（一键启动栈）」能力——在 OPX 已有的自动启动 / 启动顺序原语之上，提供用户可**定义、保存、一键执行**的跨模块服务栈（Stack），支持显式启动顺序与依赖配置，并支持一键启动 / 停止 / 重启整套关联服务（如 数据库 → 中间件 → 应用）。

---

## 1. 现有能力调研结论（事实依据，已通过 Grep/Read 核实）

> 结论：**仓库中「自动启动配置」已真实存在**，并非从零新增。B 扩展是在现有原语之上补齐「栈（Stack）抽象 + 一键编排」能力。

| 能力 | 证据（文件:行） | 现状 |
| --- | --- | --- |
| 已装软件自动启动开关 + 启动顺序 | `models/software.rs:111-112` 字段 `auto_start_on_app_start: bool`、`startup_order: u32` | ✅ 已存在 |
| 启动设置持久化 / 读取 | `services/software_manager/mod.rs:290-325` `update_startup_settings()`、`list_auto_start()`（按 startup_order 升序） | ✅ 已存在 |
| 应用启动自动拉起编排 | `services/software_manager/lifecycle.rs:511-557` `auto_start_all()`：按 startup_order 分组、批次间 sleep 500ms 拉起 | ✅ 已存在，但**仅按时间盲等、不等待依赖就绪** |
| 前端启动设置 UI | `modules/software-manager/components/StartupSettingsDialog.vue`、`models/software.ts:81`、`i18n autoStartOnAppStart` | ✅ 已存在（单实例开关） |
| Tauri 命令 | `commands/software.rs:1219` `save_startup_settings`；`commands/config.rs:49-58` `get/set_autostart`（OPX 自身开机自启，独立于单软件） | ✅ 已存在 |
| Spring Boot 自动启动 + 依赖 | `models/springboot.rs:39` `auto_start: bool`、`:51` `depends_on: Vec<String>`；`models/springboot.ts:26,37`；`list_springboot_dependency_candidates` 命令 | ✅ 字段已存在；**但 grep 未发现 `depends_on` 在启动编排中被消费**（仅存储） |
| **「栈 / Stack」抽象（命名分组、一键整体编排、跨模块）** | grep `stack` / `group_start` / `batch_start` / `one_click` → **无匹配**；`profile` 匹配均为 Spring `--spring.profiles.active` | ❌ **不存在** |
| `providers/mod.rs` 中的自动启动字段 | grep 无匹配 | ❌ 无（自动启动字段定义在 `InstalledSoftware` 模型，而非 provider） |

**调研结论一句话**：单实例的「应用启动自动拉起 + 启动顺序」已具备；Spring Boot 额外有 `depends_on` 字段（未确认是否在编排中生效）；但**跨模块、可命名、可保存、一键执行的「服务栈」概念完全缺失**——这正是 B 扩展要补齐的。

---

## 2. 产品目标

1. **一键拉起整套环境**：让用户用一次点击按顺序启动一组关联服务（如 MySQL → Redis → Nacos → 自己的 Spring Boot 应用），替代逐个人工启停与手动排序，降低出错率。
2. **跨模块统一编排**：在已有 `auto_start_on_app_start` + `startup_order`（已装软件）与 `auto_start` + `depends_on`（Spring Boot）原语之上，提供跨两大模块的「栈」编排，支持显式顺序与依赖关系。
3. **可复用、可分享的栈配置**：用户可定义并保存「栈」，导出/分享给团队，实现环境一致、开箱即用，减少重复搭建成本。

---

## 3. 用户故事

- **作为运维人员**，我希望定义一个包含 MySQL、Redis、Nacos 与我的 Spring Boot 应用的栈，以便一次点击即可按依赖顺序拉起整套联调环境。
- **作为开发者**，我希望一键停止 / 重启整个栈，以便快速重置本地环境，而不必逐个操作服务。
- **作为团队负责人**，我希望把栈配置导出并分享给同事，以便团队成员环境一致、减少"在我机器上能跑"问题。
- **作为普通用户**，我希望打开 OPX 时能自动拉起我标记为"栈自启"的服务（沿用现有 startup_order 语义），以便打开即可投入工作。
- **作为排障人员**，我希望在启动失败时能清晰看到哪个成员失败、是否因依赖未就绪，以便快速定位问题。

---

## 4. 需求池（P0 / P1 / P2）

### P0（Must have）

- **R1 数据模型**：新增 `Stack` 实体，字段含 `id`、`name`、`description`、`items: [{ ref_type: "software"|"springboot", ref_id: String, order: u32, depends_on: Vec<String> }]`、`created_at`。持久化到 `data_dir/stacks.json`（沿用现有 JSON 存储风格）。
- **R2 栈 CRUD 命令**：后端实现 `list_stacks` / `create_stack` / `update_stack` / `delete_stack` / `get_stack`，并注册到 `lib.rs`。
- **R3 一键启动（核心）**：`start_stack(stack_id)`——先对 `items` 做拓扑排序（依据 `depends_on` + `order`），**检测环并拒绝**；按执行计划**逐批启动**，依赖项就绪（健康检查通过）后再启动下游，**取代现有盲目 500ms 间隔**。
- **R4 一键停止 / 重启**：`stop_stack(stack_id)`（逆序优雅停止）、`restart_stack(stack_id)`（先停后起）。
- **R5 前端「栈管理」页**：列表 + 创建/编辑对话框（勾选「已装软件」与「Spring Boot 应用」、设置 `order` 与 `depends_on`）+ 每个栈的「启动 / 停止 / 重启」按钮。
- **R6 兼容现有能力**：复用 / 桥接现有 `InstalledSoftware.auto_start_on_app_start` + `startup_order` 与 Spring Boot `auto_start`/`depends_on`，**不破坏**现有自动启动行为与 `StartupSettingsDialog`。

### P1（Should have）

- **R7 导出 / 导入**：`export_stack` / `import_stack`（栈 JSON 文件，便于团队分享）。
- **R8 运行可视化**：栈详情页展示每个成员的实时状态（running / starting / failed）与整体进度。
- **R9 依赖就绪预检**：启动某成员前先探测其依赖（如数据库端口可达 / 健康检查通过）再拉起；若上游失败则**自动回滚已启动成员**并报告。
- **R10 失败策略**：单成员启动失败可重试；默认不阻断整栈，除非其 `depends_on` 上游失败。

### P2（Nice to have）

- **R11 栈级自动拉起**：提供栈的"应用启动自动拉起"开关（继承现有 `auto_start` 语义，按栈 `order` 编排进 `auto_start_all`）。
- **R12 栈模板**：内置快捷栈（如"微服务基座""经典 LAMP"）一键套用。
- **R13 启动报告**：启动耗时统计与最近一次启动结果摘要。

---

## 5. UI 设计稿

- **导航入口**：侧边栏新增「栈 / Stacks」，路由 `/stacks`（沿用现有 `Sidebar.vue` + `router/index.ts` 风格）。
- **列表页**：卡片 / 表格展示栈名、成员数、最后运行状态、操作（启动 / 停止 / 编辑 / 删除 / 导出）。
- **创建 / 编辑对话框**：
  - 左侧：可勾选列表，分别展示「已装软件」（`InstalledSoftware`）与「Spring Boot 应用」；
  - 右侧：已选成员列表，支持**拖拽排序**（写入 `order`）+ 每个成员可下拉选择同栈其他成员作为 `depends_on`；
  - 复用现有 `StartupSettingsDialog.vue` 的交互与 MUI/Tailwind 视觉风格。
- **栈运行面板**：展示有向依赖图（Mermaid 或简洁列表）及各成员实时状态徽标，启动 / 停止进度可见。
- **状态事件**：聚合现有 `software-status-changed` 与 `springboot-status-changed` 事件为栈级状态，前端订阅刷新。

---

## 6. 待确认问题（Open Questions）

1. **Spring Boot `depends_on` 是否已生效？** grep 未发现该字段在启动编排中被消费。B 的栈依赖应**自建拓扑排序**，还是直接复用 Spring Boot `depends_on`？建议自建、并以引用方式桥接。
2. **已装软件"就绪"判定信号？** 是否复用现有 `services/software_manager/health_check.rs`？需确认健康检查接口对 MySQL/Redis 等的可用性。
3. **顺序 vs 依赖双维度冲突？** 栈定义同时拥有 `order`（u32）与 `depends_on`，二者可能冲突——是否以 `depends_on` 拓扑序为主、`order` 仅作同层 tie-break？
4. **持久化位置？** 新建 `stacks.json`，还是并入现有 `installed.json` / Spring Boot `apps.json`？需与 `SoftwareManager` / springboot store 协调。
5. **与现有"应用启动自动拉起"如何共存？** 是否提供栈级自动拉起开关（即 P2 R11）？若提供，如何并入 `auto_start_all`？
6. **跨模块状态聚合？** 统一 `software-status-changed` / `springboot-status-changed` 为栈级事件的机制与前端的订阅方式？
7. **环检测与并发度？** 拓扑排序发现环时如何提示用户修正？同层无依赖成员是否允许并发启动（提升速度）？
