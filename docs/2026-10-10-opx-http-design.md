# 阶段 4「opx-http」架构设计：方案对比与推荐

> 状态：设计稿 v1（供用户裁定，未实施） · 日期：2026-10-10
> 关联 ADR：`docs/2026-10-09-opx-web-architecture.md`（形态 A 已定，5 项决策已拍板）、`2026-10-09-opx-web-refactor-handover.md`
> 前提：阶段 1~3 已完成 —— opx-core 独立可构建，services 全家已下沉，`EventSink` trait 已贯通，命令层是薄包装。

---

## 0. 现场核实的关键代码事实（设计依据）

| # | 事实 | 出处 |
|---|---|---|
| F1 | `TauriEventSink` 在 `setup()` 顶部装配（`Arc::new(TauriEventSink::new(app.handle().clone()))`），随后注入 LogWatcher / startup_bootstrap / backup_scheduler / renew_scheduler / watchdog / recorder 共 6 处后台任务 | `src-tauri/src/lib.rs:31-204` |
| F2 | `EventSink` trait 精确签名：`fn emit(&self, event: &str, payload: Value)` + 默认方法 `fn resource_dir(&self) -> Option<PathBuf>`（None = headless 无内置包，回退在线下载）；`EventSinkExt::emit_ser` 保证 JSON 形状与 Tauri `app.emit` 逐字节一致 | `crates/opx-core/src/event.rs` |
| F3 | 命令层共 13 个模块约 **124 个命令**；绝大多数签名是 `State<'_, Arc<XxxManager>>`（部分加 `AppHandle`）。`AppHandle` 的用途只有三种：① `sink_of(app)` 转 `Arc<dyn EventSink>`；② 窗口/退出/托盘控制（quit_app / exit_app / hide_main_window / refresh_tray_menu）；③ `app_version` 读 package_info、config 读写拿 `_app`（实际未用） | `commands/*.rs` |
| F4 | 后台任务全部经 `tauri::async_runtime::spawn` 启动 —— tauri 的 async_runtime 底层就是共享 tokio runtime，HTTP 服务器可以同宿主 | `lib.rs:128-224` |
| F5 | `startup_bootstrap::run_bootstrap(software, node, stack, sink: Arc<dyn EventSink>, node_exe)` —— 签名已完全 Tauri 无关，headless 可直接复用 | `src-tauri/src/services/startup_bootstrap.rs:261` |
| F6 | 前端所有 invoke 均经 `src/utils/ipc.ts` 唯一入口（invoke + 错误 i18n 转译，reject 抛字符串）；但**事件订阅是裸的**：约 15 个文件直接 `import { listen } from '@tauri-apps/api/event'`（App.vue、install/lifecycle/stack store、useRunningSoftware、useUpdater、StopProgressDialog、SoftwareListPage、RepositoryPage、SpringBootPage、LogViewer×2、DashboardPage、NodeAppsPage、SiteEditDialog） | 全局 grep |
| F7 | workspace：`members = ["crates/opx-core", "src-tauri"]`；tokio `1.0`（features: rt-multi-thread, macros, net, time）；reqwest 0.13；base64 0.23.1、rand_core 0.6(getrandom)、sha2、futures 已有；**无 axum/tower** | `Cargo.toml` |
| F8 | 本机 crates.io 不可达（ADR §6），但 registry cache 已核实含：axum **0.8.9**、axum-core 0.5.6、tower 0.5.3、tower-http 0.6.11、hyper 1.11.1、tokio-tungstenite 0.29、subtle 2.6.1、mime_guess 2.0.5、rand 0.9.x —— 新依赖可 `cargo check --offline` | 现场核验 |
| F9 | `AppSettings` 尚无任何 `web_*` 字段；已知坑：新增设置须三处齐改（Rust 模型+TS 模型、页面 ref+save、SettingsPage watch 依赖数组） | `models/settings.rs` |
| F10 | **路径型命令**在 HTTP 远程模式下语义断裂：`upload_site_bundle(id, loc_path, zip_path)`、`download_log(installed_id, source_index, dest_path)`、`export_combined_log(...)`、`export_audit_entries` 的参数都是**服务器侧文件路径**（桌面模式前端用 dialog 插件选本机路径）。浏览器远程访问时「客户端本地路径」对服务器无意义 | `commands/website.rs:170`、`commands/software.rs:1600/1618` |
| F11 | ADR 已拍板（本次设计不得推翻，只做落实细化）：静态 token（B2）、端口 17580、设置页「打开浏览器」入口、手动重置 token + 变更自动重生成、首期不做 SSE | ADR §7 |

