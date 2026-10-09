# opx 架构重构交接文档（阶段 1 ~ 2）

> **日期**：2026-10-09 · **HEAD**：`2e25d40`（gitee + github 已同步）
> **测试**：`cargo test --workspace` = **344 passed / 0 failed / 0 warning**
> **关联文档**：形态 A 设计 `2026-10-09-opx-web-architecture.md` · 跨平台 ADR `2026-09-28-cross-platform-readiness.md`

---

## 1. 目标与路线

把 opx 从「Tauri 桌面单体」拆为「**平台无关 core + 两个壳**」，为 headless `opx-server`（浏览器 web 管理）铺路。
设计要点见形态 A 文档：同一后端进程持有唯一状态，桌面窗口（Tauri IPC）与浏览器（HTTP/WS）是两个入口。

| 阶段 | 内容 | 状态 |
|---|---|---|
| **1** | 建 Cargo workspace；`models` / `utils` 搬入 `opx-core` | ✅ `a05469a` |
| **2** | 引入 `EventSink` 抽象；服务逐个去 Tauri 化 | 🟡 **5/12 完成** |
| **2.5** | 互连子图（7 服务 + 2 中心函数 + 21 处调用） | ⬜ **下次接续** |
| **3** | `services` 全量搬入 `opx-core` | ⬜ 依赖 2.5 |
| **4** | `opx-http`（内嵌 HTTP/WS + token 鉴权）+ 前端传输层 | ⬜ |

---

## 2. 已完成

### 2.1 阶段 1：workspace 化（`a05469a`，97 files 全部 100% rename）

```
opx/
├── Cargo.toml            ← workspace（members: crates/opx-core, src-tauri）
├── Cargo.lock            ← ⚠️ 从 src-tauri/ 移到根（本机 crates.io 不可达，lock 必须随 workspace 移动）
└── crates/opx-core/      ← 平台无关核心，零 tauri 依赖
    ├── src/models/       （10 文件）
    ├── src/utils/       （8 文件；update.rs 因依赖 tauri-plugin-updater 留壳层）
    └── src/event.rs     （EventSink trait）
```

- `src-tauri` 侧 105 文件 + `tests/` 集成测试的引用改写为 `opx_core::`。
- 根 `.gitignore` 补 `/target/`（workspace 化后产物在仓库根，而 `src-tauri/.gitignore` 的 `/target/` 只锚定 `src-tauri/target`）。

### 2.2 阶段 2：EventSink 抽象 + 5 个服务去 Tauri 化

**核心抽象**
- `crates/opx-core/src/event.rs` —— `EventSink` trait：
  - `fn emit(&self, event: &str, payload: Value)`（必需）
  - `fn resource_dir(&self) -> Option<PathBuf>`（**默认方法**）
    > 为什么资源目录挂在事件 trait 上：它是除事件外**唯一**的宿主上下文依赖（原 `AppHandle::path().resource_dir()`），逐层传参会穿透 5 层函数签名，而 Rust 无法把两个 trait 对象合成一个参数；默认 `None` 正好覆盖 headless（无 bundle 资源）。
- `src-tauri/src/event_sink.rs` —— `TauriEventSink` 包装 `AppHandle`，转发 `emit` + 提供 `resource_dir`。
- `lib.rs` setup 开头构造**统一事件出口** `event_sink: Arc<TauriEventSink>`，所有后台任务共用同一实例。

**已零 tauri 依赖的 5 个服务**
`installer` · `log_watcher` · `recorder` · `notify` · `renew_scheduler`

提交：`9ee9d6a`（EventSink + 前三个）、`f49f5de`（notify）、`2e25d40`（renew_scheduler）

---

## 3. 下一步：阶段 2.5 互连子图

### 3.1 为什么必须一次做完

剩余 7 个服务构成**强连通子图**，改任一处立刻波及其余（已实测两次回滚验证）。核心是两个"被多方调用"的中心函数：

