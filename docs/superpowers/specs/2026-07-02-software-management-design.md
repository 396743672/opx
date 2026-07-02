# 软件管理模块设计文档

**日期**: 2026-07-02
**项目位置**: `D:\object\opx`
**技术栈**: Rust + Tauri 2 + Vue 3 + TypeScript + Tailwind CSS + Monaco Editor
**阶段**: 第三阶段 — 软件管理（启停 / 配置 / 卸载校验）

## 概述

软件管理模块负责"对软件仓库已安装的软件进行启动 / 停止 / 重启 / 配置编辑 / 安全卸载"。本阶段聚焦运行时管控，与上一阶段"软件仓库"（下载/解压/登记）衔接，与未来"应用管理"（Spring Boot）通过预留接口协作。

### 范围边界

**包含**：
- 已安装软件的启动 / 停止 / 重启（服务模式，隐藏控制台窗口）
- MySQL 首次启动 `--initialize-insecure` 数据目录初始化 + 临时/无密码策略
- MinIO / RustFS 首次启动配置（API 端口、控制台端口、数据目录、access/secret key）
- 启动状态实时刷新（`Running` / `Stopped` / `Error` / `Unknown` / `Starting` / `Stopping`）
- 配置编辑：表单 + 源码双 tab（Monaco 编辑器）
- 卸载前置校验（运行中拒绝；JRE 依赖检查）
- 操作审计日志（全量，按日轮转）
- 自定义软件启动命令配置 + 模板预设
- 应用启动时按 `startup_order` 拉起 `auto_start=true` 的实例
- Portable 路径策略（配置文件相对路径 + `current_dir`）

**不包含**：
- Spring Boot 应用启停（属 `springboot-manager` 独立设计）
- 在线升级（Nginx 等版本更新操作，留到下阶段）
- 软件仓库的下载/安装（已在上一阶段完成）
- 前端测试框架（与现有项目一致，靠手动验证）

### 关键约束（用户确认）

1. **配置编辑形态**：C 方案 — 表单 + Monaco 双 tab，切换时强制重读盘避免脏状态
2. **进程持久化策略**：D 方案 — 默认退出即停，`auto_start_on_app_start=true` 的实例持久化 PID 并在 OPX 启动时按 `startup_order` 拉起
3. **健康检查**：C 方案 — `SoftwareProvider` trait 自定义 `health_check()` 钩子，每个软件按自身语义探测
4. **卸载校验**：D 方案 — 运行中拒绝 + JRE 依赖检查（不做端口兜底）
5. **Portable 路径**：A 方案 — 配置文件全部用相对路径，启动时 `Command::current_dir(working_dir)`
6. **多实例启停**：A + C 简化版 — 每实例独立启停（`installed_id` 主键），UI 按 key 排序连续显示
7. **自定义软件**：C + C2 — 启动命令模板预设 + 用户可填配置文件路径以启用源码编辑
8. **操作日志**：C 方案 — 全量审计到 `logs/software-manager.log`，按日轮转，保留 7 天
9. **Spring Boot 整合**：D 方案 — 本次不含但预留接口，JRE 卸载校验查依赖

## 架构与模块边界

### 后端分层

遵循现有 `commands/ services/ models/ utils/` 分层。

```
src-tauri/src/
├── commands/software.rs              # 扩展：启停 / 配置 / 卸载校验命令
├── services/software_manager/        # 业务服务层（既有子目录扩展）
│   ├── mod.rs                        # SoftwareManager：编排（已有，扩展方法）
│   ├── catalog.rs                    # 已有
│   ├── installer.rs                  # 已有
│   ├── lifecycle.rs                  # 新增：启停状态机 + PID 跟踪 + auto_start 拉起
│   ├── config_editor.rs              # 新增：配置文件读写 + 表单 schema 调度
│   ├── health_check.rs               # 新增：健康检查调度（TCP/HTTP/Custom）
│   ├── uninstall_guard.rs            # 新增：卸载前校验（运行中 + JRE 依赖）
│   ├── audit_log.rs                  # 新增：操作日志（tracing + daily rolling）
│   └── providers/                    # 扩展现有
│       ├── mod.rs                    # trait 扩展 start_command/stop_command/health_check/config_schema/working_dir
│       ├── custom_templates.rs       # 新增：自定义软件启动命令模板
│       ├── mysql.rs / redis.rs / nginx.rs / minio.rs / rustfs.rs / jre.rs
├── models/software.rs                # 扩展：SoftwareStatus 新增 Starting/Stopping，InstalledSoftware 新增运行时字段
└── utils/                            # 复用现有 paths / archive / download
```

### 前端结构

```
src/modules/software-manager/
├── pages/
│   └── SoftwareListPage.vue          # 改造：分组渲染 + 启停/配置/卸载集成
├── components/
│   ├── SoftwareInstanceRow.vue        # 新增：单实例行（启停/配置/卸载按钮 + 状态徽章）
│   ├── StartStopButton.vue            # 新增：启停状态切换按钮
│   ├── StatusBadge.vue                # 新增：状态徽章
│   ├── ConfigEditDialog.vue           # 新增：配置编辑对话框（双 tab 容器）
│   ├── ConfigFormTab.vue              # 新增：表单 tab
│   ├── ConfigSourceTab.vue           # 新增：源码 tab（Monaco）
│   ├── CustomStartCommandDialog.vue   # 新增：自定义软件启动命令配置
│   ├── StartupSettingsDialog.vue      # 新增：auto_start / startup_order 设置
│   ├── JreDependentsDialog.vue        # 新增：JRE 卸载依赖列表展示
│   ├── SoftwareCard.vue               # 既有（仓库页用）
│   ├── InstallDialog.vue / InstallJreDialog.vue / InstallProgressDialog.vue / CustomInstallDialog.vue  # 既有
│   └── UninstallConfirmDialog.vue     # 既有（改造：显示阻止原因）
└── stores/
    ├── catalog.ts / install.ts        # 既有
    └── lifecycle.ts                   # 新增（启停状态 store）
```

### Provider trait 扩展

```rust
pub trait SoftwareProvider: Send + Sync {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;
    fn fetch_remote_versions(&self) -> Option<Vec<CatalogVersion>> { None }

    // 新增：本阶段核心钩子
    fn start_command(&self, ctx: &StartContext) -> Result<StartCommand>;
    fn stop_command(&self, ctx: &StopContext) -> Result<Option<StopCommand>> { Ok(None) }
    fn health_check(&self, ctx: &HealthContext) -> HealthCheckSpec;
    fn config_schema(&self) -> Option<ConfigSchema> { None }
    fn config_file_path(&self, ctx: &ConfigContext) -> Option<PathBuf>;
    fn working_dir(&self, ctx: &WorkingDirContext) -> PathBuf;
}
```

## 数据模型

### SoftwareStatus 扩展

```rust
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
    Starting,      // spawn 已下发，健康检查未通过
    Stopping,      // 停止命令已下发，进程未完全退出
    Initializing,  // 首次启动初始化中（MySQL --initialize 等）
}
```

### InstalledSoftware 扩展

```rust
pub struct InstalledSoftware {
    // 既有字段
    pub id: String,
    pub key: String,
    pub version: String,
    pub name: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
    pub source: InstallSource,

    // 新增：运行时追踪
    pub pid: Option<u32>,
    pub last_started_at: Option<NaiveDateTime>,
    pub last_stopped_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,

    // 新增：自定义软件启动命令
    pub custom_start_command: Option<CustomStartCommand>,
}

pub struct CustomStartCommand {
    pub executable: String,          // 相对 install_path
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub health_check: CustomHealthSpec,
    pub config_file_relative: Option<String>,  // 填写后启用源码编辑 tab
}

pub enum CustomHealthSpec {
    None,
    Tcp { port: u16 },
    Http { url: String, expected_status: u16 },
}
```

### ConfigSchema（表单字段声明）

```rust
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
}

pub struct ConfigField {
    pub key: String,
    pub label_i18n: String,
    pub field_type: ConfigFieldType,
    pub default_value: serde_json::Value,
    pub section: Option<String>,        // 配置文件段落，None=顶层
    pub description_i18n: Option<String>,
}

pub enum ConfigFieldType {
    Text,
    Number,
    Port,
    Select { options: Vec<String> },
    Password,
}
```

### HealthCheckSpec

```rust
pub enum HealthCheckSpec {
    ProcessOnly,
    Tcp { port: u16, timeout_ms: u64 },
    Http { url: String, expected_status: u16, timeout_ms: u64 },
    Custom { checker: fn(&HealthContext) -> bool },
}
```

### 卸载校验返回