---

## 1. D1：HTTP/WS 服务器选型与同进程嵌入

### 方案 A：axum（推荐）
在 `src-tauri` 旁边新建 `crates/opx-http`（lib，依赖 opx-core），axum 0.8 组路由：
`POST /api/:cmd` 通用分发器 + `GET /api/events/ws`（axum 自带 `ws` feature，内部就是 tokio-tungstenite，无需直接依赖）+ `GET /`（SPA 静态资源）。

### 方案 B：actix-web
功能同等，生态成熟。

### 方案 C：手写 hyper / tokio-tungstenite 裸栈
零框架，直接 hyper service + 自己解析路由。

| 维度 | A axum | B actix-web | C 手写 hyper |
|---|---|---|---|
| tokio 共宿 | ✅ 原生 tokio，直接 `tauri::async_runtime::spawn(server)` | ⚠️ actix-rt 自带 runtime 封装，跨 runtime spawn 心智负担 | ✅ 原生 tokio |
| 与 tauri::async_runtime 共存 | 无冲突（F4） | 双 runtime 并存，tokio 资源（reqwest/tokio 进程句柄）跨 runtime 使用有坑 | 无冲突 |
| WS | `ws` feature 内置（tokio-tungstenite 已在缓存） | 内置 | 全手写 upgrade/分帧 |
| 缓存可用（F8 离线约束） | ✅ 0.8.9 | ❌ 缓存未见 actix 系 | ✅ hyper 1.11 |
| 通用分发器实现成本 | 低（`/:cmd` path param + State 注入） | 中 | 高 |
| 依赖体积 | 中（tower 系） | 中（actix 系） | 小 |

**推荐：A（axum）**。理由：唯一同时满足「缓存离线可用 + 与 tauri::async_runtime 零摩擦 + 通用分发器成本最低」的选项；tower-http 可选（仅静态目录中间件，也可用 mime_guess 手写 ~30 行以少拖依赖）。C 的手写量对 124 命令分发无任何收益。

**生命周期**：
- 桌面模式：`web_enabled`（默认 false）开启后，在 `setup()` 内 `tauri::async_runtime::spawn` 启动 axum server；随进程退出自然终止（无需显式 stop，Windows 进程退出即释放端口）。设置里改端口/开关 → 热生效方式：重启 server task（保留 JoinHandle，abort 后重启），实现成本低。
- headless 模式：server 就是主循环，`tokio::main` 直启（见 D2）。
- 绑定地址：`web_lan_access=false`（默认）→ `127.0.0.1:17580`；true → `0.0.0.0:17580`（ADR §3.4）。

---

## 2. D2：headless 入口形态

### 方案 A：同一 binary 加 `--headless` CLI flag
`lib.rs::run()` 开头解析 `std::env::args`，发现 `--headless` 则走 `run_headless()` 分支，不进 `tauri::Builder`。

### 方案 B：独立 bin `opx-server`（ADR §2 原规划）
workspace 新增 `crates/opx-server`（bin），依赖 opx-core + opx-http，`#[tokio::main]` 自行编排。

### 方案 C：A+B 混合 —— 桌面 binary 支持 `--headless`（Windows 应急/调试），Linux headless 用独立 bin
opx-server 与桌面 binary 共享同一个 `run_headless(ctx)` 函数（放 opx-http），两壳各一行调用。

