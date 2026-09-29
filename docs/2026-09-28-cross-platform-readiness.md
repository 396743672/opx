# opx 跨平台就绪度评估与架构决策记录（ADR-2026-09-28）

> 状态：草稿（问题清单待代码审计后追加，见第 7 章）
> 范围：下载 / 安装 / 运行全功能跨平台；Tauri vs Electron 选型；Linux web 管理模型
> 结论先行：**opx 是 Windows 原生工具，跨平台的真实工程量 100% 在 Rust 后端（catalog/platform 维度、per-OS 安装运行、服务管理抽象、运行时目标 OS 选择），与壳层无关；Linux 若走 web 管理，则"渲染一致性"不再构成换 Electron 的理由，Tauri 更贴合。**

---

## 1. 背景与决策驱动力

- 当前 opx 完全基于 Windows 开发与运行：隐藏窗口进程（`CREATE_NO_WINDOW`）、锁屏凭据 `CRED_PERSIST_LOCAL_MACHINE`、JVM agent 注入优雅停止，均为 Windows 专属逻辑。
- 近期讨论收敛出三点事实：
  1. 开发环境占用大（Rust 冷编译 / `target/` 巨树）—— 真痛点，但属"开发期摩擦"。
  2. 管理的软件本身也大多是 Windows 应用 —— opx 是"Windows 软件管理器"，不是"跨平台运维控制台"。
  3. Linux 若存在，倾向以 **web 管理**模型：opx 跑 headless 守护进程，用户在浏览器管。

## 2. 实证：当前下载/安装是否按系统选择

### 2.1 数据模型（无 platform 维度）

`src-tauri/src/models/software.rs`：

```rust
pub struct CatalogVersion { pub version: String, pub mirrors: Vec<MirrorSource>, pub archive: ArchiveInfo }
pub struct MirrorSource   { pub name: String, pub url: String, pub builtin: Option<BuiltinInfo> }
pub struct InstallParams  { key, version, pub mirror_index: usize, set_as_default_jre }
```

- `MirrorSource` 只有 `name/url`，**无任何 `os`/`arch`/`platform` 字段**。"镜像"是不同下载源，不是不同系统的包。
- `InstallParams.mirror_index: usize` 是单索引，无 OS 维度 —— 同一版本只能选一个 URL。

### 2.2 各 provider 实测结论

| provider | 是否按系统选 | 证据 |
|---|---|---|
| **JDK / JRE** | ✅ 已按系统选 | `jdk.rs:41-46`、`jre.rs:70-75`：`fetch_remote_versions` 运行时查 Adoptium API，带 `os`+`arch`（`cfg!(target_os)` → windows/mac/linux，aarch64/x64）。Linux 构建会拉 linux 包。**JDK/JRE 不是缺口。** |
| **MySQL** | ❌ Windows-only | `mysql.rs:16` impl 级 `#[cfg(windows)]`；`:58` 版本列表被 `cfg(windows)` 包住（非 Windows 构建目录里**无 MySQL**）；`:68/:73/:117` 硬编码 `winx64.zip`；`:175/:233` 写死 `bin/mysqld.exe`；`:88-91` 注释自承"本设计 Windows-only，Unix 须单独适配"。 |
| **静态目录型**（Redis / PostgreSQL / Nginx / MongoDB / Consul / Nacos / Kafka / MinIO / Elasticsearch / InfluxDB / InfluxDB3 / RustFS / Node） | ❌ 缺 platform 维度 | 写死 mirror URL（或 `RESOURCE_BASE` 内置 winx64），catalog 模型无法声明"此 URL 是 Windows 包、彼为 Linux 包"。现状两种处理：① `#[cfg(windows)]` 编译期分支 → Linux 构建直接没有该条目；② URL 硬编码 winx64。**`utils/download.rs` 零 OS 逻辑，只下载给定 URL。** |
| **Elasticsearch** | ⚠️ 已示范 per-OS 分支写法 | `elasticsearch.rs:76-91,218` 用 `#[cfg]` 做了 per-OS 子目录/二进制名分支 —— 证明"逐 provider 补多系统"这条路通，但仍是编译期烤死。 |