```rust
pub struct UninstallSafetyReport {
    pub safe: bool,
    pub blockers: Vec<UninstallBlocker>,
}

pub struct UninstallBlocker {
    pub kind: String,        // "running" / "jre_default_in_use" / "jre_app_dependent"
    pub message_i18n: String,
    pub dependents: Vec<JreDependent>,
}

pub struct JreUsageReport {
    pub in_use: bool,
    pub is_default: bool,
    pub dependents: Vec<JreDependent>,
}

pub struct JreDependent {
    pub kind: String,        // "springboot-app"
    pub id: String,
    pub name: String,
    pub status: String,
}
```

### 进程注册表（lifecycle 内）

```rust
pub struct ProcessRegistry {
    processes: HashMap<String, RegisteredProcess>,  // key = installed_id
}

pub struct RegisteredProcess {
    pub installed_id: String,
    pub pid: u32,
    pub name: String,
    pub key: String,
    pub kind: String,
    pub started_at: DateTime<Local>,
}
```

旧的 `services/process_registry.rs` 保留不动（被 `stop_all` 退应用时调用），新注册表是软件管理专用。退应用时调用 `lifecycle::stop_all_managed()` 遍历新注册表逐个停止。

### 持久化文件扩展

`config/installed.json` 新字段（向后兼容，旧文件反序列化时新字段用默认值）：

```json
{
  "software": [
    {
      "id": "uuid",
      "key": "mysql",
      "version": "8.4.10",
      "install_path": "apps/mysql/8.4.10",
      "status": "Stopped",
      "pid": null,
      "last_started_at": null,
      "last_stopped_at": null,
      "last_error": null,
      "custom_start_command": null
    }
  ]
}
```

## 启动 / 停止 / 重启流程

### 启动流程

```
前端：点"启动"按钮
  │
  ▼ invoke start_software(installed_id)
后端 start_software 命令：
  1. 加载 InstalledSoftware（installed.json 查找 by id）
  2. 状态校验：status != Stopped && status != Error && status != Unknown
     → 拒绝：return Err("当前状态为 {status}，无法启动")
  3. PID 残留校验：若 pid 字段存在且该 PID 仍存活
     → 拒绝：return Err("进程 {pid} 仍在运行，请先停止")
  4. 派生 StartContext { install_path, version, config, custom_start_command }
  5. 若 is_custom=true：用 custom_start_command 构造 Command
     否则：调用 provider.start_command(ctx) 构造 StartCommand
  6. 首次启动校验：
     - 检查 first_run_init 字段（MySQL 需初始化 data 目录）
     - 检查 config.initialized 标记
     - 若需要初始化且未初始化：
       a. 状态 → Initializing（新中间态）
       b. emit "software-status-changed" { status: "Initializing" }
       c. 执行 first_run_init.init_command
          - 同步等待完成（最多 60s）
          - 捕获 stdout/stderr（用于抓取 MySQL 临时密码）
          - 失败 → 状态 Error，返回 Err("初始化失败：{原因}")
       d. 若有 temp_secret_output：
          - 用正则匹配 stdout/stderr 或日志文件
          - 提取临时密码，存入 config.temp_secret
       e. 更新 config.initialized = true，写回 installed.json
  7. 设置 Command::current_dir(working_dir)  ← portable 关键
  8. 设置环境变量（env_vars）
  9. Windows 设置 creation_flags = CREATE_NO_WINDOW  ← 隐藏控制台窗口
  10. Command::spawn() → 拿到 Child
      失败 → 立即返回 Err("启动失败：{原因}")
  11. 拿到 PID，更新内存注册表 + installed.json:
      - status = Starting
      - pid = Some(child_pid)
      - last_started_at = now
  12. emit "software-status-changed" { installed_id, status: "Starting", pid }
  13. 异步 tokio::spawn 健康检查任务
  14. 立即返回 Ok(())（不阻塞 IPC）
  │
  ▼ 健康检查任务
  15. 按 provider.health_check(ctx) 调度轮询：
      - Tcp: 每 1s 尝试 TcpStream::connect，最多 30 次
      - Http: 每 1s reqwest GET，期望 status code
      - None: 跳过，直接标记 Running
  16. 成功 → 更新 status = Running，emit "software-status-changed"
  17. 30 次失败 → 更新 status = Error，last_error = "健康检查超时"
      emit "software-status-changed" { status: "Error", error }
      保留 PID（供用户手动排查/强杀）
  │
  ▼ 进程退出监听（并行 spawn）
  18. tokio::spawn 监听 child.wait()
  19. 若进程在 status=Running 时退出（非用户主动停止）
      → status = Error, last_error = "进程意外退出，code={x}"
      → emit "software-status-changed"
      → 从注册表移除 PID
```

启动是异步的——`start_software` IPC 命令立即返回，状态变化通过事件推送。`Starting` 状态下 UI 显示加载动画。健康检查超时不会自动杀进程（保留 PID 供排查），用户可手动点"停止"。

### 首次启动初始化（MySQL 专属）

MySQL 首次启动前必须用 `mysqld --initialize` 初始化数据目录，否则 mysqld 启动会因 datadir 为空报错。

| 初始化方式 | 临时密码策略 | 本设计选择 |
|---|---|---|
| `--initialize-insecure` | root@localhost 无密码 | ✅ 默认，UI 引导用户后续设密码 |
| `--initialize` | 生成随机密码写入 stderr | 备选，存入 config.temp_secret |

**初始化时机**：仅在 `config.initialized != true` 时执行。执行成功后置 `initialized = true` 持久化到 installed.json，下次启动跳过。

**初始化阶段状态**：新增 `Initializing` 中间态（仅启动流程中短暂出现，UI 显示"初始化数据目录…"），完成后转 `Starting`。

**初始化失败的回滚**：
- 删除半成品 `data/` 目录
- 不更新 `config.initialized`
- 状态回退到 Stopped + last_error

### 首次启动配置（MinIO / RustFS）

MinIO 与 RustFS 不需要数据目录初始化（首次启动自动创建），但需要用户配置启动参数：

| 配置项 | MinIO 默认 | RustFS 默认 |
|---|---|---|
| API 端口 | 9000 | 9000 |
| 控制台端口 | 9001 | 9001 |
| 数据目录 | `./data` | `./data` |
| Access Key | `minioadmin` | `rustfsadmin` |
| Secret Key | `minioadmin` | `rustfsadmin` |

**配置时机**：首次点"启动"时，若 `config.configured != true`，自动弹出"首次启动配置"对话框（类似自定义软件的 `CustomStartCommandDialog`），用户填入端口、目录、密钥后保存到 `config`，再走启动流程。

**后续编辑**：通过"配置"对话框的表单 tab 修改（MinIO/RustFS 的表单 schema 字段 = 启动参数）。无源码 tab（无配置文件）。

### 停止流程

```
前端：点"停止"按钮
  │
  ▼ invoke stop_software(installed_id)
后端 stop_software 命令：
  1. 加载 InstalledSoftware
  2. 状态校验：status != Running && status != Starting && status != Error
     → 拒绝：return Err("当前状态为 {status}，无法停止")
  3. PID 校验：pid 字段必须存在
     → 拒绝：return Err("无 PID 记录，可能已停止")
  4. 更新 status = Stopping
     emit "software-status-changed" { status: "Stopping" }
  5. 调用 provider.stop_command(ctx)：
     - Some(cmd) → 用 provider 声明的停止命令（如 mysqladmin shutdown）
       执行后等待最多 10s 进程退出
     - None → 走通用 kill 流程
  6. 通用 kill 流程（复用现有 process_registry::stop_one 思路）：
     a. 优雅停止：Windows taskkill /PID, Unix kill -TERM
     b. 轮询 5s 等待退出
     c. 超时强杀：taskkill /F, kill -9
     d. 再等 300ms
  7. 更新 installed.json:
     - status = Stopped
     - pid = None
     - last_stopped_at = now
  8. 从注册表移除
  9. emit "software-status-changed" { status: "Stopped" }
  10. 写审计日志
  11. 返回 Ok(stopped: bool)  // false=强杀，true=优雅
```

### 重启流程

```
invoke restart_software(installed_id)
  1. 若 status == Running 或 Starting：
     a. 调用内部 stop_software(installed_id) 等待完成
     b. 失败则返回 Err("停止失败，重启中止：" + 原因)
  2. 调用内部 start_software(installed_id)
  3. 返回 Ok(())
```

重启是同步的——IPC 命令会等到停止完成再返回（停止本身最多 ~10s）。前端 UI 显示"重启中"加载态。

### 应用启动时 auto_start 拉起

