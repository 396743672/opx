# OPX - 跨平台运维管理工具 设计文档

**日期**: 2026-06-25
**项目位置**: `D:\object\opx`
**技术栈**: Rust + Tauri 2 + Vue 3 + TypeScript + Tailwind CSS

## 概述

OPX 是一个轻量级跨平台运维管理桌面应用，参考 DBX 数据库客户端的技术架构和设计风格，提供系统监控、软件管理和 SpringBoot 应用管理一体化运维体验。

### 项目目标

- 提供开箱即用的常用运维软件开发环境部署
- 支持 SpringBoot 应用全生命周期管理
- 全平台一致体验（Windows/macOS/Linux）
- 绿色便携，软件安装相对路径，支持整体迁移
- 模块化设计，预留扩展接口

## 需求清单

### 核心功能

| 模块 | 功能 | 状态 |
|------|------|------|
| **系统监控** | CPU/内存/磁盘/网络实时监控 | ✅ |
| **系统监控** | 进程列表查看和管理 | ✅ |
| **系统监控** | 系统信息展示 | ✅ |
| **系统监控** | 历史趋势图表 | ✅ |
| **软件管理** | MySQL 在线/本地上传安装管理 | ✅ |
| **软件管理** | JDK 在线/本地上传安装管理（多版本切换） | ✅ |
| **软件管理** | Redis 在线/本地上传安装管理 | ✅ |
| **软件管理** | Nginx 在线/本地上传安装管理 | ✅ |
| **软件管理** | 用户自定义压缩包上传安装 | ✅ |
| **软件管理** | 软件启动/停止/重启/配置编辑/卸载 | ✅ |
| **SpringBoot 管理** | 应用添加/编辑/删除 | ✅ |
| **SpringBoot 管理** | 启动/停止/重启单个应用 | ✅ |
| **SpringBoot 管理** | 按分组依赖顺序批量启动 | ✅ |
| **SpringBoot 管理** | JVM 运行监控 | ✅ |
| **SpringBoot 管理** | 实时日志查看搜索 | ✅ |
| **SpringBoot 管理** | 端口占用检测 | ✅ |
| **SpringBoot 管理** | 自动备份和一键回滚 | ✅ |
| **SpringBoot 管理** | 崩溃自动重启 | ✅ |
| **SpringBoot 管理** | 多环境配置切换 | ✅ |
| **SpringBoot 管理** | 一键更新 jar 包 | ✅ |
| **系统集成** | OPX 自身注册为系统服务，随系统启动 | ✅ |
| **系统集成** | OPX 启动后按顺序自动启动管理的软件和应用 | ✅ |
| **国际化** | 中英文语言切换，默认中文 | ✅ |
| **系统配置** | 关闭窗口行为配置（托盘/退出/后台） | ✅ |

## 架构设计

### 整体架构

采用模块化单应用架构（方案 A）：
- 后端：Rust + Tauri 2，按业务分层
- 前端：Vue 3 + TypeScript，按功能模块划分
- 通信：Tauri IPC 命令调用
- 存储：本地 JSON 文件存储，软件相对路径安装

### 项目结构

```
opx/
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── main.rs               # 应用入口
│   │   ├── lib.rs
│   │   ├── commands/             # Tauri 命令层（对外暴露 API）
│   │   │   ├── mod.rs
│   │   │   ├── system.rs         # 系统监控命令
│   │   │   ├── software.rs       # 软件管理命令
│   │   │   ├── springboot.rs     # SpringBoot 管理命令
│   │   │   └── config.rs         # 配置命令
│   │   ├── services/             # 业务服务层
│   │   │   ├── mod.rs
│   │   │   ├── system_monitor/   # 系统监控服务
│   │   │   ├── software_manager/ # 软件管理服务
│   │   │   │   ├── mod.rs
│   │   │   │   ├── providers/    # 软件安装提供者
│   │   │   │   │   ├── mod.rs
│   │   │   │   │   ├── mysql.rs
│   │   │   │   │   ├── jdk.rs
│   │   │   │   │   ├── redis.rs
│   │   │   │   │   └── nginx.rs
│   │   │   │   └── installer.rs  # 安装器核心
│   │   │   └── springboot_manager/# SpringBoot 管理服务
│   │   │       ├── mod.rs
│   │   │       ├── process.rs    # 进程管理
│   │   │       └── starter.rs    # 分组顺序启动
│   │   ├── models/               # 数据模型
│   │   │   ├── mod.rs
│   │   │   ├── system.rs
│   │   │   ├── software.rs
│   │   │   └── springboot.rs
│   │   └── utils/                # 工具函数
│   │       ├── mod.rs
│   │       ├── file.rs
│   │       ├── download.rs
│   │       └── archive.rs
│   └── Cargo.toml
├── src/                          # Vue 前端
│   ├── main.ts                   # 应用入口
│   ├── App.vue
│   ├── components/               # 公共共享组件
│   │   ├── ProgressBar.vue
│   │   ├── StatusBadge.vue
│   │   └── ...
│   ├── modules/                  # 功能模块（按业务分）
│   │   ├── system-monitor/       # 系统监控模块
│   │   │   ├── components/       # 模块私有组件
│   │   │   ├── pages/            # 页面
│   │   │   └── composables/      # 状态逻辑
│   │   ├── software-manager/     # 软件管理模块
│   │   │   ├── components/
│   │   │   ├── pages/
│   │   │   └── composables/
│   │   └── springboot-manager/   # SpringBoot 管理模块
│   │       ├── components/
│   │       ├── pages/
│   │       └── composables/
│   ├── layouts/                  # 布局组件
│   │   ├── MainLayout.vue
│   │   └── Sidebar.vue
│   ├── router/                   # 路由配置
│   │   └── index.ts
│   ├── stores/                   # Pinia 全局状态
│   │   ├── app.ts
│   │   └── settings.ts
│   ├── utils/                    # 工具函数
│   ├── assets/                   # 静态资源
│   ├── styles/                   # 全局样式
│   └── locales/                  # 国际化语言文件
│       ├── zh-CN.ts
│       └── en-US.ts
├── docs/
│   └── superpowers/
│       └── specs/                # 设计文档
├── package.json
├── vite.config.ts
├── tailwind.config.ts
└── tsconfig.json
```