| 维度 | A 同 bin flag | B 独立 bin | C 混合 |
|---|---|---|---|
| 启动编排差异 | 需在 run() 内分叉，setup 逻辑要抽成「与 AppHandle 无关的初始化函数」（路径创建、settings 加载、代理初始化、audit init、6 个后台任务——F1 全部只依赖 Arc + sink，**可整体复用**） | 独立 main 干净，无分叉 | 两处都要，但共享同一函数 |
| Linux 无桌面部署 | 需带 webview 依赖的 fat binary | ✅ 纯服务端依赖，容器友好（ADR 阶段 4 目标） | ✅ |
| 单实例/托盘/updater | headless 分支需显式跳过（cfg 或运行时判断） | 天然不存在 | 同 A |
| 维护面 | 一个 binary | 两个 binary，多一份 CI 产物 | 折中 |
| Windows 用户上手 | ✅ `opx.exe --headless` 即可，无需新下载 | 需单独获取 opx-server.exe | ✅ |

**推荐：C（近期先做 A 的能力，opx-server bin 作为阶段 4 收尾/阶段 5 的 Linux 交付）**。
关键事实支撑：F1 + F5 表明启动编排的**全部实质工作**（管理器 Arc 构造、bootstrap、6 个后台任务）只依赖 `Arc<dyn EventSink>` 与各 Manager Arc，`run_headless` 只需把 `TauriEventSink` 换成 `WsEventSink`、`node_exe` 传 `None`（EventSink::resource_dir 默认 None 的语义正好是为此预留的，F2）。风险集中在 Windows：桌面 binary 链接了 tauri，`--headless` 仍会加载 webview2 相关 DLL（不影响运行，但包体不减）；独立 bin 才有瘦身效果 —— 因此独立 bin 不急于本阶段完成。

---

## 3. D3：token 鉴权（ADR B2 的落实设计）

ADR 已定：32 字节 CSPRNG → hex、`subtle` 常量时间比较、`Authorization: Bearer`、WS 握手校验、设置页展示 + 手动重置 + 变更自动重生成、受限写盘（`paths::write_file_restricted`）。以下是 ADR 未定死的三个实现细节：

### D3.1 headless 无窗口时用户怎么拿到 token
| 方案 | 做法 | 权衡 |
|---|---|---|
| A 控制台打印 | 启动横幅打印 `http://127.0.0.1:17580/#token=xxx`（完整可点击链接） | ✅ 最直观；❌ 终端关闭后不可再查（需配合 B） |
| B 落盘 token 文件 | token 本就在 settings.json 的 `web_token` 字段（受限写盘），headless 启动横幅同时提示文件路径 | ✅ 可复查；settings.json 还存其他敏感项，权限同源 |
| C 启动生成新 token | 每次 headless 启动轮换 | ❌ 与 ADR「手动重置才轮换」冲突，浏览器会话反复失效 |

**推荐：A+B 组合**——横幅打印带 `#token=` 的完整 URL（fragment 不入服务器日志、不随请求上行，ADR §3.4 已定此机制），并注明「亦可在 settings.json 查看/重置」。

### D3.2 WS 握手鉴权（浏览器 WebSocket API 不能设 Authorization 头）
| 方案 | 做法 | 权衡 |
|---|---|---|
| A query param `?token=` 直传 | 实现最简单 | ❌ token 可能进反代/系统日志（局域网场景无反代，风险低但不干净） |
| B 一次性 ticket | `POST /api/ws-ticket`（Bearer 鉴权）返回 60 秒单次有效随机 ticket → `WS /api/events/ws?ticket=` | ✅ token 永不出现在 URL；实现仅 +1 端点 + 内存 HashMap |
| C `Sec-WebSocket-Protocol` 夹带 | token 塞子协议头 | ❌ 玄学 hack，浏览器兼容性差 |

**推荐：B（一次性 ticket）**。成本约 40 行，换取 token 零泄漏面，与本项目「鉴权按局域网标准从严」（ADR 2026-10-09 决策 #1）一致。

### D3.3 HTTP header vs query param（普通命令）
统一 `Authorization: Bearer <token>`（fetch 可设头，无障碍）。**不做** cookie/session（免疫 CSRF，ADR 已定）。

---

## 4. D4：事件推送 —— EventSink fan-out

现状：单 sink 注入（F1）。双入口后桌面窗口与浏览器要**同时**收到事件。