```
Tauri setup hook：
  1. 加载 installed.json
  2. 过滤 auto_start_on_app_start == true && status in [Stopped, Unknown, Error]
  3. 按 startup_order 升序排序
  4. 逐个调用 lifecycle::start_software（内部 API，不走 IPC）
  5. 同 startup_order 的并发拉起
  6. 每个启动间隔 500ms（避免 CPU 峰值）
  7. 失败的不阻塞后续（写错误日志，标记 Error）
```

### 应用退出时 stop_all

```
Tauri on_window_event / before_exit：
  1. 遍历运行中的软件（status in [Running, Starting]）
  2. 逐个调用 stop_software（同步等待，最多 10s 每个）
  3. 写审计日志"应用退出，停止 N 个软件"
  4. 释放注册表
```

替换原 `process_registry::stop_all` 的职责。原 `process_registry.rs` 仍保留作为兜底。

## 健康检查与状态同步

### 各 provider 的启动命令（含首次初始化）

每个 provider 实现 `start_command(ctx)` 返回 `StartCommand`，内含可执行文件路径、参数、环境变量、工作目录、是否需首次初始化：

```rust
pub struct StartCommand {
    pub program: String,                       // 相对 install_path
    pub args: Vec<String>,
    pub env_vars: HashMap<String, String>,
    pub working_dir: PathBuf,
    pub creation_flags: u32,                    // Windows: CREATE_NO_WINDOW = 0x08000000
    pub first_run_init: Option<FirstRunInit>,   // 首次启动前执行初始化
}

pub struct FirstRunInit {
    pub init_command: StartCommand,              // 初始化命令
    pub temp_secret_output: Option<TempSecretSpec>, // 临时密码从哪里捞
}

pub enum TempSecretSpec {
    FromStdoutRegex(String),       // 正则匹配 stdout
    FromLogFile { path: PathBuf, regex: String }, // 从日志文件捞
}
```

#### MySQL 启动

首次启动前需初始化数据目录：

```rust
// first_run_init
init_command = StartCommand {
    program: "mysql-8.4.10-winx64/bin/mysqld.exe",
    args: ["--initialize-insecure", "--basedir=.", "--datadir=./data"],
    working_dir: "install_path/mysql-8.4.10-winx64",  // ← ./data 相对此目录解析
    creation_flags: CREATE_NO_WINDOW,
    ...
}
// --initialize-insecure 生成无密码 root@localhost
// --basedir=. --datadir=./data 全部相对 working_dir，OPX 目录移动后仍生效
// 也可用 --initialize 生成随机密码，从 stderr 抓取 "A temporary password is generated for root@localhost: xxx"
```

正常启动：
```rust
start_command = StartCommand {
    program: "mysql-8.4.10-winx64/bin/mysqld.exe",
    args: ["--defaults-file=my.ini", "--console"],
    working_dir: "install_path/mysql-8.4.10-winx64",
    creation_flags: CREATE_NO_WINDOW,  // 隐藏控制台窗口
    ...
}
```

健康检查：`Tcp { port: config.port or 3306 }`

**密码策略选择**：本设计用 `--initialize-insecure`，初始化后 root 无密码。启动后通过 `mysqladmin -u root password "新密码"` 设置密码（用户首次配置时由 UI 引导，或保留无密码供本地开发用）。`--initialize` 生成的随机临时密码从 stderr 抓取后存入 `installed.json.config.temp_root_password`，UI 首次进入时提示用户。

#### Redis 启动

```rust
start_command = StartCommand {
    program: "redis-server.exe",
    args: ["redis.conf", "--port", "6379"],
    working_dir: "install_path",
    creation_flags: CREATE_NO_WINDOW,
    ...
}
```

无首次初始化。健康检查：`Custom` 发 `redis-cli ping` 期望 `PONG`。

#### Nginx 启动

```rust
start_command = StartCommand {
    program: "nginx.exe",
    args: ["-g", "daemon off;"],   // 前台运行（便于进程管理）
    working_dir: "install_path",
    creation_flags: CREATE_NO_WINDOW,
    ...
}
```

无首次初始化。健康检查：`Http { url: "http://127.0.0.1:{port}/", expected_status: 200 }`。

`daemon off` 让 Nginx 前台运行，`Command::spawn` 拿到的 PID 即 master 进程，便于停止。

#### MinIO 启动（参数来自官方文档）

```rust
start_command = StartCommand {
    program: "minio.exe",
    args: ["server", "./data", "--address", ":9000", "--console-address", ":9001"],
    env_vars: {
        "MINIO_ROOT_USER": config.access_key or "minioadmin",
        "MINIO_ROOT_PASSWORD": config.secret_key or "minioadmin",
    },
    working_dir: "install_path",
    creation_flags: CREATE_NO_WINDOW,
    ...
}
```

参数说明（官网 `/minio/docs`）：
- 第一个位置参数 `./data` 是数据目录，**相对 `install_path`**（如 `apps/minio/RELEASE.2024-09-13/data/`），与 `Command::current_dir(install_path)` 配合实现 Portable
- `--address :9000` API 端口
- `--console-address :9001` 控制台端口
- `MINIO_ROOT_USER` / `MINIO_ROOT_PASSWORD` 根账号（环境变量，命令行无对应 flag）

健康检查：`Http { url: "http://127.0.0.1:{api_port}/minio/health/live", expected_status: 200 }`（官方健康端点）。

**数据目录路径解析**：`working_dir = install_path`（MinIO provider 不重写 working_dir），`args` 中的 `./data` 相对 working_dir 解析为 `{install_path}/data/`。OPX 目录移动后，相对路径仍生效。

**首次启动需配置项**（首次启动对话框，类似自定义软件的启动命令配置）：
- API 端口（默认 9000）
- 控制台端口（默认 9001）
- 数据目录（默认 `./data`，相对 install_path）
- Access Key（默认 `minioadmin`）
- Secret Key（默认 `minioadmin`，UI 提示"建议修改"）

#### RustFS 启动（参数来自官方文档）

```rust
start_command = StartCommand {
    program: "rustfs.exe",
    args: ["./data",
           "--address", "127.0.0.1:9000",
           "--access-key", config.access_key or "rustfsadmin",
           "--secret-key", config.secret_key or "rustfsadmin"],
    env_vars: {
        "RUSTFS_CONSOLE_ENABLE": "true",
        "RUSTFS_CONSOLE_ADDRESS": "127.0.0.1:9001",
    },
    working_dir: "install_path",
    creation_flags: CREATE_NO_WINDOW,
    ...
}
```

参数说明（官网 `/rustfs/rustfs`）：
- 第一个位置参数 `./data` 是数据目录，**相对 `install_path`**（如 `apps/rustfs/x.y.z/data/`），与 `Command::current_dir(install_path)` 配合实现 Portable（与 MinIO 一致的 S3 风格）
- `--address` API 端口（默认 `0.0.0.0:9000`，本设计改为 `127.0.0.1:9000` 仅本机访问，更安全）
- `--access-key` / `--secret-key` 根账号（命令行直接传，也可用 `RUSTFS_ACCESS_KEY` / `RUSTFS_SECRET_KEY` 环境变量）
- `RUSTFS_CONSOLE_ENABLE=true` 启用控制台
- `RUSTFS_CONSOLE_ADDRESS` 控制台端口

健康检查：`Http { url: "http://127.0.0.1:{api_port}/health", expected_status: 200 }`。

**数据目录路径解析**：与 MinIO 一致，`working_dir = install_path`，`./data` 解析为 `{install_path}/data/`。

**首次启动需配置项**（与 MinIO 一致的对话框）：
- API 端口（默认 9000）
- 控制台端口（默认 9001）
- 数据目录（默认 `./data`）
- Access Key（默认 `rustfsadmin`）
- Secret Key（默认 `rustfsadmin`）

#### JRE

不参与启停（作为依赖项被 Spring Boot 应用拉起），无 start_command。

#### Custom

由 `custom_start_command` 决定。

### Windows 隐藏控制台窗口

所有 `Command::spawn` 调用必须设置 `creation_flags = CREATE_NO_WINDOW`（0x08000000），避免弹出黑色 cmd 窗口：

```rust
#[cfg(windows)]
use std::os::windows::process::CommandExt;

let mut cmd = std::process::Command::new(&program);
cmd.args(&args)
   .current_dir(&working_dir)
   .env_clear();

for (k, v) in &env_vars {
    cmd.env(k, v);
}

#[cfg(windows)]
cmd.creation_flags(0x08000000);  // CREATE_NO_WINDOW

let child = cmd.spawn()?;
```

`CREATE_NO_WINDOW` 标志让进程无窗口运行（既不是控制台窗口也不是 GUI 窗口），适合服务式后台运行。Unix 平台无此参数，进程默认不依附终端。

### health_check.rs 调度器