## 3. 更深的结构性缺口（不止 catalog）

### 3.1 catalog 模型无 platform 维度
单 provider 无法声明多系统包；需给 `CatalogVersion`/`MirrorSource` 加 `os`/`arch`，或增加"运行时按 host 选 URL"的 provider hook。

### 3.2 安装/运行逻辑 Windows 写死
- `lifecycle.rs` 仅有 Windows 的 `CREATE_NO_WINDOW`；`installer.rs` 有 `app.exe` 兜底。
- **缺统一服务管理抽象**（Windows 服务 / systemd / launchd）：自启、看门狗、服务注册全缺失（代码库 grep `systemd`/`launchd`/`sc `/`net start` 无命中）。Linux 的自启/看门狗/服务注册无从落地。

### 3.3 `cfg!(target_os)` 是编译期、单主机假设
- opx 是原生二进制 + 单主机模型：跑 opx 的机器 = 被管软件所在机器。`cfg!` 编译时定死 OS，对"本机管本机"正确。
- 一旦 Linux web 管理隐含**混合机群**（一台 Linux opx-server 管多台 Windows/Linux 目标），`cfg!` 便**错** —— 需运行时按目标主机 OS 选包，catalog 变动态。当前架构不支持此模型。

## 4. 选项与权衡

### 选项 A：留 Tauri + 新增 headless web 服务（推荐）
```
opx-core (Rust lib)  ← 下载/安装/运行/凭据/provider，平台无关逻辑
  ├─ opx (Tauri 桌面壳)      → Windows / macOS 原生窗口
  └─ opx-server (headless)  → Linux：内嵌 HTTP 服务，复用现有 Vue SPA
```
- Vue 前端只构建一次，Tauri 与 headless server 共用，零重复。
- 渲染一致性：Linux 走**用户浏览器**（Chrome=Chromium，与 Windows WebView2 同内核），比 Tauri 桌面壳（Linux 用 WebKitGTK）更一致。
- **Electron 的 Chromium 在 Linux-web 场景纯负担**：不在 Electron 窗口渲染，而在用户浏览器渲染。
- 真成本在后端：catalog 加 platform、逐 provider 补 per-OS 安装运行、新增服务管理抽象、混合机群时把 `cfg!` 改运行时选择。

### 选项 B：翻写 Electron
- 必须整盘 TS 重写 Rust 后端（凭据/进程/JVM/provider）。
- 锁屏凭据退化为 DPAPI per-user 或 `safeStorage`（**Linux 无 libsecret 时静默回退 `basic_text` 明文等价**），失去刚修好的 `LOCAL_MACHINE` 保证。
- 运行时多 150–250 MB + 一个 Chromium；特权主进程变完整 Node，供应链爆炸半径变大。
- 收益点（渲染一致）在 Linux-web 模型下消失。

## 5. 结论

1. opx 是 Windows 原生工具；Linux 若走 web 管理，"渲染一致"不再构成换 Electron 理由，**Tauri 更贴合**。
2. 跨平台真实工程量 100% 在 Rust 后端，与壳无关，**换框架不减这笔账**。
3. 唯一真痛点（Rust 冷编译 / 开发环境占用）用 `sccache` + 裁剪无用 crate + CI 缓存缓解，不靠重写。
4. 选 A 时，当前"已合并、已修完锁屏、能跑"的功能集不推翻；增量加 `opx-server` + catalog/platform + 服务管理抽象。

## 6. 达到"完全跨平台"的最小改动清单

| # | 改动 | 涉及 |
|---|---|---|
| 1 | 数据模型加 `os`/`arch` 维度（`CatalogVersion`/`MirrorSource` 或 provider 运行时选 URL hook） | `models/software.rs`、`catalog.rs` |
| 2 | 逐 Windows-only provider 补 per-OS 子目录名、二进制名（`mysqld` vs `mysqld.exe`）、配置文件（`my.cnf` vs `my.ini`） | `providers/*` |
| 3 | 新增统一服务管理抽象层（systemd / launchd / Windows-service） | 新 `services/service_mgr/` |
| 4 | 若做混合机群：把 `cfg!(target_os)` 编译期选择改为运行时按目标主机 OS 选择 | `providers/*`、`catalog.rs` |
| 5 | 端到端验证：Linux 容器跑 headless web 服务 + 真实下载/安装一个跨平台软件（如 PostgreSQL） | 新 `opx-server` + CI |

