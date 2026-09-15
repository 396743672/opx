# 首页瘦身 + 新建「监控中心」设计

> 状态：已确认设计
> 日期：2026-09-15
> 范围：纯前端布局调整，不动后端命令与数据流

## 背景与现状

首页（`/dashboard`，`DashboardPage.vue`，520 行）定位是「系统监控」，但混进了大量软件管理内容，共 8 个区块：

| # | 区块 | 归属 |
|---|---|---|
| 1 | 4 张指标卡（CPU/内存/磁盘/网络） | 监控 |
| 2 | 趋势图 ×2（CPU/内存） | 监控 |
| 3 | 服务组概览 | 编排 |
| 4 | 最近一次启动报告 | 编排 |
| 5 | 进程资源监控（运行中软件/应用的 CPU/内存 + 行内曲线） | 监控 |
| 6 | 已装软件 / 运行中服务 / 运行中应用 三列列表 | 软件管理 |
| 7 | 系统信息 / 磁盘 / 网络 三列 | 监控 |

问题：**#5 与 #6 的「运行中服务/运行中应用」是同一批数据的两种展示**（#5 信息更多）；**#6 的「已装软件」是软件管理页的全量列表**，与监控主题无关。首页因此又长又杂。

数据流现状：`processRows` 由 `runningSoftware + runningApps + latestSamples` 推导；`runningSoftware` 由 `list_installed_software` + `lifecycleStore` 状态覆盖得出；`runningApps` 由 SpringBoot store + `springboot-status-changed` 事件维护。

## 已确认决策

1. **首页回归纯整机监控**：指标卡 + 趋势图 + 系统信息/磁盘/网络。
2. **进程资源监控搬到新菜单「监控中心」**（侧边栏 monitor 组），后续可扩展其他监控类型。
3. **运行中服务/应用保留在首页，折叠**，默认收起。
4. **移除**：服务组概览、最近一次启动报告、已装软件全量列表。
5. 监控中心顶部**加 3 张 KPI 卡**，作为后续新监控的落位。
6. 设计沿用项目现有 token 与组件（`StatCard` / `CardHeader` / `TrendChart` / `ProgressBar`），**不引入新配色或字体**（一致性优先）。

## 设计

### 1. 共用逻辑抽出：`src/composables/useRunningSoftware.ts`（新）

两个页面都要「运行中的软件与应用」，且都依赖定时刷新与事件监听。抽 composable，内部用 `onMounted` / `onUnmounted` 自管生命周期：

```ts
/// 运行中软件与应用（含 installedSoftware 全量，供筛选运行态）
export function useRunningSoftware() {
  const installedSoftware = ref<InstalledSoftware[]>([])
  const runningApps = ref<SpringBootApp[]>([])
  // ...
  return { installedSoftware, runningSoftware, runningApps }
}
```

内部职责（原 DashboardPage 的对应逻辑原样搬入）：
- `list_installed_software` 拉取 + `lifecycleStore.getStatus` 覆盖状态 → `runningSoftware`
- `sbStore.fetchApps()` + `listen('springboot-status-changed')` → `runningApps`
- `onUnmounted` 注销监听

**不进 composable**（只有监控中心需要）：1s 采样定时器、`latestSamples`、`processRows`、`procHistory`、`expandedPids`。

### 2. 首页 `system-monitor` 改后结构

```
PageHeader  系统监控  (hostname)
├─ 4 张指标卡（CPU / 内存 / 磁盘 / 网络）              ← 不变
├─ 趋势图 ×2（CPU / 内存）                            ← 不变
├─ 三列：系统信息 / 磁盘 / 网络                        ← 不变
└─ ▸ 运行中服务与应用 (N)                             ← 新增，默认收起
   ├─ 运行中服务：名称 + 版本
   └─ 运行中应用：名称 + 端口
```

- 折叠块**放最后**：前三块为纯监控主体，软件内容压尾。
- 标题行显示条目总数 `runningSoftware.length + runningApps.length`，收起时也能看到规模。
- 交互沿用项目已有的「按钮 + ref 集合」模式（非 `<details>`），与进程行展开一致。
- 空态：无运行中项时标题行显示 `(0)`，展开后显示「暂无运行中的服务或应用」。
- **移除**：服务组概览区块、启动报告区块、已装软件列；以及它们的取数（`stackStore.loadStacks/subscribe`、`get_last_startup_report`、`startup-progress` / `startup-completed` 监听、`upsertStartupItem`、`kindIcon`、`formatStartTime`、`stackOverview`）。

### 3. 新页 `src/modules/monitor/pages/MonitorCenterPage.vue`（新）

路由 `/monitor`，侧边栏 monitor 组第 2 项，图标 `mdi:chart-timeline-variant`，标题键 `monitorCenter`。