### 方案 A：组合广播（FanOutSink）（推荐）
opx-core 新增（或 opx-http 提供）：
```rust
pub struct FanOutSink { sinks: Vec<Arc<dyn EventSink>> }   // emit 逐个转发
```
- 桌面模式（web 开启）：`FanOutSink([TauriEventSink, WsEventSink])` → 两入口同收。
- 桌面模式（web 关闭）：仅 `TauriEventSink`（与现状逐字节一致，零回归面）。
- headless：仅 `WsEventSink`。
- **改动点只有 lib.rs 一处装配 + WsEventSink 新实现**，6 个后台任务与 services 零改动（F1/F2 的抽象红利）。

### 方案 B：模式二选一
headless 用 WsEventSink，桌面仍单 TauriEventSink，「浏览器随桌面并存」退化为「桌面开 web 时浏览器收不到事件，只能轮询」→ 不可接受的体验降级。

### 方案 C：事件总线 hub（注册订阅者 + 每订阅者队列）
最通用（支持 per-client 过滤、背压），但引入订阅生命周期管理，本期事件仅 12 种 + 客户端 ≤个位数，过度设计。

| 维度 | A FanOut | B 二选一 | C hub |
|---|---|---|---|
| 双入口同收 | ✅ | ❌ | ✅ |
| 改动面 | 装配处 1 行 + 新 sink 1 个 | 中 | 大 |
| 背压/slow client | 广播时 per-WS 用 bounded channel，满则丢弃+记 warn（事件是通知不是数据，前端有全量刷新兜底） | — | 可精细控制 |
| 复杂度 | 低 | 低 | 高 |

### WS 断线重连丢事件补偿（三选项）
1. **重连成功后触发一次全量刷新**（推荐，ADR §5 已列）：前端 transport 的 `on()` 重连回调发布一个内部 `resync` 事件，各 store 现有的刷新函数挂上即可；实现集中、语义简单。事件流里 `software-status-changed`/`install-progress` 等**都伴随可重新拉取的全量状态**（installed.json 列表、快照任务列表），天然适合。
2. 事件序列号 + 服务端环形缓冲重放（last-N）：可靠但要改 `EventSink` 签名或包一层序号信封，且 12 种事件各自的重放语义要逐个定义 —— 成本高收益低。
3. 不补偿：❌ install-progress 丢失会让进度条卡死。
**推荐 1**，并在实施批次中把「重连→resync」作为前端 transport 的内置行为（调用点无感）。

---

## 5. D5：前端传输层抽象

### 环境检测
ADR §3.2 已定 **A2 运行时探测**（检测 `__TAURI_INTERNALS__`，一份产物通吃），本次确认无推翻理由；备选的编译期 `VITE_TRANSPORT` 需双构建，违背「一份 SPA」目标。落实为：

```ts
// src/utils/transport.ts —— 新增抽象，ipc.ts 内部改用之，54 个调用点签名不变
export interface Transport {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
  listen(event: string, cb: (payload: unknown) => void): Promise<() => void>
  isDesktop: boolean   // 桌面专属能力开关
}
```
- `TauriTransport`：包装现有 `@tauri-apps/api` 的 invoke/listen（行为与今天完全一致）。
- `HttpTransport`：`fetch(POST /api/:cmd, {Authorization})` + 单条 WebSocket（多路分发到各 `listen` 回调）+ 断线自动重连（指数退避）+ 重连后 resync。
- **错误转译单一入口保持**：HttpTransport 拿到 `{code,message}` 后同样走 `translateError`（沿用 ipc.ts 既有约定）。

### 事件订阅迁移（F6 的 15 个文件）
`listen` 从 `@tauri-apps/api/event` 改 import `@/utils/transport` 的同名函数——机械替换，签名对齐（Tauri 的 `event.payload` vs 我们的 cb payload 在 transport 内抹平：Tauri 回调包 `{payload}`，抽象层直接给 payload）。