## 7. 其他问题清单（全仓代码审计，2026-09-28）

> 由 `Explore` 全仓审计产出，逐条带 `file:line` 证据。标记【已核实】= 主理人复核原文确认；【审计】= 探索代理读文件确认，未单独复核。
> 总体结论：**opx 实质是 Windows-only**——catalog 无 platform 维度，几乎所有 provider 用 `#[cfg(windows)]` 在编译期写死版本；本可跨平台的 PostgreSQL/MongoDB/Consul/MinIO 还把二进制名硬编码 `.exe`（双重锁定）。进程管理、错误处理、凭据安全存在若干确认 bug/缺口；命令注入与图标覆盖是干净的。

### 7.1 P1（建议优先修）

| # | 问题 | 证据 | 影响 | 建议 |
|---|---|---|---|---|
| P1-1 | **本可跨平台却被写死（双重锁定 Windows-only）** | `postgresql.rs:204/214` `initdb.exe`/`postgres.exe`（catalog `#[cfg(windows)]`:89）；`mongodb.rs:186` `mongod.exe`（:95）；`consul.rs:167` `consul.exe`（:95）；`minio.rs:284/286` `minio.exe`/`silo.exe`（:152） | 只放开 catalog 的 `#[cfg(windows)]` 不够，Unix 上会因 `.exe` 启动失败；上游跨平台却不支持 | 各 provider 补 per-OS 二进制名，见第 6 章清单 #2 |
| P1-2 | **启动竞态（TOCTOU）双 spawn** | `commands/software.rs:609` `validate_start_transition` 在请求入口校验，但状态翻成 `Starting` 发生在 spawned 异步任务内的 `:1320-1327`；中间还跑 `ensure_dependencies` + `first_run_init`（最长阻塞 180s，`lifecycle.rs:173`）。两者不处同一把锁 | 并发两次 start 同一实例可都通过校验、都 spawn → 双进程、端口冲突、孤儿 | 在持有状态锁内原子地完成"校验+置 Starting" |
| P1-3 | **Windows 优雅停止失效 → DB 被强杀** | `lifecycle.rs:417-459`：对控制台子进程 `taskkill /PID /T`（不带 `/F`）被系统拒绝（注释 L418-423 自承），1.5s 后强制 `/F`。MySQL/PostgreSQL/MongoDB 被强杀有数据损坏/未落盘风险；按语义优雅停止（mysqladmin/redis-cli/nginx -s quit）列为"后续项" | 停止即强杀，数据风险 | 补各软件语义化优雅停止命令（同 SpringBoot agent 注入思路） |
| P1-4 | **Nacos 硬编码 token 密钥（明文凭据泄漏）**【已核实】 | `nacos.rs:272` 把 `nacos.core.auth.plugin.nacos.token.secret.key=VGhpc0lzTXlDdXN0b21TZWNyZXRLZXkwMTIzNDU2Nzg=`（即 `ThisIsMyCustomSecretKey012345678`）写死进启动参数；`nacos.rs:743` 测试还断言它存在 | 任何能读源码/二进制的人都拿到 Nacos token 签发密钥，可伪造 token | 随机生成并安全存储（接入锁屏凭据库） |

### 7.2 P2