## 界面布局设计

采用经典多面板布局，参考 DBX 设计风格：

```
┌─────────────────────────────────────────────────────────┐
│  [≡]  OPX  │  当前模块                  [主题] [设置] │ ← 顶部导航栏
├───────┬─────────────────────────────────────────────────┤
│       │                                                 │
│  功   │                                                 │
│  能   │                 主内容区域                      │ ← 主内容区
│  列   │                                                 │
│  表   │                                                 │
│       │                                                 │
│       │                                                 │
├───────┴─────────────────────────────────────────────────┤
│  状态栏 │ CPU: 12% 内存: 4.2GB/16GB  软件目录: ./apps │ ← 底部状态栏
└─────────────────────────────────────────────────────────┘
```

### 左侧功能菜单

1. **仪表盘** - 系统监控概览
2. **软件管理** - 已安装软件管理
3. **软件仓库** - 安装新软件
4. **SpringBoot** - SpringBoot 应用管理
5. **设置** - 应用设置

### 设计要点

- 现代化简约风格，支持明暗主题
- 左侧边栏可折叠
- 支持多标签页同时打开多个功能
- 底部状态栏常驻显示关键系统信息

## 数据存储设计

### 运行时目录结构

应用安装后目录结构（所有路径相对可迁移）：

```
<opx-root>/
├── opx.exe (或 opx 可执行文件)
├── config/                # 配置目录
│   ├── settings.json      # 全局应用设置
│   ├── installed.json     # 已安装软件元数据
│   └── springboot.json    # SpringBoot 应用配置
├── apps/                  # 安装的软件根目录
│   ├── mysql/             # MySQL 安装目录
│   ├── jdk/               # JDK 安装目录
│   ├── redis/             # Redis 安装目录
│   ├── nginx/             # Nginx 安装目录
│   ├── custom/            # 用户自定义安装软件
│   └── springboot/        # SpringBoot 应用
├── logs/                  # 应用日志
└── temp/                  # 临时文件（下载解压用）
```

### 数据模型

#### 全局设置 (`config/settings.json`)

```json
{
  "theme": "auto",
  "language": "zh-CN",         // 语言：zh-CN / en-US
  "sidebarCollapsed": false,
  "softwareRoot": "apps",
  "configRoot": "config",
  "mirrorUrl": "https://mirrors.opx.dev",
  "autoCheckUpdate": true,
  "closeWindowAction": "minimize-to-tray", // minimize-to-tray | exit | background-service
  "registerAsSystemService": false,    // OPX 自身是否注册为系统服务
  "autoStartManagedServices": true,    // 系统服务启动后是否自动启动管理的软件
  "startOnSystemBoot": false           // （过时，由 registerAsSystemService 替代）
}
```

#### 已安装软件 (`config/installed.json`)

```json
{
  "software": [
    {
      "id": "uuid",
      "key": "mysql",
      "name": "MySQL 8.0.36",
      "version": "8.0.36",
      "installPath": "apps/mysql",
      "installTime": "2026-06-25T10:00:00Z",
      "status": "running",
      "port": 3306,
      "config": {
        "dataDir": "data"
      },
      "isCustom": false,
      "autoStartOnAppStart": true,   // OPX 启动时是否自动启动该软件
      "startupOrder": 10             // 启动顺序（越小越先启动）
    }
  ]
}
```