```rust
pub async fn run_health_check(
    app: AppHandle,
    installed_id: String,
    spec: HealthCheckSpec,
    pid: u32,
) -> HealthCheckResult {
    let max_attempts = 30;
    let interval_ms = 1000;

    for attempt in 1..=max_attempts {
        if !is_process_alive(pid) {
            return HealthCheckResult::ProcessExited;
        }

        let ok = match &spec {
            HealthCheckSpec::ProcessOnly => true,
            HealthCheckSpec::Tcp { port, timeout_ms } => {
                tcp_probe("127.0.0.1", *port, Duration::from_millis(*timeout_ms)).await
            }
            HealthCheckSpec::Http { url, expected_status, timeout_ms } => {
                http_probe(url, *expected_status, Duration::from_millis(*timeout_ms)).await
            }
            HealthCheckSpec::Custom { checker } => checker(&ctx),
        };

        if ok {
            return HealthCheckResult::Healthy;
        }

        tokio::time::sleep(Duration::from_millis(interval_ms)).await;
    }

    HealthCheckResult::Timeout
}

pub enum HealthCheckResult {
    Healthy,
    Timeout,
    ProcessExited,
}
```

### 状态同步：事件协议

事件频道：`software-status-changed`

```typescript
type SoftwareStatusEvent = {
  installed_id: string
  status: 'Running' | 'Stopped' | 'Error' | 'Unknown' | 'Starting' | 'Stopping' | 'Initializing'
  pid?: number
  error?: string
  timestamp: string
}
```

前端 `SoftwareListPage.vue` 在 `onMounted` 监听此事件，更新本地状态。

### 状态轮询（兜底）

事件驱动外，前端每 30s 调用一次 `list_installed_software` 拉取最新状态。后端 `SoftwareManager::get_installed()` 从 `installed.json` 读 + 内存注册表合并 PID 实时状态：

```rust
pub fn get_installed(&self) -> Vec<InstalledSoftware> {
    let mut list = self.installed.read().unwrap().software.clone();
    let registry = self.process_registry.lock().unwrap();

    for item in list.iter_mut() {
        if let Some(reg) = registry.get(&item.id) {
            if !is_process_alive(reg.pid) {
                item.status = SoftwareStatus::Error;
                item.last_error = Some("进程意外退出".to_string());
            }
        } else if item.status == SoftwareStatus::Running {
            item.status = SoftwareStatus::Stopped;
            item.pid = None;
        }
    }
    list
}
```

每次调用都做状态修正，保证返回前端的总是最新状态。修正时同步写回 `installed.json`（防抖：1s 内多次只写一次）。

### 状态机转换合法表

| From → To | 触发条件 |
|---|---|
| Unknown → Initializing | 用户启动且需首次初始化 |
| Unknown → Starting | 用户启动且无需初始化 |
| Unknown → Stopped | 首次 get_installed 修正 |
| Stopped → Initializing | 用户启动且需首次初始化 |
| Stopped → Starting | 用户启动且无需初始化 |
| Initializing → Starting | 初始化完成 |
| Initializing → Error | 初始化失败 |
| Initializing → Stopping | 用户取消初始化 |
| Starting → Running | 健康检查通过 |
| Starting → Error | 健康检查超时 / 进程提前退出 |
| Starting → Stopping | 用户停止（启动中取消） |
| Running → Stopping | 用户停止 |
| Running → Error | 进程意外退出 |
| Error → Initializing | 用户重试且需重新初始化 |
| Error → Starting | 用户重试启动 |
| Error → Stopping | 用户停止（强杀残留进程） |
| Stopping → Stopped | 进程退出确认 |

非法转换（拒绝并返回错误）：
- Stopped → Stopping（已停止不能再停）
- Running → Starting（运行中不能再启动）
- Stopping → Starting（停止中不能取消启动）

## 配置编辑器

### 配置文件路径

每个 provider 实现 `config_file_path(ctx)`：

| Provider | 路径（相对 install_path） | 说明 |
|---|---|---|
| MySQL | `mysql-8.4.10-winx64/my.ini` | Windows 子目录 |
| Redis | `redis.conf` | install_path 根 |
| Nginx | `conf/nginx.conf` | |
| MinIO | 无配置文件 | 启动参数全部走 env_vars + args，表单 schema 即启动参数 |
| RustFS | 无配置文件 | 同 MinIO |
| JRE | 无配置文件 | 不参与配置编辑 |
| Custom | 用户填的 `config_file_relative` | 可选 |

`config_file_path()` 返回 `Option<PathBuf>`，`None` 时 UI 不显示"配置"按钮。

### 各 provider 的配置 schema（表单字段）

#### MySQL
- `port` (Port, [mysqld] section, default 3306)
- `bind-address` (Text, [mysqld], "0.0.0.0")
- `max_connections` (Number, [mysqld], 151)
- `character-set-server` (Select: utf8mb4/utf8/latin1, [mysqld], "utf8mb4")
- `innodb_buffer_pool_size` (Text, [mysqld], "128M")

#### Redis
- `port` (Port, 顶层, 6379)
- `bind` (Text, 顶层, "127.0.0.1")
- `maxmemory` (Text, 顶层, "256mb")
- `maxmemory-policy` (Select: allkeys-lru/volatile-lru/noeviction, 顶层, "noeviction")
- `requirepass` (Password, 顶层, "")

#### Nginx
- `listen` (Port, events 块外, 80)
- `worker_processes` (Number, 顶层, 4)
- `root` (Text, http.server 块, "html")

#### MinIO（无配置文件，表单字段 = 启动参数）
- `api_port` (Port, 默认 9000) — `--address :{api_port}`
- `console_port` (Port, 默认 9001) — `--console-address :{console_port}`
- `data_dir` (Text, 默认 "./data") — 位置参数，**相对 install_path**（如 `apps/minio/.../data/`）；不接受绝对路径与 `..`
- `access_key` (Text, 默认 "minioadmin") — `MINIO_ROOT_USER` env
- `secret_key` (Password, 默认 "minioadmin") — `MINIO_ROOT_PASSWORD` env

MinIO 表单提交时，字段写入 `InstalledSoftware.config`（不走配置文件），启动时 provider.start_command 读 config 构造 env_vars + args。`data_dir` 校验：必须匹配 `^[a-zA-Z0-9_./-]+$` 且不含 `..`（与自定义软件 executable 校验同一套白名单）。

#### RustFS（同 MinIO 风格）
- `api_port` (Port, 默认 9000) — `--address 127.0.0.1:{api_port}`
- `console_port` (Port, 默认 9001) — `RUSTFS_CONSOLE_ADDRESS` env
- `data_dir` (Text, 默认 "./data") — 位置参数，**相对 install_path**；同样校验白名单
- `access_key` (Text, 默认 "rustfsadmin") — `--access-key` arg
- `secret_key` (Password, 默认 "rustfsadmin") — `--secret-key` arg

#### JRE
无 config_schema（不参与管理页配置）。

#### Custom
无 config_schema（通过 `CustomStartCommandDialog` 配置启动命令）。

### 表单 schema 示例（MySQL）

```rust
fn config_schema(&self) -> Option<ConfigSchema> {
    Some(ConfigSchema {
        fields: vec![
            ConfigField {
                key: "port".to_string(),
                label_i18n: "configField.port".to_string(),
                field_type: ConfigFieldType::Port,
                default_value: json!(3306),
                section: Some("[mysqld]".to_string()),
                description_i18n: Some("configField.port.desc".to_string()),
            },
            ConfigField {
                key: "bind-address".to_string(),
                label_i18n: "configField.bindAddress".to_string(),
                field_type: ConfigFieldType::Text,
                default_value: json!("0.0.0.0"),
                section: Some("[mysqld]".to_string()),
                description_i18n: None,
            },
            ConfigField {
                key: "max_connections".to_string(),
                label_i18n: "configField.maxConnections".to_string(),
                field_type: ConfigFieldType::Number,
                default_value: json!(151),
                section: Some("[mysqld]".to_string()),
                description_i18n: None,
            },
            ConfigField {
                key: "character-set-server".to_string(),
                label_i18n: "configField.charset".to_string(),
                field_type: ConfigFieldType::Select {
                    options: vec!["utf8mb4".to_string(), "utf8".to_string(), "latin1".to_string()],
                },
                default_value: json!("utf8mb4"),
                section: Some("[mysqld]".to_string()),
                description_i18n: None,
            },
            ConfigField {
                key: "innodb_buffer_pool_size".to_string(),
                label_i18n: "configField.innodbBufferPool".to_string(),
                field_type: ConfigFieldType::Text,  // "128M" 这种带单位
                default_value: json!("128M"),
                section: Some("[mysqld]".to_string()),
                description_i18n: None,
            },
        ],
    })
}
```