| # | 问题 | 证据 | 影响 | 建议 |
|---|---|---|---|---|
| P2-1 | catalog 无 platform 维度 + 编译期 `#[cfg(windows)]` 全锁 | `models/software.rs:33-38` `CatalogVersion.mirrors` 无 os/arch；`influxdb.rs:79`/`elasticsearch.rs:76`/`nacos.rs:103` 等 versions 全被 `#[cfg(windows)]` 包住 | 非 Windows 构建整张目录为空（除 jdk/jre/kafka 动态拉取） | 见第 6 章 #1 |
| P2-2 | 其余 `.exe` 硬编码 + 自定义模板 Windows-only 默认值 | `mysql.rs:175/233` `mysqld.exe`；`redis.rs:199` `redis-server.exe`（注释 L91 Unix 不同）；`nginx.rs:222` `nginx.exe`；`rustfs.rs:253`；`custom_templates.rs:14/21` 默认 `redis-server.exe`/`nginx.exe` | 非 Windows 用户创建自定义软件默认值错误 | per-OS 默认 + 自定义模板按 OS 给默认 |
| P2-3 | 全局 Mutex `.lock().unwrap()` 中毒级联 | `lifecycle.rs:64/68/72/76`、`process_monitor.rs:59/64/95`、`stack_manager.rs` 多处、`node_app_manager.rs` 多处、`software_manager/mod.rs:133/138/150`、`log_watcher.rs`、`ddns/mod.rs:338` | 任一线程 panic 持锁即中毒，后续 `.unwrap()` 直接 panic 级联崩溃 | 统一 `.lock().unwrap_or_else(\|e\| e.into_inner())` 自动恢复 |
| P2-4 | PID 跟踪对 `.bat` 包装 Java 应用失真（待验证） | `commands/software.rs:1317` `child.id()`；Windows 上 elasticsearch/kafka/nacos 经 `.bat` 启动，cmd.exe 可能先退出 → 误判 stopped + 孤儿 JVM（Unix 用 `.sh` + exec java 正常） | Windows 上 Java 类软件状态误判 | Windows 实机验证；改用 job object / 跟踪子进程树 |
| P2-5 | `stop_all_on_exit` 不尊重依赖拓扑 | `lifecycle.rs:614-639` `drain()` 后按 HashMap 顺序逐个 `stop_one`，无 `startup_order`/拓扑排序 | 依赖方可能在依赖之前被杀 | 按 `depends_on`/启动顺序逆序停止 |
| P2-6 | 明文凭据持久化 | `minio.rs:261/322` `secret_key`（默认 minioadmin）；`rustfs.rs:226/323`（默认 rustfsadmin）；`nacos.rs:291/302/508` `mysql_password` 明文；`influxdb.rs:280` 回写 `admin_token`；`influxdb3.rs:225-230/309` 写 `admin-token.json` 于数据目录 | 敏感字段落盘，权限不当可被读 | secret 类默认 `ephemeral` 不写盘，或接入 OS 凭据库；至少收紧文件权限 |
| P2-7 | 默认弱口令 | MinIO `minioadmin/minioadmin`、RustFS `rustfsadmin` 默认若不改即全开 | 默认凭据暴露 | UI 强制首次改密或随机生成 |
| P2-8 | 配置 schema 不一致 | jdk/jre catalog 静态 versions 为空（依赖动态拉取）而 node 是静态 win-x64（`node.rs:71-95`）；各 provider 对"secret 是否落盘"处理不统一 | 跨软件行为不一致，易踩坑 | 统一 schema 约定（参考 MySQL/PostgreSQL 的 `ephemeral_keys`） |
| P2-9 | i18n 真实孤儿键（待验证） | 静态扫描 `zh-CN.ts`/`en-US.ts` 有约 237 个 data-driven 假阳性（`configField.*`/`catalogDesc.*`/`template.*` 被 Rust 引用）；字面 `$t('key')` 零引用里可能有少量真孤儿（如 `unknownError`/`expectedStatus`/`forceUninstall`/`saveWithoutRestart`/`restoreBackupConfirm`/`builtinCorrupted`/`builtinMissing`） | 可能有死文案 | 跟随 data-driven 引用逐键追溯，勿直接删 |

### 7.3 死代码（P2 已确认）

