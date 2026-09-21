# 扩展设计 4：启动编排增强（全自动编排 + 报告）

> 状态：已确认设计
> 日期：2026-08-28
> 目标：把 OPX 启动时零散的多路 auto_start 收敛为单一有序启动编排，带失败回滚与可恢复的前端启动报告。

## 现状（已勘察）

OPX 启动时三条自启源**各自并发、无统一顺序、无回滚、无前端反馈**：

| 自启源 | 位置 | 现行为 |
|---|---|---|
| Node 应用 | `lib.rs` setup 内 `node_mgr.auto_start_all` | 同步阻塞，按 `startup_order` |
| 软件 | `lib.rs` spawn `lifecycle::auto_start_all` | 后台，按 `startup_order` |
| 服务组 Stack | `lib.rs` spawn `stack_mgr.auto_start_all` | 后台，按 `updated_at` |

失败仅打日志；Stack 已有 `last_run_report` 可参考结构。

> 注：OPX 自身「开机自启」已完整实现（`config.rs::get_autostart/set_autostart`，winreg 写注册表 Run 键，设置页开关），本扩展不重复做。

## 已确认决策

1. **全自动编排 + 报告**：收敛为单一启动序列，依次拉起，前端展示实时进度与持久化报告。
2. **失败回滚已拉起项**：仅应用于开机统一编排语境。
3. **回滚边界按语境区分**：
   - 开机统一启动编排：某项失败 → 逆序停止本次已成功拉起项（回滚）。
   - 手动点启动软件拉依赖（扩展1）：失败**不**回滚（共享依赖不误杀）。两语境规则不冲突，合理共存。

## 设计

- **统一启动协调器** `startup_bootstrap.rs`：
  - 汇总三类 auto_start 目标，合并为带顺序的启动项列表（软件↑`startup_order`、Node↑`startup_order`、Stack↑`updated_at`）。
  - 按序执行，每项实时推 `startup-progress` 事件；成功项收集到报告。
  - 任一项失败 → 对本次**已成功拉起**项按逆序停止（回滚），报告标记失败项与原因。
  - 产出持久化 `StartupReport`（对齐 Stack `last_run_report` 结构）。
- **替换** `lib.rs` 三段分散自启 spawn → 一次调用协调器。
- **命令/事件**：`get_last_startup_report`；事件 `startup-progress{ item, status }`。
- **前端**：Dashboard 或设置页新增「最近一次启动报告」卡片：逐项状态 / 耗时 / 失败原因，失败聚焦。

## 测试

- 协调器单测：目标排序正确；回滚逆序正确；失败项回滚已成功项（注入 fake 启停）。
- `cargo test --lib`；`npx vue-tsc --noEmit`。

## 边界

- 回滚仅作用于**本次统一启动序列拉起的**项，不触碰用户此前已独立启动的服务。

## 改动文件清单

- `startup_bootstrap.rs`（新，统一协调器）
- `lib.rs`（替换三段分散 spawn）
- `commands/`（`get_last_startup_report`）
- 前端 Dashboard/设置页（启动报告卡片）
- locales