### 命令路由表（哪些走 HTTP、哪些桌面专属）
| 类别 | 命令 | HTTP 模式行为 |
|---|---|---|
| 通用（约 115 个） | 全部业务命令（software/springboot/website/stack/node_app/config/dns_account/audit/system/lock_screen） | 直接可用（服务器与本机同 FS，路径参数语义同桌面） |
| 桌面专属 | `quit_app` / `exit_app` / `hide_main_window` / `refresh_tray_menu` | HTTP 返回 404/`desktop_only` 错误码（前端按 `transport.isDesktop` 直接不渲染对应入口，不发起调用） |
| 桌面专属 | `install_app_update` / `check_app_update`（updater 插件） | 同上；HTTP 模式「关于页」隐藏更新按钮或提示到桌面端操作 |
| 需适配 | `app_version` | HTTP 侧由 opx-http 提供 `/api/app_info`（编译期 env! 版本号），无需 AppHandle |
| 需适配 | `get_settings`/`save_settings` 的 `_app` 参数 | 本就未用（F3），签名去 `AppHandle` 化即可 |
| 路径断裂（F10） | `upload_site_bundle` / `download_log` / `export_combined_log` / `export_audit_entries` | **本阶段从简**：HTTP 模式下 dest_path 语义 = 服务器侧路径（管理本机文件，本来就是 ops 工具语义）；`upload_site_bundle` 的 zip_path 同理（要求 zip 已在服务器磁盘）。真正的浏览器上传/下载（multipart / 流式下载）列为开放项，不阻塞主链路 |

---

## 6. D6：端口策略

| 方案 | 做法 | 权衡 |
|---|---|---|
| A 固定默认 + 可配置（ADR 已定 17580） | settings `web_port`；占用时明确报错 | ✅ URL 稳定可收藏、防火墙规则一次配好；❌ 冲突需用户手改 |
| B 动态分配 + 落盘/打印告知 | 每次启动绑 0 号端口后回写 | ❌ URL 漂移，浏览器书签失效，防火墙规则失效，headless 用户困惑 |
| C 固定 + 自动 +N 重试 | 17580→17581→…→17590 | 冲突自愈，但实际监听端口与 UI 显示可能不一致，需额外回显逻辑 |

**推荐：A（与 ADR §7 #2 一致），冲突行为采纳「明确报错」**（正好把 ADR 遗留开放项 #2 关掉）：启动失败在桌面端 emit `web-server-error` 事件 + 设置页横幅，headless 打印到控制台；`web_lan_access`/`web_port` 变更时重启 server（见 D1 生命周期）。默认绑 `127.0.0.1` 是安全底线（LAN 开关才放开）。

---

## 7. 阶段 4 最大的工程问题：124 个命令如何挂上 HTTP（独立决策点 D7）

命令内核已在薄包装之下（services 已进 core），但壳层 commands 里仍有 oplog 宏、`sink_of(app)`、参数解包等胶水。三方案：

### 方案 A：服务上下文结构 + 命令层「去 AppHandle 化」（推荐）
1. 新建 `AppContext`（opx-core 或 opx-http）：
```rust
pub struct AppContext {
    pub software: Arc<SoftwareManager>,
    pub website: Arc<WebsiteManager>,
    pub springboot: Arc<SpringBootManager>,
    pub node: Arc<NodeAppManager>,
    pub dns: Arc<DnsAccountManager>,
    pub stack: Arc<StackManager>,
    pub sink: Arc<dyn EventSink>,      // FanOutSink 或单 sink
    pub node_exe: Option<PathBuf>,
}
```
2. 命令层逐个把 `app: AppHandle`（仅用于 sink_of 的，F3 类型 ①）替换为 `ctx: State<AppContext>` 或直接取字段 —— Tauri 命令签名变化但**前端无感**；窗口/托盘类命令（类型 ②）保持 AppHandle 不动。
3. opx-http 的分发器注册表 `HashMap<&str, Handler>` 直接闭包调用同一批函数 → **胶水写一份，两边共享**。

### 方案 B：opx-http 自写 124 个适配器
不动命令层，HTTP 侧逐命令重写胶水。❌ 两份胶水长期漂移，oplog/错误码不一致，维护灾难。

### 方案 C：完整用例层下沉 core（ADR §3.5 原设想）
把 commands 内业务部分整体搬进 opx-core use-case 模块。✅ 最彻底；❌ 阶段 4 工作量翻倍，且命令层现在已是薄包装，边际收益低。