- **`TempSecretSpec` 枚举 + `FirstRunInit.temp_secret_output` 字段是死代码**：全仓 grep `temp_secret_output: Some` / `TempSecretSpec::` 零命中，所有 provider 都写 `None`（`mysql.rs:241`、`postgresql.rs:222`、`nacos.rs:363`、`kafka.rs:327`、`consul`、`influxdb3.rs:358` 等）。`TempSecretSpec::FromStdoutRegex`/`FromLogFile` 从未被构造——临时密码从输出/日志提取的能力形同虚设（MySQL 临时密码实际未读取，`commands/software.rs:1282` 注释 `// 暂不使用 stderr`）。建议：要么实现提取逻辑，要么删掉这套未启用类型。
- 彻底死代码扫描建议在 CI 加 `#![deny(dead_code)]` 或编译后读警告（lib 的 `pub` 项默认豁免 dead_code lint）。

### 7.4 正面（无需处理）

- **无命令注入**：全仓无 `shell(true)` / `cmd /c` / `sh -c` 拼接用户输入；子进程均 `Command::new(prog).args(vec![...])` 逐参传递；`build_custom_command` 做了相对路径白名单 + 禁 `..` + 禁绝对路径（`lifecycle.rs:346-371`）。
- **图标覆盖完整**：`scripts/gen-icons.mjs` 生成 `mdi-icons.json`（126 图标），前端 119 + `src-tauri/src` 12 个 `mdi:` 用法全部命中，零缺失（早前 `mdi:storage` 缺口是提取正则假阳性）。
- **测试内 `unwrap` 正常**：`software.rs`/`lock_screen.rs`/`log_viewer.rs` 中大量 `.unwrap()` 均在 `#[cfg(test)]` 辅助里，非生产路径。
- **`is_process_alive` 高效**：用单 PID 刷新（`health_check.rs:113-118`），非 `refresh_all`。

### 7.5 优先级汇总

- **P1（确认/需修）**：①PG/Mongo/Consul/MinIO 双重锁定 Windows-only；②start 竞态双 spawn；③Windows 优雅停止失效→DB 强杀风险；④Nacos 硬编码 token secret key。
- **P2**：catalog 无 platform 维度 + 编译期全锁；其余 `.exe` 硬编码 + 自定义模板 Windows-only 默认；全局 Mutex `.unwrap()` 中毒；PID 跟踪(.bat)失真(待验证)；stop_all 无拓扑；明文凭据持久化；默认弱口令；i18n 真孤儿键(待验证)；jdk/jre 与 node schema 不一致；TempSecretSpec 死代码。
- **P3/正面**：图标覆盖完整；无命令注入；测试 unwrap 正常；is_process_alive 高效。

> 注：死代码与 i18n 两项的"全仓彻底"结论依赖编译期检查（本环境未编译，故标"疑似需验证"而非断言）。

### 7.6 P1 修复进度（fix/audit-p1 分支）

| # | 状态 | 修复方案 | commit |
|---|---|---|---|
| P1-2 | ✅ 已修复 | `start_software` 入口在写锁内原子完成「校验 + 置 Starting + 清 last_error + 落盘」（`SoftwareManager::try_reserve_start` + `lifecycle::reserve_start_in_list`），消除并发双 spawn 竞态 | `e7b21cd` |
| P1-4 | ✅ 已修复 | Nacos token 密钥改为每实例随机 32 字节 base64，落盘 `conf/opx-token-secret.key`（真实安装目录），缺失则临时生成；移除硬编码 `ThisIsMyCustomSecretKey012345678` | `f2c0fd4` |
| P1-3 | ✅ 已修复 | provider trait 新增可选 `graceful_stop_command` 钩子（默认 None，其余 14 个 provider 零改动）；Redis/Nginx/PostgreSQL/MongoDB 实现各自的语义化关闭命令（redis-cli shutdown / nginx -s quit / pg_ctl stop -m fast -w / mongod --dbpath … --shutdown）。`stop_software`/`restart_software` 先执行优雅停止命令、失败/超时回退 `taskkill /F` 强杀 | 本次 |
| P1-1 | 🔧 起步 | per-OS 二进制名已完成（共享 `exe_name` 助手，PG/Mongo/Consul/MinIO 启动+优雅停止不再硬编码 `.exe`，Windows 行为不变）；catalog/版本发现的 `#[cfg(windows)]` 锁定与 per-OS 下载 URL 仍留待 P2-1（catalog 平台维度）一并处理，避免 Linux 上「列得出装不了」破窗 | `fe01c1b` |

