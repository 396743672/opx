# 扩展设计 2：定时备份可视化

> 状态：已确认设计
> 日期：2026-08-28
> 目标：把已实现的后端定时备份调度器补上前端可视化配置入口。

## 现状勘察（已确认）

- 后端调度器**已完整实现**（`backup_scheduler.rs`）：分钟间隔、`backup_schedules.json` 持久化、每 60s 检查到期、Hot 模式备份、距离最新快照判到期，含单元测试。
- 命令已暴露：`set_backup_schedule` / `get_backup_schedule`。
- 前端 store 已接：`ops.ts::set_backup_schedule` / `get_backup_schedule`。
- **缺 UI 入口** ← 本次核心工作。
- 保留策略：硬编码滚动保留 5 个（`MAX_SNAPSHOTS_PER_INSTANCE`），不动。

## 已确认决策

1. **调度粒度档位化分钟** — 沿用现有分钟间隔模型，前端提供档位（映射分钟值），零后端改动。
2. 最小改动，复用现有后端与 store。

## 前端设计

- 入口：备份/恢复对话框（`BackupRestoreDialog.vue`）新增「定时备份」区块：
  - 启用开关 + 频率下拉选档位：关闭 / 每 30 分钟(30) / 每 60 分钟(60) / 每天(1440) / 每周(10080)。
  - 载入时 `get_backup_schedule` 回显当前值；保存时 `set_backup_schedule`。
- 反馈：显示当前生效间隔；可基于最近快照时间 + 间隔推演下次提示（从 `list_snapshots` 取最新，前端推算）。
- locales 文案。

## 后端改动

无必要改动。保留 `set_schedule(installed_id, minutes)` 接口，前端映射档位→分钟。

## 测试

- 后端已有测试覆盖调度判定；无新增。
- 前端 `npx vue-tsc --noEmit`。
- 手动验证：设「每 30 分钟」→ 观察 60s 检查触发（可临时调 `CHECK_INTERVAL_SECS` 加速验证）。
- 设档位后重启应用 → 配置仍生效（`backup_schedules.json` 持久化）。

## 改动文件清单

- `src/modules/software-manager/components/BackupRestoreDialog.vue`（定时备份区块）
- locales
