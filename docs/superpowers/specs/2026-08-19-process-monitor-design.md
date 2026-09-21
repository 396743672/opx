# R5 进程级资源监控 设计

> 分支：`feat/roadmap2-r5`（从 dev 派生）
> 日期：2026-08-19
> 前序：roadmap-2-design.md。R1/R3/R4 已合并 dev。本设计为 R5：进程级资源监控。

## 现状

- 系统监控已具备整机采样：前端 `system store` 每 1s 调 `system_info`，内存环形 120 点 `HistoryPoint`，`TrendChart` 画整机 CPU/内存。
- 已装软件 `InstalledSoftware.pid`、SpringBoot 应用 `SpringBootApp.pid` 均有运行时 pid，可据此用 sysinfo 采样**单进程** OS 级 CPU/内存。
- `sysinfo = "0.31.0"` 已依赖；`lifecycle.rs::is_pid_alive` 已示范 `refresh_processes(ProcessesToUpdate::All)` + `process(Pid)` 用法。
- 单进程 CPU% 需连续两次 refresh 以进程 CPU 时间差 / 墙钟差计算，故须跨调用缓存 `System` 实例。
- `system_history.json` 后端命令读一个无写入方的文件——遗留死代码，本设计**不触碰**。

## 目标

系统监控页新增「进程资源监控」：列出所有运行中进程（已装软件 + SpringBoot 应用）的实时 CPU%/内存，点某进程行内展开该进程的 CPU/内存趋势图；CPU/内存超固定阈值时弹 toast 告警。

## 方案

### 后端（src-tauri）

1. **新增 `src-tauri/src/services/software_manager/process_monitor.rs`**：
   - `pub struct ProcessSample { pub pid: u32, pub cpu_usage: f64, pub mem_bytes: u64 }`
   - `pub fn sample_processes(pids: &[u32]) -> Vec<ProcessSample>`：
     - 模块级 `static PROCESS_SYS: Lazy<Mutex<System>>` 缓存 System，跨调用保持进程 CPU 时间基准。
     - `refresh_processes(ProcessesToUpdate::All)` 后对每个 pid 调 `process(Pid::from_u32(pid))`：
       - 进程存在 → `cpu_usage = p.cpu_usage()`（sysinfo 已算出增量 %）、`mem_bytes = p.memory()`。
       - 进程不存在（已退出）→ 跳过（不返回该 pid）。
   - 纯函数、无状态外部依赖，可单测。
2. **新增命令 `sample_process_resources(pids: Vec<u32>) -> Vec<ProcessSample>`**，注册 `lib.rs`。
3. 单测：`sample_processes` 对不存在的 pid 返回空；对自身 PID（当前进程）能采样出非负 CPU/内存（不崩溃）。抽 `collect` 纯逻辑便于测试。

### 前端

1. **`src/models/process.ts`**：`ProcessSample { pid, cpu_usage, mem_bytes }`。
2. **系统监控页新增「进程资源监控」区块**（插在「服务与应用概览」之后、信息行之前）：
   - 运行中进程列表 = `installedSoftware.filter(Running)`（已装软件）+ `runningApps`（SpringBoot），各自带 `pid`；仅保留 `pid != null` 的。
   - 扩展一个 `processSamples: Map<pid, HistoryPoint[]>`（复用 `HistoryPoint { timestamp, cpu_usage, memory_usage }`，用 memory_usage 存内存百分比或直接另存字节；推荐存字节并单独展示，趋势图内存用百分比需归一化——见边界）。
   - 复用系统监控页既有 1s 轮询节奏：在现有 `fetchAll` 顺带采样进程（与整机采样同频，避免多定时器）。
   - 表格：名称（类型图标区分软件/SpringBoot）、PID、CPU%、内存、操作「查看」。
   - 点「查看」行内展开该进程的 CPU/内存趋势图（复用 `TrendChart`，`:points="该pid的历史"`，metric cpu/memory 各一个）。
3. **阈值告警**（前端采样后判断，固定阈值）：
   - CPU ≥ 90% 或 内存 ≥ 90% → `toast(t('processAlert', { name, metric, value }), 'err')`。
   - 去重：同一进程同一指标连续超限只提示一次（维护 `alertedSet: Set<pid:metric>`，恢复阈值下后移除，再超再提示）。

### 复用点

- `TrendChart`、`toast`、系统监控页既有轮询时钟、`formatBytes`。

## 边界

- 内存趋势图：进程内存是绝对值（字节），`TrendChart` 的 y 轴按百分比 `max=100%` 渲染。若直接画字节会超出刻度。决策：**内存趋势图存字节但缩放**——将 mem_bytes 归一化为「占整机内存百分比」或「占该进程历史峰值百分比」。采纳前者（占整机内存百分比），与整机 memory_usage 同量纲、y 轴 0-100% 自洽；表中仍显示原始字节（formatBytes）。
- 进程退出（pid 失效）：采样返回空，该行从实时表格移除（或置 0）；趋势历史保留到超 MAX_HISTORY 自然淘汰。
- 告警为前端判定（跟随前端内存采样决策），不落审计日志。
- 本功能只读采样，不启停/改进程。

## 改动文件

- 新增 `src-tauri/src/services/software_manager/process_monitor.rs`
- 新增 `src-tauri/src/services/software_manager/mod.rs`（导出 mod，如需）
- `src-tauri/src/commands/software.rs`（或新增命令文件）——命令 `sample_process_resources`
- `src-tauri/src/lib.rs`（注册命令）
- 新增 `src/models/process.ts`
- `src/modules/system-monitor/pages/DashboardPage.vue`（进程资源区块 + 采样 + 告警 + 展开）
- `src/stores/system.ts`（可选：进程采样面移至 store 统一管理）
- `src/locales/zh-CN.ts` / `en-US.ts`（i18n key）

## 测试

- `process_monitor::sample_processes` 单测：不存在 pid 返回空；自身 PID 采样非负不崩溃。
- 验证命令：`cargo test --lib` + `npx vue-tsc --noEmit`。

## 验收

1. 系统监控页新增「进程资源监控」区块，列出运行中的 MySQL/Redis/Nginx 等已装软件与 SpringBoot 应用及其 PID、实时 CPU%、内存。
2. 进程 CPU% 随实际运行变化（MySQL 空闲近 0、满载高）。
3. 点某进程「查看」行内展开其 CPU/内存趋势图，随轮询实时更新。
4. CPU 或内存超 90% 时弹 toast 告警，恢复后再次超限能再次提示（去重正确）。
5. 进程停止后其行消失，不报错。