**P1-3 已知例外（MySQL）**：MySQL root 密码为一次性 `ephemeral`（不持久化到 installed.json / 配置文件），`mysqladmin shutdown` 无法认证，`graceful_stop_command` 故意返回 `None` 走强杀回退——否则会陷入「认证失败 → 强杀」的假优雅。若要彻底优雅停止 MySQL，需将 root 密码存入 OS 凭据库（同锁屏 `LOCAL_MACHINE` 思路），属独立设计项，不在本次 P1-3 范围。

> 实机验证建议（待用户本机）：装一个 Redis + 一个 PostgreSQL，分别点「停止」，观察 `opx-<id>.log` 不再出现强杀、进程干净退出；并在数据有写入时停库，重启确认数据未损坏。

### 7.7 P2 修复进度（fix/audit-p2 分支）

| # | 状态 | 修复方案 | commit |
|---|---|---|---|
| P2-3 | ✅ 已修复 | 全局 50 处 `Mutex.lock().unwrap()` 改为 `.unwrap_or_else(\|e\| e.into_inner())`，任一线程 panic 持锁不再级联中毒崩溃（覆盖 lifecycle/process_monitor/stack_manager/node_app_manager/software_manager/mod/log_watcher/ddns/startup_bootstrap/system_monitor/info/download 共 10 文件） | `f0cc561` |
| P2-5 | ✅ 已修复 | `RegisteredProcess` 记录 `startup_order`；`stop_all_on_exit` 按 `startup_order` 逆序停止（依赖方先于依赖被杀），抽出可测纯函数 `sort_by_shutdown_order` 并加回归测试 | `f0cc561` |
| P2-1 | ✅ 架构就绪 + PG 示范 | 新增 `utils/platform.rs` 运行时 OS 分发（`current_os()`），替代编译期 `#[cfg(windows)]` 锁；PostgreSQL 作示范：`archive_url_for(version, os)` 三平台 URL/格式（EDB linux/macOS/windows）、`parse_supported_versions` 去 cfg（纯解析与 OS 无关），catalog/版本发现改运行时分发，Windows 行为不变。其余 13 个 provider 待逐软件补 per-OS URL + 服务管理抽象（第 6 章 #1~#5） | 本次 |
| P2-2 | ⏳ 未修 | 其余 `.exe` 硬编码（mysql/redis/nginx/rustfs/custom_templates）按 OS 给默认 | — |
| P2-4 | ⏳ 未修 | PID 跟踪对 `.bat` 包装 Java 应用失真（Windows 实机验证；job object/子进程树） | — |
| P2-6 | ✅ 已修复 | 敏感文件落盘收紧权限：installed.json（覆盖所有 provider config secret）、nacos token 密钥文件、influxdb3 admin-token.json；Unix 0600 / Windows 只读位（真正 owner-only ACL 留 P2-1）。5 个 secret 均因重启一致性不可 ephemeral，故采用「落盘即收紧权限」。**补修**：Windows 只读位会阻断覆盖写（installed.json 每次启停重写、influxdb3 token 每次启动重写），抽出 `write_file_restricted`（清只读 → 写 → 收紧）统一三个落点，加重写回归测试 | `9985508` |
| P2-7 | ⏳ 未修 | 默认弱口令（MinIO/RustFS 强制首次改密或随机生成） | — |
| P2-8 | ⏳ 未修 | 配置 schema 不一致（统一 secret 落盘约定） | — |
| P2-9 | ⏳ 未修 | i18n 真实孤儿键（逐键追溯，勿直接删） | — |

> P2-3 / P2-5 已在 `fix/audit-p2` 分支提交并通过 `cargo test --lib`（318 passed / 0 warning）+ 全量 `cargo build`（0 warning）；**尚未合并 dev**（按工作流等「合并到 dev」指令）。

