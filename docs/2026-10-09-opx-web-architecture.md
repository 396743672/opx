# opx Web 管理设计草案（形态 A：单进程双入口）

> 状态：草案 v2（5 项决策已定，见 §7） · 日期：2026-10-09 · 关联 ADR：`2026-09-28-cross-platform-readiness.md`
>
> 结论先行：**同一个后端进程持有唯一状态，桌面窗口（Tauri IPC）与浏览器（HTTP/WS）是两个入口**。
> 绝不采用「桌面壳 + 独立 opx-server 两个进程」——那会因状态无跨进程锁而互相覆盖（§3.1）。

---

## 1. 目标与非目标

**目标**
- Windows / macOS：桌面窗口照常，同时可用浏览器管理**同一份状态**。
- **局域网**：其它设备（手机 / 另一台电脑）可在**受控开关**下访问 Web 管理（默认关闭）。
- Linux（无桌面）：只跑 headless 壳，浏览器即唯一入口。
- 前端仍是**一份 Vue SPA**，不产生第二套 UI 代码。

**非目标（本期不做）**
- 远程管理**多台**目标机（混合机群）——那需要 catalog 运行时按目标主机 OS 选包（ADR §3.3），工程量另计。
- 服务管理抽象（systemd / launchd / Windows 服务注册）——独立项（ADR §6 #3）。
- 移动端壳。

---

## 2. 目标架构（crate 划分）

```
opx/                          ← Cargo workspace（新增根 Cargo.toml）
├── crates/
│   ├── opx-core/    (lib)    ← 业务逻辑 + 状态 owner，零 tauri 依赖
│   ├── opx-http/    (lib)    ← 内嵌 HTTP/WS + 鉴权，依赖 opx-core
│   ├── opx/         (bin)    ← Tauri 桌面壳，依赖 opx-core + opx-http（按需内嵌）
│   └── opx-server/  (bin)    ← headless 壳（Linux / 只跑 web），依赖 opx-core + opx-http
└── src/             (Vue SPA，一份，两壳共用)
```

**依赖方向（单向，不许回环）**：`opx / opx-server → opx-http → opx-core`。`opx-core` 不得依赖 tauri、不得依赖 axum。

**搬迁分类**（依据实测耦合，见 ADR §7）
| 类别 | 内容 | 处理 |
|---|---|---|
| 纯逻辑（约 40+ 文件） | `providers/*`（16 个全部）、`catalog`、`audit`/`audit_log`、`config_editor`、`health_check`、`log_viewer`、`netutils`、`process_monitor`、`ddns/*`、`dns_account/*`、`acme/dns/*`、`utils/paths`、`utils/download`、`utils/archive`、`utils/process`、`models/*` | **直接搬入 `opx-core`**，零改动 |
| Tauri 耦合（12 文件） | `installer`、`lifecycle`×2、`log_watcher`、`watchdog`、`backup_scheduler`、`renew_scheduler`、`recorder`、`notify`、`stack_manager`、`startup_bootstrap`、`backup` | 依赖 `EventSink` trait（§3.3）后搬入 |
| 壳专属（留桌面壳） | 托盘、窗口、单实例、updater、dialog/fs 插件、`main.rs` | 不进 `opx-core` |

---

## 3. 关键设计决策

### 3.1 状态归属：进程内唯一 owner（硬约束）

**现状事实**：`SoftwareManager` 是**内存态**（`RwLock<InstalledList>`）+ `save_installed_list` 落盘，**无跨进程锁**；桌面壳另有 `tauri_plugin_single_instance` 保单实例。

**决策**：整个进程只允许 `Arc<SoftwareManager>` 一份实例；桌面壳与 `opx-http` **共享同一个 Arc**（`opx-http` 从参数接收，不自行 `new()`）。

**明确禁止**：以独立进程方式同时运行桌面壳与 `opx-server`（后写盘者覆盖先写盘者）。

### 3.2 前端传输层（`@/utils/ipc` 改造）

选项：
- **A1 编译期切换**（`VITE_TRANSPORT=tauri|http`）：一份产物只能一种传输 → 需双构建。
- **A2 运行时探测**（检测 `__TAURI_INTERNALS__`：有→IPC，无→HTTP）：**一份产物通吃**。✅ **已定**
- A3 双构建双产物：维护成本高。

**改法**（`utils/ipc.ts` 是唯一入口，基础很好）：
```ts
// transport 在应用启动时选定（探测 __TAURI_INTERNALS__）
export interface Transport {
  invoke<T>(cmd: string, args?: unknown): Promise<T>
  on(event: string, cb: (payload: unknown) => void): () => void
}
```
- Tauri 实现：`@tauri-apps/api/core.invoke` + `@tauri-apps/api/event.listen`。
- HTTP 实现：`fetch('/api/'+cmd, {method:'POST', headers:{Authorization}, body})` + WebSocket 订阅。
- 现有 54 处调用点**无需改动**（`invoke()` 签名不变）。