MinIO / RustFS 的 schema 字段对应启动参数（`api_port` / `console_port` / `data_dir` / `access_key` / `secret_key`），表单提交后写入 `config` 而非配置文件，启动时 provider.start_command 读 config 构造 env_vars + args。

### 配置文件读写双引擎

`config_editor.rs` 内部按文件格式分派：

```rust
pub fn read_config_as_form(file_path: &Path, schema: &ConfigSchema) -> Result<FormData> {
    let content = std::fs::read_to_string(file_path)?;
    let format = detect_format(file_path);

    let mut form_data = FormData::new();
    for field in &schema.fields {
        let value = match format {
            ConfigFormat::Ini => ini_lookup(&content, field.section.as_deref(), &field.key)?,
            ConfigFormat::KeyValue => kv_lookup(&content, &field.key)?,
            ConfigFormat::NginxConf => nginx_lookup(&content, &field.key)?,
            ConfigFormat::Json => json_lookup(&content, &field.key)?,
        };
        form_data.insert(field.key.clone(), value);
    }
    Ok(form_data)
}

pub fn write_form_to_config(file_path: &Path, schema: &ConfigSchema, form_data: &FormData) -> Result<()> {
    let content = std::fs::read_to_string(file_path).unwrap_or_default();
    let format = detect_format(file_path);

    let mut new_content = content;
    for (key, value) in form_data {
        let field = schema.find_field(key)?;
        new_content = match format {
            ConfigFormat::Ini => ini_upsert(&new_content, field.section.as_deref(), &key, value)?,
            ConfigFormat::KeyValue => kv_upsert(&new_content, &key, value)?,
            ConfigFormat::NginxConf => nginx_upsert(&new_content, &key, value)?,
            ConfigFormat::Json => json_upsert(&new_content, &key, value)?,
        };
    }

    backup_config(file_path)?;
    std::fs::write(file_path, new_content)?;
    Ok(())
}

pub fn write_source_to_config(file_path: &Path, source: &str) -> Result<()> {
    backup_config(file_path)?;
    std::fs::write(file_path, source)?;
    Ok(())
}
```

### 配置格式支持矩阵

| 格式 | 软件 | 解析方式 |
|---|---|---|
| INI（带 `[section]`） | MySQL（my.ini） | 简单状态机：识别 `[section]` 行、`key=value` 行、注释行 |
| KeyValue（无 section） | Redis（redis.conf） | 按行匹配 `key value`（空格分隔） |
| NginxConf（嵌套块） | Nginx | 用 `ngyn`/`oxi-ngyn` crate 或手写简单解析器 |
| 无配置文件 | MinIO/RustFS | 仅表单（启动参数），不显示源码 tab |

**YAGNI 边界**：本次配置解析只支持"键值替换"，不解析嵌套结构（如 nginx 的 `http { server {...} }`）。Nginx 的 `listen`、`worker_processes`、`root` 这几个目标字段位于嵌套块内，用行级正则匹配 + 行号定位的方式 upsert，不做完整 AST。

### Monaco 编辑器集成

前端 `ConfigSourceTab.vue`：

```vue
<template>
  <div class="config-source-tab">
    <div ref="editorContainer" class="editor-container" />
    <div class="tab-footer">
      <span class="hint">{{ $t('configEdit.sourceHint') }}</span>
      <button class="btn primary" @click="saveSource" :disabled="!dirty">{{ $t('save') }}</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import * as monaco from 'monaco-editor'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ installedId: string; filePath: string }>()
const editorContainer = ref<HTMLElement>()
let editor: monaco.editor.IStandaloneCodeEditor | null = null
let originalContent = ''

onMounted(async () => {
  const content = await invoke<string>('read_config_source', { installedId: props.installedId })
  originalContent = content
  editor = monaco.editor.create(editorContainer.value!, {
    value: content,
    language: detectLanguage(props.filePath),
    theme: 'vs-dark',
    automaticLayout: true,
    minimap: { enabled: false },
  })
})

async function saveSource() {
  const content = editor!.getValue()
  await invoke('write_config_source', { installedId: props.installedId, content })
  originalContent = content
}
</script>
```

`detectLanguage`：`.ini` → `ini`；`redis.conf` / `*.conf` → `ini`；`nginx.conf` → `plaintext`。

### 配置编辑对话框布局

```
┌─ ConfigEditDialog ────────────────────────────────────────┐
│  MySQL 8.4.10 配置                                  [X]    │
├───────────────────────────────────────────────────────────┤
│  [表单] [源码]                                              │
│                                                            │
│  ┌─ 表单视图 ────────────────────────────────────────┐    │
│  │  端口         [3306            ]                    │    │
│  │  绑定地址     [0.0.0.0         ]                    │    │
│  │  最大连接数   [151             ]                    │    │
│  │  字符集       [utf8mb4       ▼]                      │    │
│  │  InnoDB 缓冲  [128M            ]                    │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                            │
│  ⓘ 修改后需重启软件生效                                     │
├───────────────────────────────────────────────────────────┤
│                              [取消]  [保存并重启]            │
└────────────────────────────────────────────────────────────┘
```

切换 tab 时强制重读盘（避免脏状态）：

```typescript
async function switchTab(tab: 'form' | 'source') {
  if (currentTab.value === tab) return
  if (dirty.value) {
    const confirmed = await confirmDialog(t('configEdit.dirtyConfirm'))
    if (!confirmed) return
  }
  currentTab.value = tab
  reloadKey.value++
}
```

### 保存后是否自动重启

- 表单 tab 保存：仅写文件，提示"配置已保存，是否立即重启 X 生效？"
- 源码 tab 保存：同上
- 不强制重启——用户可能想连续改多处再统一重启

### 备份策略

`backup_config(file_path)` 在写入前复制到 `config/backups/{installed_id}/{timestamp}_{filename}`，保留最近 5 份，超出按时间清理。

### Portable 路径策略

post_install 阶段生成配置文件时全部使用相对路径：

- MySQL `my.ini`：`basedir=.` `datadir=./data`（去掉绝对路径）
- Redis `redis.conf`：`dir ./`
- Nginx：默认 `root html` 相对 prefix

启动子进程时 `Command::current_dir(working_dir)`，其中 `working_dir` 由 provider 声明（MySQL 重写为 `install_path/mysql-{ver}-winx64/`，其他默认 `install_path`）。

OPX 目录移动后，相对路径仍生效，无需修改配置文件。

## 卸载校验与自定义软件

### 卸载前置校验流程

```
前端：点"卸载"按钮
  │
  ▼ invoke check_uninstall_safety(installed_id)
后端 check_uninstall_safety 命令：
  1. 加载 InstalledSoftware
  2. 运行中校验（A）：
     - 查内存注册表是否有此 installed_id
     - 或 status in [Running, Starting, Stopping]
     → in_use = true, reason = "running"
  3. 依赖校验（C，仅 key=="jre"）：
     - 调 check_jre_in_use(installed_id) → JreUsageReport
     - 若 in_use=true → 合并到返回值
  4. 自定义软件（is_custom=true）：跳过 C，仅 A
  5. 返回 UninstallSafetyReport
```

### UninstallConfirmDialog 改造

- 若 `safe=true`：显示"卸载"按钮，点击 → `uninstall_software(installed_id)`
- 若 `safe=false`：禁用"卸载"按钮，仅显示"取消"，列出阻止原因
- JRE 依赖场景显示依赖列表（默认 JRE 标记 + Spring Boot 应用列表）

### JRE 依赖检查实现

```rust
pub fn check_jre_in_use(jre_installed_id: &str) -> JreUsageReport {
    let mut report = JreUsageReport {
        in_use: false,
        is_default: false,
        dependents: vec![],
    };

    if let Some(settings) = load_settings() {
        if settings.jre_default_id.as_deref() == Some(jre_installed_id) {
            report.is_default = true;
            report.in_use = true;
        }
    }

    // springboot-manager 未实现时返回 None，不阻塞卸载
    if let Some(springboot_apps) = try_load_springboot_apps() {
        for app in springboot_apps {
            if app.jre_id == jre_installed_id && app.status == "running" {
                report.dependents.push(JreDependent {
                    kind: "springboot-app".to_string(),
                    id: app.id,
                    name: app.name,
                    status: "running".to_string(),
                });
                report.in_use = true;
            }
        }
    }

    report
}
```

`try_load_springboot_apps()` 当前返回 `None`（springboot-manager 是空文件），规格里标注"待 springboot-manager 实现后填充"。

### 卸载执行流程

