# OPX 后续开发计划（Roadmap 2）

> 分支：`feat/roadmap-2`
> 日期：2026-08-18
> 前序：extensions-design.md（7 项已合并 dev）。本计划为第二批扩展，按依赖/价值排序实施。

## 优先级排序（按价值 × 复用度 × 成本）

| 序号 | 功能 | 模块 | 说明 |
|---|---|---|---|
| R1 | **定时自动备份** | 软件管理/备份 | 按计划自动快照 + 保留策略；复用现有 backup.rs |
| R2 | **软件升级检测** | 软件仓库 | 已装版本 vs catalog latest 对比，一键升级 |
| R3 | **全局配置模板/预设** | 软件管理 | MySQL/Redis 等通用参数预设一键应用 |
| R4 | **SpringBoot 快速重启** | SpringBoot | 改 jar 后一键 reload + JVM 参数模板 |
| R5 | **进程级资源监控** | 系统监控 | 每已装软件实时 CPU/内存占用图表 + 告警 |
| R6 | **网站证书/代理增强** | 网站管理 | 反向代理可视化、HTTPS 证书管理 |
| R7 | **托盘增强** | 全局 | 托盘显示运行服务数、一键启停 |
| R8 | **批量安装** | 软件仓库 | 勾选多个软件批量安装 |

---

## R1 定时自动备份（首批实施）

### 现状
`backup.rs` 已支持手动快照（StopAndBackup/Hot），含滚动保留（最多 5 个）、zip-slip 防护。当前无"定时"触发。

### 方案
- 新增 `backup_schedule` 配置（每个已装软件可选）：`{ enabled: bool, cron: "daily"|"weekly"|"interval_minutes": number }`
- 后端调度器：`tokio` 定时遍历启用了计划备份的实例，执行 `create_snapshot`（Hot 模式，避免停机）
- 持久化到 installed 记录或独立 schedules 文件；应用启动时用 `auto_start_all` 类似模式挂后台任务
- 前端：BackupRestoreDialog 增「定时备份」配置入口（启用开关 + 频率选择）

### 改动文件
- `src-tauri/src/models/software.rs`（BackupSchedule 字段）
- 新 `services/software_manager/backup_scheduler.rs`（调度循环）
- `src-tauri/src/lib.rs`（启动挂调度）
- 前端 `BackupRestoreDialog.vue`、`stores/ops.ts`、locales

### 验收
设置某实例「每天备份」→ 缩短间隔为 1 分钟 → 观察自动生成快照；保留策略仍生效（≤5）。

---

## R2 软件升级检测

暂缓（需 catalog 网络刷新与版本比较，依赖远程镜像，待离线环境策略明确）。

---

## R3 全局配置模板

### 方案
内置常见配置预设（MySQL performance 参数、Redis 内存策略等），在前端 `ConfigEditDialog` 提供「应用预设」下拉，一键填充常见推荐配置。

---

## R4 SpringBoot 快速重启

### 方案
`AppCard` 增加「快速重启」：复用现有 restart 逻辑，追加 `--spring.devtools.restart.enabled=true` 或直接重启进程；JVM 参数模板预置常用 GC/内存参数。

---

## R5 进程级资源监控

### 方案
用 `sysinfo` 按已装软件 pid 采样 CPU/内存，前端 TrendChart 展示；阈值告警（超阈值弹提示 / 日志）。

---

## R6 网站证书/代理增强

### 方案
网站管理补反向代理规则渲染（基于 nginx conf 解析）与 HTTPS 证书配置引导。

---

## R7 托盘增强

### 方案
托盘图标右键菜单列出运行中软件，一键启停；托盘 tooltip 显示运行数。

---

## R8 批量安装

### 方案
仓库页多选 catalog 条目，批量推入安装队列（复用 install_software，逐条排队）。

---

## 实施顺序
R1 → R3 → R4 → R5 → R7 → R8 → R6（R2 待离线策略明确后补）。

## 验证流程（每项）
1. `cargo test --lib`（相关模块）+ `npx vue-tsc --noEmit`
2. `npm run tauri:dev` GUI 走查
3. 合并 dev，推送