### 3.3 事件总线：`EventSink` trait

现状后端 **emit 12 个事件**：
`catalog-refreshed`、`software-status-changed`、`software-uninstalled`、`stack-status-changed`、
`install-progress`、`export-progress`、`import-progress`、`stop-complete`、`log-file-changed`、
`close-requested`、`tray-software-stop`、`update-available`
（前端订阅：`install-progress`、`stop-complete`、`springboot-status-changed`、`close-requested` 等）

设计：
```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}
```
- 桌面壳实现 → `app.emit(event, payload)`（原行为不变）。
- `opx-http` 实现 → 向所有 WebSocket 订阅者广播。
- `opx-core` 内部只持有 `Arc<dyn EventSink>`，不再触碰 `AppHandle`。

### 3.4 鉴权（**因「支持局域网」而提高要求**）

**决策记录（2026-10-09）**：首期**支持局域网访问**，故鉴权不可省，且比 localhost 场景更严。

选项：
- **B1** localhost + 无鉴权 → ❌ 同机其他用户 / 恶意网页可 CSRF。
- **B2 静态 token**（首次生成、落盘、请求头携带）→ ✅ **本期采用**。
- **B3** 账号密码 + session cookie → 多用户 / 公网场景再加。
- B4 mTLS → 过重。

**B2 细节**：
- 生成：32 字节 CSPRNG（`rand`）→ hex。
- 校验：`subtle` 常量时间比较，防时序侧信道。
- 传输：`Authorization: Bearer <token>`（**用 header 不用 cookie** → 天然免疫 CSRF）。
- WebSocket：握手时校验 token。
- 前端：token 存 `localStorage`（同源）；桌面壳「打开浏览器」时经 **URL fragment**（`#token=…`，不随请求发往服务器、不入访问日志）传入。

**绑定的两种模式**（由 `web_lan_access` 控制）
| `web_lan_access` | 绑定 | 场景 |
|---|---|---|
| `false`（默认） | `127.0.0.1` | 仅本机浏览器 |
| `true` | `0.0.0.0` | 局域网其它设备可访问 |

🚨 **局域网带来的明文风险（必须正视）**：HTTP 未加密，同网段的设备可**嗅探到 token**。
- 首期对策：**设置页开启局域网时弹出明确风险提示**（"仅在你信任的网络中开启"）+ 强制强随机 token。
- 后续演进（列为开放项）：自签 TLS、IP 白名单/限网段、或升级到 B3。

### 3.5 HTTP API 形态（124 命令零重写）

**映射规则**：`snake_case` 命令名 → `POST /api/<command>`，请求体 = 参数 JSON，响应体 = 命令返回值 JSON。

- **利**：124 个命令不需要逐个手写路由——`opx-http` 维护一张 `cmd_name → handler` 表，**一个通用 handler 分发**即可。
- 命令内核（即现在 `commands/*.rs` 里的业务部分）搬迁到 `opx-core` 的「用例层」，两个壳各写薄适配（Tauri `#[tauri::command]` 包装 / axum handler 包装）。
- Tauri 壳专属命令（`close-requested` 触发、单实例激活、updater）在 HTTP 侧返回明确「不适用」错误。
- 事件：`GET /api/events/ws`（WebSocket upgrade）。**首期不做 SSE 降级**（见 §7）。
- 静态资源：`/` 服务 Vue SPA 构建产物（`mime_guess` 定 Content-Type）。
- 命令分布参考（合计 124）：`software` 45 / `springboot` 23 / `website` 11 / `stack` 10 / `node_app` 7 / `config` 6 / `system` 5 / `dns_account` 4 / `lock_screen` 4 / `app` 3 / `audit` 3 / `update` 2。

### 3.6 配置项（`AppSettings` 新增）

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `web_enabled` | bool | `false`（桌面）/ `true`（headless） | 是否启用内嵌 HTTP |
| `web_lan_access` | bool | `false` | 局域网可访问（开启 = 绑 `0.0.0.0`，UI 提示风险） |
| `web_bind` | String | 随 `web_lan_access`（`127.0.0.1` / `0.0.0.0`） | 绑定地址（高级可覆盖） |
| `web_port` | u16 | **`17580`** | 监听端口（已避开被管软件默认端口，本机空闲） |
| `web_token` | String | 首次启动生成 | **敏感字段**，受限写盘 |
| `web_open_browser_on_start` | bool | `false` | 启动后自动开浏览器 |