```
invoke uninstall_software(installed_id)
  │
  ▼ 后端：
  1. 复查 check_uninstall_safety（防止前端绕过）
     → 若 unsafe，返回 Err
  2. 加载 InstalledSoftware
  3. 若 pid 存在 → 强杀兜底（防意外）
  4. 删除 install_path 目录
     - 失败（文件被占用）→ 返回 Err("文件被占用，无法删除：{path}")
       Windows 上 mysqld 残留文件锁的常见场景
  5. 从 installed.json 移除记录
  6. 从内存注册表移除
  7. 写审计日志"uninstall {key}/{version}"
  8. emit "software-uninstalled" { installed_id }
  9. 返回 Ok(true)
```

### 自定义软件启动命令配置

`CustomStartCommandDialog.vue` 字段：

```
┌─ 自定义软件启动配置 ──────────────────────────────┐
│                                                  │
│  预设模板：[通用可执行文件 ▼]                       │
│            (Redis / Nginx / 通用可执行文件)         │
│                                                  │
│  可执行文件 *  [bin/app.exe           ] [浏览]    │
│  启动参数      [--port=8080           ]            │
│  工作目录      [(默认 install_path)    ] [浏览]    │
│  端口          [8080                   ]          │
│                                                  │
│  环境变量：                                       │
│  ┌─────────────────┬─────────────────┬───┐       │
│  │ KEY             │ VALUE           │ × │       │
│  └─────────────────┴─────────────────┴───┘       │
│  [+ 添加]                                        │
│                                                  │
│  健康检查：                                       │
│  ○ 不检查（仅进程存活）                           │
│  ○ TCP 端口   [8080]                              │
│  ○ HTTP URL   [http://127.0.0.1:8080/health]      │
│    期望状态码 [200]                               │
│                                                  │
│  配置文件路径（可选，填写后启用源码编辑）：         │
│  [conf/app.conf          ] [浏览]                 │
│                                                  │
│                          [取消]  [保存]            │
└──────────────────────────────────────────────────┘
```

### 预设模板

```rust
pub struct CustomTemplate {
    pub id: &'static str,
    pub name_i18n: &'static str,
    pub executable: &'static str,
    pub args: &'static [&'static str],
    pub health_spec: CustomHealthSpec,
    pub config_file_relative: Option<&'static str>,
}

pub fn builtin_templates() -> &'static [CustomTemplate] {
    &[
        CustomTemplate {
            id: "redis-server",
            name_i18n: "template.redisServer",
            executable: "redis-server.exe",
            args: &["{config_file}"],
            health_spec: CustomHealthSpec::Tcp { port: 6379 },
            config_file_relative: Some("redis.conf"),
        },
        CustomTemplate {
            id: "nginx",
            name_i18n: "template.nginx",
            executable: "nginx.exe",
            args: &["-g", "daemon off;"],
            health_spec: CustomHealthSpec::Http {
                url: "http://127.0.0.1/".to_string(),
                expected_status: 200,
            },
            config_file_relative: Some("conf/nginx.conf"),
        },
        CustomTemplate {
            id: "generic",
            name_i18n: "template.generic",
            executable: "",
            args: &[],
            health_spec: CustomHealthSpec::None,
            config_file_relative: None,
        },
    ]
}
```

### 自定义软件的"配置"对话框

`ConfigEditDialog` 对 `is_custom=true` 的特殊处理：

- **启动命令 tab**：复用 `CustomStartCommandDialog` 的字段，编辑模式（填充已保存的 `custom_start_command`）
- **源码 tab**：仅当 `custom_start_command.config_file_relative` 不为空时显示
- **表单 tab**：不显示（自定义软件无 `config_schema`）

### 自定义软件首次启动

若用户从未配置 `custom_start_command` 直接点"启动"：

- 不直接报错，而是自动打开 `CustomStartCommandDialog`
- 提示"请先配置启动命令"
- 配置完成保存后，自动触发启动

### 安全约束

- `executable` 必须是相对路径（禁止绝对路径与 `..`），白名单字符 `^[a-zA-Z0-9_./-]+$`
- args 不走 shell（`Command::arg` 直接传参）避免命令注入
- `working_dir` 同样限制为相对路径

## 审计日志

### 初始化

在 `lib.rs` 的 `setup` hook 中初始化 `tracing` + `tracing-appender`：

```rust
use tracing_appender::rolling;
use tracing_subscriber::{fmt, EnvFilter};

let log_dir = paths::log_dir();
let file_appender = rolling::daily(&log_dir, "software-manager.log");
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

tracing_subscriber::registry()
    .with(EnvFilter::new("info"))
    .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
    .with(fmt::layer().with_writer(std::io::stderr()))
    .init();
```

`_guard` 保存在 `AppState` 中防止 flush 丢失。

### 日志格式

```
2026-07-02T14:23:01.123Z INFO start_software installed_id=abc-123 key=mysql version=8.4.10 pid=12345
2026-07-02T14:23:32.456Z INFO software_healthy installed_id=abc-123 key=mysql duration_ms=31233
2026-07-02T14:25:00.789Z INFO stop_software installed_id=abc-123 key=mysql pid=12345 force=false
2026-07-02T14:25:05.012Z INFO software_stopped installed_id=abc-123 key=mysql graceful=true
2026-07-02T14:25:30.000Z INFO uninstall_software installed_id=abc-123 key=mysql version=8.4.10 path=apps/mysql/8.4.10
2026-07-02T14:25:30.500Z ERROR uninstall_failed installed_id=abc-123 error="文件被占用"
```

### 事件类型与字段

| 事件 | level | 字段 |
|---|---|---|
| `start_software` | INFO | installed_id, key, version, pid |
| `software_healthy` | INFO | installed_id, key, duration_ms |
| `start_failed` | ERROR | installed_id, key, error |
| `health_check_timeout` | WARN | installed_id, key, pid |
| `software_exited` | WARN | installed_id, key, pid, code |
| `stop_software` | INFO | installed_id, key, pid, force |
| `software_stopped` | INFO | installed_id, key, graceful |
| `restart_software` | INFO | installed_id, key |
| `uninstall_software` | INFO | installed_id, key, version, path |
| `uninstall_failed` | ERROR | installed_id, error |
| `config_edit` | INFO | installed_id, key, tab, fields_changed |
| `auto_start_on_boot` | INFO | count, ids |
| `stop_all_on_exit` | INFO | count, ids |

### 轮转与保留

- `tracing-appender::rolling::daily` 按天滚动，文件名 `software-manager.log.2026-07-02`
- 默认保留 7 天，`audit_log::cleanup_old_logs()` 在应用启动时清理

## Tauri 命令 API

```rust
// 启停
start_software(installed_id: String) -> Result<()>
stop_software(installed_id: String) -> Result<bool>  // true=优雅, false=强杀
restart_software(installed_id: String) -> Result<()>

// 状态
list_installed_software() -> Result<Vec<InstalledSoftware>>  // 已有，扩展返回值字段
get_software_status(installed_id: String) -> Result<SoftwareStatus>
check_jre_in_use(jre_installed_id: String) -> Result<JreUsageReport>

// 卸载
check_uninstall_safety(installed_id: String) -> Result<UninstallSafetyReport>
uninstall_software(installed_id: String) -> Result<bool>  // 已有，加入前置校验

// 配置编辑
get_config_schema(installed_id: String) -> Result<Option<ConfigSchema>>
read_config_form(installed_id: String) -> Result<FormData>
write_config_form(installed_id: String, data: FormData) -> Result<()>
read_config_source(installed_id: String) -> Result<String>
write_config_source(installed_id: String, content: String) -> Result<()>

// 自定义软件启动命令
get_custom_start_command(installed_id: String) -> Result<Option<CustomStartCommand>>
save_custom_start_command(installed_id: String, cmd: CustomStartCommand) -> Result<()>
list_custom_templates() -> Result<Vec<CustomTemplate>>
browse_executable() -> Result<String>
browse_config_file() -> Result<String>

// 启动设置
save_startup_settings(installed_id: String, auto_start: bool, order: u32) -> Result<()>

// 审计日志（本次不接 UI，预留）
get_audit_log_lines(date: Option<String>, limit: u32) -> Result<Vec<String>>
```

所有命令在 `lib.rs` 的 `invoke_handler!` 注册。

## 前端 UI 设计

### 总体布局

