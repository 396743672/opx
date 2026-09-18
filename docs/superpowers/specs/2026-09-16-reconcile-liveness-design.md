# 启动对账探活修复（reconcile_stale_statuses）

> 状态：已确认设计
> 日期：2026-09-16
> 范围：`SoftwareManager::load_installed_list` 的启动对账逻辑，小修

## 背景

`reconcile_stale_statuses`（`services/software_manager/mod.rs:43`）在每次 `SoftwareManager::new` 时把 `Running/Starting/Stopping/Initializing` **无条件**重置为 `Stopped` 并清 pid，注释假设「应用启动时子进程都不在」。该假设在**应用被强杀**时不成立：子进程（mysqld 等）仍在运行，却被判死——监控中心不显示该软件、重启撞端口（`start_software` 的 PID 残留校验会拒绝）、且只改内存不落盘导致内存与 `installed.json` 不一致。2026-09-15 调试时实际撞上。

## 已确认决策

1. 对账前**探活**：`pid` 存活的 Running 条目保留状态与 pid（收养孤儿）。
2. 瞬态（`Starting/Stopping/Initializing`）无论死活都重置 Stopped——瞬态不该跨启动存活。
3. 对账结果**落盘**，消除内存/磁盘不一致。
4. 探活只查 pid 存在，不校验进程名（pid 复用误收养概率极低，失败模式是停止失败重试即可）。

## 设计

`reconcile_stale_statuses` 改为：

```rust
fn reconcile_stale_statuses(list: &mut InstalledSoftwareList) {
    for s in &mut list.software {
        match s.status {
            SoftwareStatus::Running => {
                // 探活：强杀场景子进程仍在跑，收养（保留 Running 与 pid）
                // ponytail: 只查 pid 存在，不校验进程名——pid 被复用且恰好存活时误收养，
                // 概率极低，表现为停止失败，重试即可
                if s.pid.map(|p| health_check::is_process_alive(p)) != Some(true) {
                    s.status = SoftwareStatus::Stopped;
                    s.pid = None;
                }
            }
            SoftwareStatus::Starting | SoftwareStatus::Stopping | SoftwareStatus::Initializing => {
                s.status = SoftwareStatus::Stopped;
                s.pid = None;
            }
            _ => {}
        }
    }
}
```

`load_installed_list` 在对账后，若有条目被修改则 `save_installed_list` 落盘（探测「是否修改」最简方式：对账函数返回 `bool`）。

复用既有 `health_check::is_process_alive(pid)`（单 pid 刷新，快 10-50 倍）。

## 测试

`mod.rs` 单测（伪 pid 走重置路径——不可能存活的极大值）：
- `Running + 死 pid → Stopped`
- `Starting + 任意 → Stopped`（瞬态无条件重置）
- `Stopped/Error/Unknown → 不动`

「Running + 活 pid 保留」无法确定性单测（需要真实活进程），由实机验证覆盖。

## 实机验证

1. 启动 MySQL → 任务管理器强杀 opx.exe → 重启 OPX → MySQL 仍显示运行中、监控中心有曲线
2. 点停止 → 正常停止
3. 正常退出 OPX（走 stop_all_on_exit）→ 重启 → 全部 Stopped（原行为不回归）

## 改动文件

- `src-tauri/src/services/software_manager/mod.rs`（reconcile + load 落盘 + 3 个单测）