#### SpringBoot 应用 (`config/springboot.json`)

```json
{
  "applications": [
    {
      "id": "uuid",
      "name": "用户认证服务",
      "jarPath": "apps/springboot/auth.jar",
      "version": "1.0.0",
      "env": "dev",
      "port": 8080,
      "jvmOpts": "-Xmx512m",
      "args": "",
      "status": "running",
      "logPath": "apps/springboot/auth.log",
      "startTime": "2026-06-25T10:00:00Z",
      "backupEnabled": true,
      "autoRestart": false,
      "group": "group-1",
      "autoStartOnAppStart": true,   // OPX 启动时是否自动启动该应用
      "startupOrder": 100            // 启动顺序（越小越先启动）
    }
  ],
  "groups": [
    {
      "id": "group-1",
      "name": "基础服务",
      "order": 1,
      "dependsOn": []
    },
    {
      "id": "group-2",
      "name": "业务应用",
      "order": 2,
      "dependsOn": ["group-1"]
    }
  ]
}
```

### 启动顺序逻辑

分组依赖启动流程：

1. 读取所有分组，按 `order` 排序
2. 拓扑排序确认依赖关系，检测循环依赖
3. 对每一层分组：
   - 分组内所有应用**并行启动**
   - 等待全部启动成功或任一失败
   - 全部成功后继续启动下一层依赖分组
   - 任一失败则终止启动并报告错误

## 后端 API 设计（Tauri Commands）

### 系统监控模块

```rust
// 获取系统实时信息
system_info() -> Result<SystemInfo>

// 获取历史统计数据
system_history(hours: u32) -> Result<HistoryData>

// 获取进程列表
process_list() -> Result<Vec<ProcessInfo>>

// 结束进程
kill_process(pid: u32) -> Result<()>
```

### 软件管理模块

```rust
// 获取可安装软件列表（预置）
list_available_software() -> Result<Vec<SoftwareMeta>>

// 获取已安装软件列表
list_installed_software() -> Result<Vec<InstalledSoftware>>

// 安装预置软件
install_software(params: InstallParams) -> Result<()>

// 安装用户上传软件
install_custom(archive_path: String, name: String) -> Result<()>

// 启动软件
start_software(id: String) -> Result<()>

// 停止软件
stop_software(id: String) -> Result<()>

// 重启软件
restart_software(id: String) -> Result<()>

// 读取软件配置文件
get_software_config(id: String) -> Result<String>

// 保存软件配置文件
save_software_config(id: String, content: String) -> Result<()>

// 卸载软件
uninstall_software(id: String) -> Result<()>
```

### SpringBoot 管理模块

```rust
// 获取所有应用列表
list_applications() -> Result<Vec<SpringApp>>

// 获取所有分组列表
list_groups() -> Result<Vec<AppGroup>>

// 保存应用（新增/编辑）
save_application(app: SpringApp) -> Result<()>

// 删除应用
delete_application(id: String) -> Result<()>

// 启动单个应用
start_application(id: String) -> Result<()>

// 按分组顺序启动所有应用
start_all_applications() -> Result<()>

// 停止单个应用
stop_application(id: String) -> Result<()>

// 停止所有应用
stop_all_applications() -> Result<()>

// 重启单个应用
restart_application(id: String) -> Result<()>

// 获取 JVM 监控信息
get_jvm_info(id: String) -> Result<Option<JvmInfo>>

// 获取应用日志尾部
get_logs(id: String, max_lines: u32) -> Result<String>

// 上传并更新应用 jar
upload_jar(app_id: String, source_path: String) -> Result<()>

// 回滚到上一版本
rollback_application(id: String) -> Result<()>

// 保存分组
save_group(group: AppGroup) -> Result<()>

// 删除分组
delete_group(id: String) -> Result<()>

// 注册/取消注册 OPX 自身为系统服务
toggle_opx_system_service(enable: bool) -> Result<()>
```

### 配置模块

```rust
// 获取全局设置
get_settings() -> Result<AppSettings>

// 保存全局设置
save_settings(settings: AppSettings) -> Result<()>
```

## 核心模块设计

### 软件管理提供者模式

每种预置软件实现一个 `SoftwareProvider` trait：

```rust
trait SoftwareProvider {
    // 获取软件元信息（名称、描述、可用版本）
    fn meta(&self) -> SoftwareMeta;

    // 获取下载 URL
    fn download_url(&self, version: &str) -> String;

    // 安装后配置初始化
    fn post_install(&self, ctx: &mut InstallContext) -> Result<()>;

    // 获取启动命令
    fn start_command(&self, software: &InstalledSoftware) -> Command;

    // 获取停止命令
    fn stop_command(&self, software: &InstalledSoftware) -> Result<()>;
}
```