```
┌─ SoftwareListPage ──────────────────────────────────────────────────────┐
│  📦 软件管理                                          [↻ 刷新]            │
│  管理已安装的软件实例                                                     │
├────────────────────────────────────────────────────────────────────────┤
│  ┌── 数据库 ────────────────────────────────────────────────────────┐  │
│  │ ┌─ SoftwareInstanceRow ──────────────────────────────────────┐ │  │
│  │ │ 🗄  MySQL 8.4.10          ● 运行中  PID 12345  端口 3306   │ │  │
│  │ │    apps/mysql/8.4.10                                       │ │  │
│  │ │    [启动] [配置] [⚙] [卸载]                                  │ │  │
│  │ └────────────────────────────────────────────────────────────┘ │  │
│  │ ... MySQL 8.0.36 ...                                         │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│  ┌── 缓存 ─────────────────────────────────────────────────────────┐  │
│  │ ... Redis ...                                                │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│  ┌── Web 服务器 ───────────────────────────────────────────────────┐  │
│  │ ... Nginx ...                                                 │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│  ┌── 对象存储 ─────────────────────────────────────────────────────┐  │
│  │ ... MinIO / RustFS ...                                        │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│  ┌── 自定义 ───────────────────────────────────────────────────────┐  │
│  │ ... 用户上传的软件 ...                                         │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

分组按 `SoftwareCategory` enum，自定义软件独立分组。JRE 不在此页（仓库页可卸载，管理页不展示）。

### SoftwareInstanceRow 按钮逻辑

| 状态 | 启动按钮 | 停止按钮 | 配置 | 启动设置 | 卸载 |
|---|---|---|---|---|---|
| Running | 禁用 | 启用 | 启用 | 启用 | 禁用（带 tooltip"请先停止"） |
| Stopped | 启用 | 禁用 | 启用 | 启用 | 启用 |
| Starting | 禁用+加载 | 启用（取消启动） | 禁用 | 禁用 | 禁用 |
| Stopping | 禁用 | 禁用+加载 | 禁用 | 禁用 | 禁用 |
| Error | 启用（重试） | 启用（清理） | 启用 | 启用 | 启用 |
| Unknown | 启用 | 禁用 | 禁用 | 启用 | 启用 |

按钮统一 `btn` 样式，主按钮（启动）用 `bg-primary`，危险按钮（卸载）用 `bg-destructive`。

### StatusBadge 状态显示

| 状态 | 颜色 | 图标 | 文本 |
|---|---|---|---|
| Running | 绿色（success） | `mdi:circle-medium` | "运行中" |
| Stopped | 灰色（muted） | `mdi:circle-outline` | "已停止" |
| Starting | 黄色（warning）+ 脉冲动画 | `mdi:loading` | "启动中" |
| Stopping | 黄色（warning）+ 脉冲动画 | `mdi:loading` | "停止中" |
| Error | 红色（destructive） | `mdi:alert-circle` | "错误" + tooltip 显示 last_error |
| Unknown | 灰色（muted） | `mdi:help-circle` | "未知" |

### 启动设置对话框

```
┌─ 启动设置 ────────────────────────────────┐
│                                          │
│  MySQL 8.4.10                             │
│                                          │
│  ☑ 随 OPX 启动时自动拉起                   │
│                                          │
│  启动顺序：[ 10 ]                          │
│  （数字越小越早启动，相同数字并发启动）       │
│                                          │
│                          [取消]  [保存]    │
└────────────────────────────────────────────┘
```

### Pinia store（lifecycle.ts）

```typescript
export const useLifecycleStore = defineStore('lifecycle', () => {
  const statuses = ref<Record<string, SoftwareStatus>>({})

  function setStatus(id: string, status: SoftwareStatus) {
    statuses.value[id] = status
  }

  function getStatus(id: string): SoftwareStatus {
    return statuses.value[id] ?? 'Unknown'
  }

  async function initListener() {
    const { listen } = await import('@tauri-apps/api/event')
    await listen<SoftwareStatusEvent>('software-status-changed', (e) => {
      setStatus(e.payload.installed_id, e.payload.status)
    })
  }

  return { statuses, setStatus, getStatus, initListener }
})
```

`SoftwareListPage` 在 `onMounted` 调 `initListener()`，并在 `onBeforeUnmount` 取消监听。

## 错误处理

后端统一用 `anyhow::Result`，错误以中文消息返回前端。前端按状态/操作类型区分展示。

| 错误场景 | 处理方式 | 用户可见消息 |
|---|---|---|
| 启动时状态非法 | 校验后立即返回 Err | "当前状态为 {status}，无法启动" |
| 启动时 PID 残留 | 校验后立即返回 Err | "进程 {pid} 仍在运行，请先停止" |
| Command::spawn 失败 | 立即返回 Err | "启动失败：{原因}" |
| 健康检查超时 | 标记 Error，保留 PID | "健康检查超时（30s 内未响应）" |
| 进程意外退出 | 标记 Error + 事件推送 | "进程意外退出，code={x}" |
| 停止时状态非法 | 校验后返回 Err | "当前状态为 {status}，无法停止" |
| 优雅停止超时 | 强杀 + 返回 false | toast"已强制停止" |
| 卸载时运行中 | check_uninstall_safety 阻止 | "软件正在运行，请先停止" |
| 卸载时 JRE 被依赖 | check_uninstall_safety 阻止 | "X 个应用依赖此 JRE" |
| 卸载时文件被占用 | 重试 3 次后返回 Err | "文件被占用，无法删除：{path}" |
| 配置文件读失败 | 返回 Err | "读取配置文件失败：{原因}" |
| 配置文件写失败 | 返回 Err + 不更新内存 | "写入配置文件失败：{原因}" |
| 自定义软件 executable 含非法字符 | 校验 `^[a-zA-Z0-9_./-]+$` | "可执行文件路径仅允许字母、数字、下划线、连字符、点、斜杠" |
| 自定义软件 executable 含 `..` | 校验拒绝 | "可执行文件路径不允许 `..`" |
| 自定义软件未配置启动命令启动 | 自动打开配置对话框 | "请先配置启动命令" |

### 并发安全

- `installed.json` 读写用 `RwLock<InstalledSoftwareList>`，写时持锁
- 进程注册表用 `Mutex<HashMap<String, RegisteredProcess>>`
- 健康检查任务通过 `tokio::spawn` 独立运行，与状态查询通过 `Arc<SoftwareManager>` 共享
- 所有锁通过 `tauri::State` 注入

## 测试策略

### 后端单元测试

| 模块 | 测试内容 |
|---|---|
| `lifecycle.rs` | start_command 派生（provider 各一个）；PID 注册/注销；状态机转换合法性（含 Initializing 中间态）；stop_one 优雅→强杀流程；restart 调用 stop+start；CREATE_NO_WINDOW flag 正确设置 |
| `health_check.rs` | TCP 探测成功/失败；HTTP 状态码匹配/不匹配；ProcessOnly 退化；超时 30 次后返回 Timeout；进程提前退出返回 ProcessExited |
| `config_editor.rs` | INI 读 port=3306；INI upsert 不破坏其他字段；Redis kv 读 maxmemory；Nginx listen 字段 upsert；备份文件生成；自定义软件无 config_file_path 时跳过；MinIO/RustFS 表单字段写入 config（无文件 IO） |
| `uninstall_guard.rs` | 运行中拒绝卸载；JRE 默认时拒绝；JRE 被 Spring Boot 应用依赖时拒绝；springboot-manager 未实现时不阻塞 |
| `audit_log.rs` | 日志按事件类型记录；cleanup_old_logs 清理 7 天前文件；日志文件按日滚动命名 |
| `providers/mysql.rs` | `start_command` 含 `--defaults-file=my.ini --console`；`first_run_init` 含 `--initialize-insecure`；`config_schema` 字段完整；`working_dir` 为 `install_path/mysql-{ver}-winx64/` |
| `providers/minio.rs` | `start_command` 含 `server ./data --address :9000 --console-address :9001`；env_vars 含 `MINIO_ROOT_USER`/`MINIO_ROOT_PASSWORD`；`config_schema` 含 api_port/console_port/data_dir/access_key/secret_key；无 `config_file_path` |
| `providers/rustfs.rs` | `start_command` 含 `./data --address 127.0.0.1:9000 --access-key ... --secret-key ...`；env_vars 含 `RUSTFS_CONSOLE_ENABLE`/`RUSTFS_CONSOLE_ADDRESS`；`config_schema` 字段完整；无 `config_file_path` |
| `providers/redis.rs` / `nginx.rs` | `start_command` 含 daemon off / redis.conf；`config_schema` 字段完整 |
| `providers/custom_templates.rs` | 三种模板（redis-server / nginx / generic）字段完整 |

集成测试：

```rust
// tests/software_lifecycle.rs
#[test]
fn start_software_writes_pid_to_installed_json() { ... }
#[test]
fn start_then_stop_clears_pid() { ... }
#[test]
fn mysql_first_run_initializes_data_dir() { ... }     // 新增
#[test]
fn mysql_second_start_skips_initialization() { ... }  // 新增
#[test]
fn minio_start_command_has_correct_args() { ... }     // 新增
#[test]
fn rustfs_start_command_has_correct_args() { ... }     // 新增
#[test]
fn minio_form_save_updates_config_not_file() { ... }  // 新增
#[test]
fn uninstall_running_software_blocked() { ... }
#[test]
fn auto_start_on_boot_pulls_by_startup_order() { ... }
#[test]
fn config_edit_form_tab_writes_to_my_ini() { ... }
#[test]
fn config_edit_source_tab_preserves_unrelated_lines() { ... }
#[test]
fn windows_no_console_window_flag_set() { ... }      // 新增：CREATE_NO_WINDOW
```

用 `tempfile` 隔离 apps 目录，mock HTTP server（`mockito`）做健康检查测试。

### 前端测试

不引入测试框架（与现有项目一致），靠手动验证。

## 验证清单

实现完成后手动跑：

1. `cd src-tauri && cargo test` — 后端单元 + 集成测试全绿
2. `npm run build` — vue-tsc + vite 构建无错
3. `npm run tauri:dev` 启动后：
   - 已安装的 MySQL/Redis/Nginx 在管理页按分组渲染
   - 点 MySQL "启动" → 状态从 已停止 → 初始化中（黄色脉冲，"初始化数据目录…"）→ 启动中 → 运行中，PID 显示在行上
   - MySQL data 目录创建后，再次启动跳过初始化（状态直接 已停止 → 启动中 → 运行中）
   - 启动中关闭对话框 → 状态仍实时更新（事件驱动）
   - 点"停止" → 运行中 → 停止中 → 已停止
   - 启动 MySQL 8.0.36 和 8.4.10 两个实例 → 端口冲突时第二个标记 Error + 错误提示
   - 点 MySQL "配置" → 表单 tab 改 port=3307 → 保存 → 提示重启 → 重启后端口生效
   - 切到源码 tab → 看到磁盘最新内容 → 改一行 → 保存 → 重启验证
   - MinIO 首次点"启动" → 自动弹出首次配置对话框（API 端口 9000 / 控制台端口 9001 / 数据目录 ./data / access key / secret key）→ 保存 → 启动 → 健康检查通过 → 运行中
   - MinIO 启动后访问 `http://127.0.0.1:9001` 控制台，用配置的 access/secret key 登录验证
   - RustFS 首次点"启动" → 同 MinIO 流程，验证 `http://127.0.0.1:9001` 控制台
   - 启动 MinIO 与 RustFS 同时运行 → 端口冲突时第二个标记 Error
   - 启动设置勾选"随 OPX 启动" → 关闭 OPX → 重新打开 → MySQL 自动拉起
   - 启动一个软件后直接关 OPX → 退应用前看到日志"stop_all_on_exit count=1"
   - **无 cmd/控制台窗口弹出**（Windows 任务栏无黑色 cmd 图标）
   - 运行中点卸载 → 对话框显示阻止原因"软件正在运行"，卸载按钮禁用
   - 先停止再卸载 → 正常卸载，目录被删除
   - JRE 设置为默认后卸载 → 阻止原因"默认 JRE"
   - 自定义软件首次启动 → 自动打开启动命令配置对话框 → 选"通用可执行文件"模板 → 填路径 → 保存 → 自动启动
   - `logs/software-manager.log.2026-07-02` 文件存在，含完整操作记录
   - 移动 OPX 目录到其他位置 → 启动 MySQL → 仍正常工作（portable 验证，basedir/datadir 相对路径生效）