**推荐 A**：与阶段 2/3 的手法同构（trait 抽象 + 机械替换），Tauri 侧改动可逐文件小批验证；C 可作为后续演进而非本阶段目标。

---

## 8. 推荐组合总览（一页纸）

```
┌────────────────────────── 同一进程 ──────────────────────────┐
│  桌面模式（现状 + web 开关）          headless 模式（--headless）│
│  Tauri Builder setup()               run_headless(ctx)          │
│   ├─ AppContext 构造（两模式共用）    ├─ 同一 AppContext 构造    │
│   ├─ sink = FanOut[Tauri,Ws]         ├─ sink = FanOut[Ws]       │
│   ├─ 6 后台任务（不变）              ├─ 同 6 后台任务（复用）    │
│   └─ web_enabled → spawn axum       └─ axum 就是主循环          │
│                                                                 │
│  opx-http: axum 0.8 · POST /api/:cmd 分发器（共享命令胶水）      │
│            · GET /api/events/ws（ws ticket 鉴权）· Bearer 鉴权   │
│            · GET / 静态 SPA（mime_guess）· WsEventSink          │
└──────────────────────────────────────────────────────────────┘
前端：src/utils/transport.ts 双通道（运行时探测 __TAURI_INTERNALS__）
      ipc.ts 错误转译不变 · 15 文件 listen 机械换 import · 重连 resync
```

---

## 9. 实施批次拆分草案（每批有明确验收点）

> 粒度对齐阶段 3；每批结束桌面版必须保持「功能与数据逐项不变」+ `cargo test -p opx-core` 全绿。

| 批次 | 内容 | 验收点 |
|---|---|---|
| **4.0 骨架** | 建 `crates/opx-http`：axum server + Bearer 中间件（token 生成/校验/受限写盘进 settings）+ `WsEventSink` + `FanOutSink` + `/api/health`；workspace 注册成员 | `cargo check --offline` 通过；`cargo run` 单测里起 server，curl 带 token 打通 health，错 token 401 |
| **4.1 AppContext + 命令层去 AppHandle 化** | `AppContext` 结构；13 个 commands 模块中 sink_of 型 AppHandle 全部替换为 ctx（约 30+ 处机械替换，窗口/托盘类不动）；lib.rs 装配改为构造 ctx 后 manage | 桌面版全功能回归（含安装/启动/事件推送 6 处后台任务）；`app_version`/`get_settings` 等小命令单独验证 |
| **4.2 通用分发器 + 只读命令先行** | `POST /api/:cmd` 分发器 + 注册表；先挂只读命令（list_*/get_*/system_info/audit 等 ~40 个）；设置新增 `web_*` 字段（**三处齐改**）+ SettingsPage web 配置组 | 浏览器（同机 127.0.0.1）带 token 可查列表、看监控曲线；桌面窗口与浏览器并存状态一致 |
| **4.3 全量命令 + 事件通路** | 注册表补齐写操作命令；桌面专属命令清单返回 `desktop_only`；WS `/api/events/ws` + ticket 端点接入；`install-progress` 等关键事件经 WsEventSink 实测 | 浏览器完成一次完整安装软件流程（进度条实时跳动=事件通）；浏览器点启动/停止软件生效 |
| **4.4 前端 transport 双通道** | `transport.ts`（Tauri/Http 双实现 + 探测 + WS 重连 + resync）；ipc.ts 内部切换；15 文件 listen 换 import；桌面专属 UI 按 `isDesktop` 隐藏 | **同一份 dist**：Tauri 窗口全功能不变；浏览器全功能（除桌面专属）；拔网线重连后列表自愈 |
| **4.5 headless 入口 + 收尾** | `run_headless(ctx)` 复用 bootstrap/后台任务；`--headless` flag + 控制台横幅（带 `#token=` URL + settings 路径提示）；静态 SPA 服务（mime_guess）；端口冲突明确报错；设置页「打开浏览器」+ LAN 开关风险提示 + 重置 token | Windows 11：`opx.exe --headless` 启动→浏览器打开横幅 URL→全功能可用；`web_lan_access=true` 后手机经局域网访问；关闭 LAN 回落 127.0.0.1；token 重置后旧会话 401 被引导重新输 token |
| （后续）4.6+ | `crates/opx-server` 独立 bin（Linux/容器）；浏览器文件上传/下载适配（F10）；自签 TLS/IP 白名单 | 列入开放项，不阻塞本阶段 |