新增预置软件只需要添加一个新的实现，不需要修改核心代码，符合开闭原则。

### SpringBoot 进程管理

- Rust 后端 `std::process::Command` 启动子进程
- PID 持久化保存到配置，退出时通过 PID 停止进程
- 标准输出和标准错误重定向到日志文件
- 定期检测进程状态，更新状态到 UI

### 系统服务实现

**架构：**
- 只需要把 **OPX 自身**注册为系统服务
- OPX 启动后，按照用户配置的 `autoStartOnAppStart` 和 `startupOrder` **自动启动**它管理的软件和应用
- 不需要把每个软件单独注册为系统服务，由 OPX 统一控制启动顺序

**多平台支持：**

| 平台 | 服务管理方式 |
|------|-------------|
| Windows | 注册为 Windows Service 使用 `windows-service` crate |
| Linux | 注册为 systemd 单元文件 |
| macOS | 注册为 launchd plist |

**自动启动顺序：**
- 所有设置了 `autoStartOnAppStart = true` 的软件和应用，按 `startupOrder` 从小到大排序启动
- 默认顺序：
  - MySQL: `10`
  - Redis: `20`
  - Nginx: `30`
  - SpringBoot 应用: `100+`（根据分组顺序）
- 用户可手动修改每个软件/应用的启动顺序
- 同顺序的软件并行启动，不同顺序按顺序依次启动

### 国际化（i18n）

- 使用 `vue-i18n` 实现国际化
- 默认语言：中文（zh-CN）
- 支持切换：英文（en-US）
- 语言配置保存在全局设置中
- 所有 UI 文本统一从语言文件读取

### 系统托盘和关闭行为

- 支持系统托盘图标，托盘菜单可快速打开窗口、退出应用
- 可配置关闭窗口行为：
  1. **最小化到系统托盘**（默认推荐）- 窗口关闭，程序继续在托盘运行
  2. **直接退出程序** - 关闭窗口即退出应用
  3. **后台服务模式** - 退出窗口后所有已启动服务继续运行

## 技术选型

| 层次 | 技术 | 说明 |
|------|------|------|
| 框架 | Tauri 2 | 跨平台桌面应用框架，体积小性能好 |
| 后端语言 | Rust | 内存安全，系统编程能力强 |
| 前端框架 | Vue 3 | 渐进式框架，开发体验好 |
| 语言 | TypeScript | 类型安全，减少错误 |
| 构建工具 | Vite | 快速热更新 |
| CSS 框架 | Tailwind CSS v4 | 工具类 CSS，快速开发 |
| 组件库 | shadcn-vue | 参考 DBX，现代化组件 |
| 图表 | Chart.js | 轻量级图表，满足需求 |
| 国际化 | vue-i18n | Vue 国际化方案 |
| 系统托盘 | tauri-plugin-tray | Tauri 系统托盘原生支持 |
| 系统信息 | sys-info | Rust 系统信息获取库 |
| Windows 服务 | windows-service | Rust Windows 服务支持 |

## 新增功能总结

根据补充需求，新增以下内容：

1. **系统服务注册** - **OPX 自身**可注册为系统服务，随系统自动启动在后台运行
2. **自动启动管理** - OPX 启动后，根据用户配置按顺序自动启动管理的软件（MySQL/Redis/Nginx）和 SpringBoot 应用
3. **启动顺序控制** - 每个软件/应用配置 `startupOrder`，数值越小越先启动，默认：MySQL(10) → Redis(20) → Nginx(30) → SpringBoot(100+)
4. **国际化 i18n** - 使用 vue-i18n，支持中文/英文切换，默认中文
5. **系统托盘** - 支持托盘图标，可配置关闭窗口行为：最小化到托盘（默认）/ 退出程序 / 后台服务模式

## 规格自检

| 检查项 | 状态 | 说明 |
|--------|------|------|
| 占位符扫描 | ✅ 无 | 没有 TODO 或未完成章节 |
| 内部一致性 | ✅ 通过 | 架构、功能、API 描述一致 |
| 范围检查 | ✅ 合适 | 一个实现计划可以覆盖，无需拆分 |
| 模糊性检查 | ✅ 通过 | 所有需求都明确定义 |

修正要点：只将 OPX 自身注册为系统服务，由 OPX 统一按配置顺序启动管理的软件和应用，不需要给每个软件单独注册系统服务。

---

设计文档已更新完成，请审查。如果需要任何修改，请告诉我。如果没问题，我们下一步将创建实现计划。