## 国际化 keys

```
// 启停
start / stop / restart / starting / stopping / running / stopped / error / unknown / initializing /
processExited / healthCheckTimeout / startFailed / stopFailed / restartFailed /
initializingDataDir / initializationFailed

// 配置编辑
config / configEdit / formView / sourceView / configDirtyConfirm / saveAndRestart /
saveWithoutRestart / configSaved / configSaveFailed / configField.{port,bindAddress,...} /
configField.{port,bindAddress,...}.desc /
configField.apiPort / configField.consolePort / configField.dataDir /
configField.accessKey / configField.secretKey /
template.redisServer / template.nginx / template.generic /
executable / startArgs / workingDir / envVars / healthCheck /
noHealthCheck / tcpPort / httpUrl / expectedStatus / customConfigFile

// 卸载
uninstall / uninstalling / uninstallBlocked / uninstallBlockedRunning /
uninstallBlockedJreDefault / uninstallBlockedJreDependents / forceUninstall

// 启动设置
startupSettings / autoStartOnAppStart / startupOrder / startupOrderDesc

// 首次启动配置（MinIO/RustFS）
firstRunConfig / firstRunConfigDesc / firstRunConfigMinio / firstRunConfigRustfs

// 审计
auditLog / operationHistory
```

## 依赖新增

### Cargo.toml（src-tauri）

```toml
[dependencies]
# 新增
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

[dev-dependencies]
# 已有 mockito、tempfile
```

### package.json

```json
{
  "dependencies": {
    "monaco-editor": "^0.50.0"
  }
}
```

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| Monaco 体积大，拖慢首屏 | 异步加载 `ConfigEditDialog` 组件（`defineAsyncComponent`），仅在打开配置时拉取 |
| Windows 文件锁（mysqld 残留）阻止卸载 | 卸载前强杀进程 + 等 500ms + 重试删除 3 次 |
| 健康检查误判（端口探测时防火墙拦截） | 30 次重试 + Provider 可声明 `Custom` 钩子用 `redis-cli ping` 等更准的检查 |
| PID 复用（OPX 长期运行后 PID 被回收） | 每次状态查询实时 `is_process_alive` + 进程名匹配（不仅看 PID） |
| 配置文件格式各异，解析器维护成本 | 仅支持 INI / KeyValue / NginxConf 三种，自定义软件走"无 schema"路径；YAGNI |
| 自定义软件启动命令注入风险 | `executable` 必须是相对路径，白名单字符校验；args 不走 shell |

## 设计决策总结

| 决策点 | 选择 | 理由 |
|---|---|---|
| 配置编辑形态 | C：表单 + Monaco 双 tab | 用户明确要求 |
| 进程持久化策略 | D：默认退出即停 + auto_start 标记 | 复用现有字段，向后兼容 |
| 健康检查 | C：Provider 自定义钩子 | 软件差异大，一刀切不准 |
| 卸载校验范围 | D：运行中 + JRE 依赖 | 不做端口兜底，YAGNI |
| 配置双视图 | C：双 tab + 切换重读盘 | 避免脏状态，实现成本可控 |
| Portable 路径 | A：相对路径 + current_dir | 已有 portable 诉求 |
| 多实例启停 | A + C 简化版 | 每实例独立，UI 按 key 排序 |
| 自定义软件 | C + C2 | 启动命令模板 + 可选配置文件 |
| 操作日志 | C：全量审计 | 排查 + 可追溯 |
| Spring Boot 整合 | D：本次不含但预留接口 | 边界清晰 |
| 模块拆分 | B：按职责分子模块 | 与现有风格一致 |
| **MySQL 初始化** | `--initialize-insecure` 无密码 | 简单，UI 引导后续设密码；备选 `--initialize` 抓临时密码 |
| **MinIO 启动参数** | `server ./data --address :9000 --console-address :9001` + env `MINIO_ROOT_USER/PASSWORD` | 官方文档推荐 |
| **RustFS 启动参数** | `./data --address 127.0.0.1:9000 --access-key --secret-key` + env `RUSTFS_CONSOLE_*` | 官方文档推荐 |
| **进程窗口模式** | Windows CREATE_NO_WINDOW (0x08000000) | 服务式运行，无黑色 cmd 窗口 |
| **MinIO/RustFS 首次配置** | 弹出对话框填端口/目录/密钥 | 与自定义软件启动命令配置一致的交互 |
| **状态机扩展** | 新增 Initializing 中间态 | MySQL 等需首次初始化的软件有明确阶段 |

## 规格自检

| 检查项 | 状态 | 说明 |
|---|---|---|
| 占位符扫描 | ✅ 无 | 没有 TODO 或未完成章节；`try_load_springboot_apps` 返回 None 是明确预期行为，非占位 |
| 内部一致性 | ✅ 通过 | 架构、数据模型、API、UI、测试各节一致；Initializing 状态在状态机/事件/UI/测试均覆盖 |
| 范围检查 | ✅ 合适 | 聚焦启停/配置/卸载/首次初始化，一个实现计划可覆盖 |
| 模糊性检查 | ✅ 通过 | 所有需求明确定义，边界清晰 |
| 官方参数验证 | ✅ 通过 | MinIO/RustFS 启动参数来源于 Context7 查询的官方文档 |
| 初始化策略 | ✅ 通过 | MySQL `--initialize-insecure` 明确，备选 `--initialize` 抓临时密码 |
| 窗口隐藏 | ✅ 通过 | Windows CREATE_NO_WINDOW flag 在启动流程与测试清单均覆盖 |