| 中心函数 | 被谁调用 |
|---|---|
| `commands::software::do_start_software`<br>`(software.rs:1106)` | `watchdog.rs:194`、`stack_manager.rs:654`、`software_manager/lifecycle.rs:776`、`startup_bootstrap.rs:200` |
| `software_manager::lifecycle::emit_status_changed`<br>`(lifecycle.rs:664)` | **21 处**：`commands/software.rs` 15 · `stack_manager.rs` 3 · `watchdog.rs` 2 · `backup.rs` 1 |

### 3.2 改动顺序（约 30+ 点 / 9 文件）

1. **两个中心函数改签名**：`&AppHandle` → `&Arc<dyn EventSink>`
2. **services 层（6 个）**：`software_manager/lifecycle` · `watchdog` · `stack_manager` · `startup_bootstrap` · `springboot_manager/lifecycle` · `backup` + `backup_scheduler`
   （`backup_scheduler` 依赖 `backup::create_snapshot`；`backup` 依赖 `emit_status_changed` —— 一环扣一环）
3. **commands 层（3 个）**：`software.rs` / `stack.rs` / `springboot.rs` 构造 sink

### 3.3 三个已探明的坑

1. **`Arc<T>` → `Arc<dyn Trait>` 不会自动 coerce** —— 必须显式标注类型：
   ```rust
   // ❌ found `&Arc<TauriEventSink>`
   // ✅ 助手函数集中一次转换
   fn sink_of(app: &tauri::AppHandle) -> std::sync::Arc<dyn opx_core::event::EventSink> {
       std::sync::Arc::new(crate::event_sink::TauriEventSink::new(app.clone()))
   }
   ```
2. **同名但不同类型的 `app`**：`stack_manager.rs` 与 `springboot_manager/lifecycle.rs` 里 `app` 是 **SpringBoot 变量**（`springboot_mgr.find_app(..)`），AppHandle 参数分别叫 `app` / `app_handle`。sed 必须按「行首缩进参数 `^    app: &AppHandle,$`」或「具名调用 `self.xxx(app, `」定位。
3. **测试数据勿伤**：`lifecycle.rs:903-912` 有字符串 `mk("app", 3)`；现有模式不会匹配到它，但批量替换后**务必看「残留 app 引用」清单**确认。

---

## 4. 关键约定（后续必须沿用）

- **参数类型**：内部函数一律 `&Arc<dyn EventSink>` —— 借 `Arc: Deref` 让 `sink.emit(..)` 直接可用，**免去层层 clone**；仅跨 `spawn` 边界（需要 `'static`）才 `sink.clone()`。
- **emit 统一**：`sink.emit("event-name", payload)`，投递失败静默忽略（与原 `app.emit` 行为一致，事件推送不阻断业务）。
- **构造点收敛**：后台任务共用 `lib.rs` 的 `event_sink`；命令层用各自文件里的 `sink_of(&app_handle)` 助手，**不要在每个调用点重复构造**。
- **验证纪律**：每次改完跑 `cargo test --workspace`（**不是 `--lib`** —— 集成测试 `tests/*.rs` 也引用这些路径，只跑 `--lib` 会漏），基线 **344 passed**。

---

## 5. 验证方式

```bash
cd /d/object/opx
cargo test --workspace --offline        # 基线：344 passed（--offline 因本机 crates.io 不可达）
cargo tauri info                          # 确认 Tauri CLI 仍识别 workspace 布局
```

Windows 实机验证（P2-1 遗留）：装一个软件走完整链路（下载→解压→进度事件→启停），确认前端 `install-progress` 等事件正常。

---

## 6. 待处理 / 未提交

- `docs/2026-10-09-opx-web-architecture.md` —— **形态 A 设计草案，仍 untracked 未提交**（属 web 拆分那条线，待阶段 4 开工时再提交）。
- 阶段 2.5 完成后，阶段 3（services 搬入 core）才有意义；届时 `software_manager/mod.rs` 需拆分（它当前同时声明纯逻辑子模块与 tauri 子模块）。