---

## 10. 新增依赖清单（全部已核实 registry cache 可离线解析，F8）

| crate | 版本 | 用途 | 理由 |
|---|---|---|---|
| `axum` | 0.8.9（features: ws, json） | HTTP 路由/分发器/WS upgrade | 唯一缓存可用且 tokio 同宿的框架；ws feature 内含 tokio-tungstenite，无需直接依赖 |
| `tower` | 0.5.3 | axum 传递依赖/中间件 | 随 axum |
| `subtle` | 2.6.1 | token 常量时间比较 | 防时序侧信道（ADR §3.4） |
| `mime_guess` | 2.0.5 | 静态 SPA Content-Type | 30 行手写替代 tower-http serve-dir，少拖依赖 |
| `rand` | 0.9.x | token 生成（或复用已有 `rand_core`+`getrandom` 直取字节） | CSPRNG；倾向后者，零新依赖 |
| `tokio-tungstenite` | — | （经 axum ws 间接引入，不直接依赖） | — |

明确**不引入**：actix 系、tower-http（可用性让位于依赖最小化）、jsonwebtoken（无会话需求）、数据库类。

---

## 11. 风险点

| # | 风险 | 等级 | 对策 |
|---|---|---|---|
| R1 | `tauri::async_runtime` 与 axum 的 runtime 归属：axum server 必须跑在 tauri 初始化的那个 tokio runtime 上 | 中 | 一律经 `tauri::async_runtime::spawn` 启动（桌面）/`#[tokio::main]`（headless），禁止自建 runtime；批次 4.0 用集成测试钉死 |
| R2 | 4.1 批 30+ 处机械替换引入回归（生命周期/借用） | 中 | 只替换 sink_of 型（F3 类型①），分 13 个 commands 模块小批提交，每批 `cargo check` + 桌面冒烟 |
| R3 | WS 慢客户端/背压拖垮广播 | 低 | WsEventSink per-client bounded(64) channel，满则丢+warn（事件均有全量刷新兜底） |
| R4 | token 泄漏（LAN 明文、settings.json 可读） | 中 | ADR 已定对策维持：默认 127.0.0.1、LAN 开关强制风险提示、受限写盘、日志脱敏、fragment 传 token、ticket 机制（D3.2） |
| R5 | 前端 15 文件 listen 迁移漏改 → 某页面浏览器端无事件 | 中 | 迁移批次用 `grep "@tauri-apps/api/event"` 必须归零作为验收门 |
| R6 | 路径型命令（F10）在远程模式下语义与用户预期不符（文件落在了服务器侧） | 中 | UI 文案明示「保存到运行 opx 的那台机器」；上传/下载适配列为开放项 |
| R7 | 离线依赖解析失败（cache 版本与 Cargo.lock 解析图冲突） | 低 | 批次 4.0 第一步即 `cargo check --offline`，失败则回退 rand_core 直取方案并微调版本 |
| R8 | `--headless` 与 Windows 单实例插件交互（第二实例激活逻辑误触发） | 低 | headless 分支不注册 single_instance 插件；或 headless 下忽略激活回调 |

---

## 12. 与既有 ADR 的关系

- 完全继承：形态 A（单进程双入口）、B2 静态 token、端口 17580、运行时探测、WS-only、设置页入口。
- 本设计新增裁定建议：FanOutSink 组合广播（D4）、ws ticket（D3.2）、AppContext 共享胶水（D7-A）、`--headless` 先行 + opx-server 延后（D2-C）、端口冲突明确报错（D6，关闭 ADR 开放项 #2）。
- 需用户确认后进入实施（建议从批次 4.0 + 4.1 起步，4.1 与 4.2 无强依赖可并行推进）。