```
PageHeader  监控中心                        [搜索框] [刷新]
├─ 3 张 KPI：
│    运行中进程 (N)  |  CPU 告警中 (N)  |  内存告警中 (N)
├─ 告警机制提示行（原文案 alertHintLine，含「无运行实例」提示）
└─ 进程列表（紧凑行）
   ├─ 行：类型图标 · 名称 (pid) · CPU x% · 内存 xx · [查看/收起]
   └─ 展开：CPU / 内存 双曲线（TrendChart，高 120）
```

- **KPI 口径**：
  - 运行中进程 = `processRows.length`
  - CPU 告警中 = `processRows.filter(r => r.cpu >= alert_process_cpu).length`（阈值实时取 `settingsStore.settings`，与后端告警阈值同源）
  - 内存告警中 = `processRows.filter(r => r.memPct >= alert_process_mem).length`
  - 告警中的 KPI 卡数字用 `text-destructive`；为 0 时用默认色（a11y：数值本身即语义，不单靠颜色）
- **超阈值标识**：CPU/内存值超阈值时 `text-destructive` + 前置 `mdi:alert` 小图标（当前只标红字，违反「不要只靠颜色传达信息」）。
- **搜索框**：按名称或 pid 子串过滤（大小写不敏感），过滤只影响列表展示，不影响 KPI 计数。空结果时显示「无匹配进程」。
- 数据：`useRunningSoftware()` + 本页自有的采样定时器（1s）、`processRows`、`procHistory`（30s 刷新 `process_metrics_history`）、`expandedPids`。
- 刷新按钮 = 立即重拉 `process_metrics_history`。

### 4. 侧边栏与路由

```ts
// Sidebar.vue monitor 组
{ path: '/monitor', titleKey: 'monitorCenter', icon: 'mdi:chart-timeline-variant' }
```
```ts
// router/index.ts
{ path: '/monitor', name: 'monitor', component: () => import('@/modules/monitor/pages/MonitorCenterPage.vue'), meta: { title: 'monitorCenter' } }
```

### 5. i18n

`zh-CN.ts` / `en-US.ts` 新增（两文件同键同义）：

| key | zh-CN | en-US |
|---|---|---|
| `monitorCenter` | 监控中心 | Monitor Center |
| `runningServicesAndApps` | 运行中服务与应用 | Running services & apps |
| `searchProcessPlaceholder` | 搜索名称或 PID | Search name or PID |
| `noMatchingProcess` | 无匹配进程 | No matching process |
| `kpiRunningProcesses` | 运行中进程 | Running processes |
| `kpiCpuAlerting` | CPU 告警中 | CPU alerting |
| `kpiMemAlerting` | 内存告警中 | Memory alerting |
| `noRunningServicesOrApps` | 暂无运行中的服务或应用 | No running services or apps |

复用现有键：`runningServices`、`runningApps`、`processMonitor`、`alertHintLine`、`noRunningProcess`、`view`、`collapse`、`cpuUsage`、`memoryUsage`。

### 6. 测试

- `npx vue-tsc --noEmit` 通过；`npm run build` 通过。
- 实机验证清单：
  1. 首页只剩监控区块；折叠块标题显示正确数量，默认收起，展开后两个列表内容正确
  2. 首页刷新后「运行中服务/应用」随启停实时变化（事件监听仍生效）
  3. 监控中心 KPI 三个数字与实际运行进程一致；停掉一个软件后数字下降
  4. 阈值压低（如 CPU 阈值设为 1）→ 列表出现红字 + 警示图标，CPU 告警中 KPI 计数上升
  5. 搜索框按名称与 PID 过滤正确；清空后恢复
  6. 进程行展开曲线正常（CPU / 内存双图）
  7. 离开监控中心再回来，定时器不叠加（无重复采样、无内存增长）
  8. 侧边栏「监控」组显示两项，切页高亮正确
  9. 中英文切换两页文案齐全

## 边界（不做）

- 不改后端命令与事件（`sample_process_resources` / `process_metrics_history` / `system_history` / `springboot-status-changed` 全部复用）。
- 不做监控中心的其他监控类型（磁盘 IO、网络连接、端口图谱等）——本期只搭好落位。
- 不做首页自定义布局/拖拽排序。
- 不做进程列表排序切换（保持后端返回顺序）。
- 不删除后端启动报告与栈相关命令（`get_last_startup_report` 等仍被其他入口使用）。

## 改动文件清单

- `src/composables/useRunningSoftware.ts`（新）
- `src/modules/system-monitor/pages/DashboardPage.vue`（瘦身 + 折叠块）
- `src/modules/monitor/pages/MonitorCenterPage.vue`（新）
- `src/router/index.ts`、`src/layouts/Sidebar.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