🚨 **项目已知坑**：新增设置字段须**三处齐改**——① Rust `models/settings.rs` + TS 模型；② 页面 `ref` + save；③ SettingsPage 的 **watch 依赖数组**（漏③ = 改了没存）。
🚨 `web_token` 属敏感文件，写盘走 `paths::write_file_restricted`（清只读→写→收紧），**禁止 write 后单独 restrict**。

**「打开浏览器」入口**：放在**设置页**（web 配置组所在，就近操作）。

---

## 4. 改造阶段与验收

| 阶段 | 内容 | 验收标准 |
|---|---|---|
| **1** | 建 workspace；`services/`+`utils/`+`models/` 中纯逻辑整体搬入 `opx-core` | 桌面版**行为不变**；`cargo test --lib` **331 全绿**；能正常 `tauri build` |
| **2** | 引入 `EventSink`；解耦 12 个 tauri 服务 + 命令层用例化 | 同上（仍只跑桌面壳验证） |
| **3** | 新 `opx-http`（axum + WS + 鉴权）；前端 transport 抽象；SettingsPage 加 web 配置组 | 浏览器可用**全部**功能；局域网设备可访问；桌面窗口与浏览器**并存无冲突**；token 校验生效 |
| **4** | `opx-server` 独立 bin（Linux headless） | Linux 容器跑通 + 真实安装一个软件（ADR §6 #5） |

**每阶段硬门槛**：桌面版功能与数据（`installed.json`/`settings.json`/`apps/`/`logs/`）**逐项不变**。

---

## 5. 风险与对策

| 风险 | 对策 |
|---|---|
| 双入口状态覆盖 | 单进程唯一 `Arc`（§3.1）——从架构上消除 |
| 大搬迁回归 | 每阶段以现有 331 测试 + 桌面版行为为验收基线 |
| WS 断线丢事件 | 前端重连 + 事件回调后触发一次全量刷新兜底 |
| 端口冲突 | 可配置端口；启动失败给出明确错误而非静默 |
| token 泄漏 | 受限写盘 + 日志脱敏 + URL fragment 传入 |
| 🚨 局域网 HTTP 明文 | 开启局域网时 UI 明确风险提示；后续加自签 TLS / IP 白名单 / 限网段 |
| 抽象层调试变难 | 壳层只做 I/O；逻辑错误仍可在 `opx-core` 单测覆盖 |

---

## 6. 技术选型（含本机可行性依据）

> 本机 **crates.io 不可达**（ADR 记录：直连/代理均失败），新依赖**必须**已在 `~/.cargo/registry/cache`。已核实缓存可用：

| 用途 | crate | 缓存版本 |
|---|---|---|
| HTTP 框架 | **axum** | `0.8.9` ✅ |
| 异步 WebSocket | **tokio-tungstenite** | `0.29.0` ✅ |
| 异步运行时 | tokio（已有，features `rt-multi-thread,macros,net,time`） | ✅ |
| 中间件/服务抽象 | tower | `0.5.3` ✅ |
| 底层 HTTP | hyper | `1.11.1` ✅ |
| 随机 token | rand | `0.9.x` ✅ |
| 常量时间比较 | subtle | `2.6.1` ✅ |
| 编码 | base64 | `0.22.1` ✅ |
| 静态资源 Content-Type | mime_guess | `2.0.5` ✅ |

**注意**：加入 axum 后需确认 tokio 的 feature 足够（可能需补 `sync`）；用 `cargo check --offline` 验证后再落盘 `Cargo.lock`。

---

## 7. 已决事项（2026-10-09 拍板）

| # | 问题 | 决定 |
|---|---|---|
| 1 | 远程 / 局域网访问 | ✅ **首期支持局域网**——`web_lan_access` 开关（默认关），开启后绑 `0.0.0.0`；鉴权必须为静态 token（§3.4）。（**2026-10-12 修订**：默认关仅适用桌面；headless 首启默认开，见 `2026-10-12-installer-bootstrap-design.md` §3） |
| 2 | 默认端口 | ✅ **`17580`**（已避开 opx 管理软件全部端口，本机空闲） |
| 3 | 「打开浏览器」入口 | ✅ **设置页**（web 配置组所在） |
| 4 | token 轮换 | ✅ **手动「重置 token」按钮 + 端口 / 开关变更时自动重生成** |
| 5 | SSE 降级 | ✅ **暂不做**——首期只 WebSocket，按需再加 |

**剩余开放项**（实现期再定，不阻塞设计）
- 局域网场景是否 / 何时引入自签 TLS 或 IP 白名单。
- 端口冲突时的行为（自动 +1 重试 vs 明确报错）。
- token 重置后既有浏览器会话如何失效（前端 401 处理路径）。
