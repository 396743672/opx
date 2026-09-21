# 扩展设计 3：进程端口图谱（卡片嵌入）

> 状态：已确认设计
> 日期：2026-08-28
> 目标：在软件管理页运行中实例卡片展示该软件进程监听的端口，并诊断配置端口冲突。

## 已确认决策

1. **数据源 netstat** — 后端调用 `netstat -ano` 解析「端口 → PID」。无新依赖、Windows 原生、复用现有 `std::process::Command` 先例。
2. **展示形态：卡片嵌入** — 运行中实例卡片展示监听端口。
3. **含端口冲突检测** — 配置端口 vs 实际监听比对，被其他进程占用时红标诊断。

## 数据采集（后端）

- 新增端口解析工具（可并入 `process_monitor.rs` 或新建 `netutils.rs`）：调用 `netstat -ano`，解析 `LISTENING` 行 → `(pid, port, addr)` 列表；解析失败返回空集不崩。
- 新增后端命令 `get_port_map`：返回 `{ pid → [port] }` 全量监听映射。
- 软件 → 进程 pid 映射复用现有 `lifecycle` 已追踪的实例 pid。

## 冲突检测

- 复用现有 `collect_configured_ports(software, provider)` 拿「配置的端口」。
- 比对：配置端口在 netstat 监听集中 → 判断宿主 pid 是否属于本软件：
  - 非本软件 → 「端口冲突：被 PID x（进程名）占用」。
  - 属本软件 → 正常监听。
- 返回 `PortReport { listening: [(port, pid)], configured: [(port, status)], conflicts: [...] }`。

## 前端（卡片嵌入）

- 运行中软件实例卡片新增监听端口展示区：
  - 监听端口 chip，点击可开浏览器（复用栈成员端口快捷入口方式）。
  - 冲突项红标 + 占用者提示。
- 数据在卡片渲染时 `get_port_map` 拉取，或随软件状态快照返回。

## 测试

- `netstat` 解析器单元测试：喂样本输出 → 断言端口/pid 提取 + 畸形行容错。真实命令 GUI 走查。
- `npx vue-tsc --noEmit`。

## 边界

- 仅展示运行中软件；未运行实例不查询。
- 权限不足时 netstat 缺 pid → 降级「端口已监听，宿主未知」。

## 改动文件清单

- `netutils.rs` 或并入 `process_monitor.rs`（netstat 解析）
- `commands/software.rs` 或 `commands/system.rs`（`get_port_map` 命令）
- 软件管理页实例卡片组件（端口展示 + 冲突红标）
- locales
