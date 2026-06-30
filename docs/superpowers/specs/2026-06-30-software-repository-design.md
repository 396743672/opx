# 软件仓库模块设计文档

**日期**: 2026-06-30
**项目位置**: `D:\object\opx`
**技术栈**: Rust + Tauri 2 + Vue 3 + TypeScript + Tailwind CSS
**阶段**: 第二阶段 — 软件仓库（安装新软件）

## 概述

软件仓库模块负责"把软件装进 `apps/` 并登记到 `installed.json`"。本阶段聚焦安装流程，不涉及启动/停止/配置编辑/卸载——那些属"软件管理"下一阶段。

### 范围边界

**包含**：
- 预置软件（MySQL / JRE / Redis / Nginx）的在线镜像安装
- 用户自定义压缩包上传安装
- Catalog 管理（内置 + 远程合并）
- 安装进度推送与 UI 反馈
- 多版本共存（同 key 可装多个版本）

**不包含**（留到下一阶段"软件管理"）：
- 软件启动 / 停止 / 重启
- 配置文件编辑
- 卸载
- MySQL data 目录初始化（首次启动时做）
- Nginx 版本更新操作（本阶段只在 UI 标记"可更新"）

### 关键约束（用户确认）

1. **下载源**：国内镜像，每个软件配置多个镜像源，下载时可选
2. **JRE 替代 JDK**：用 Eclipse Temurin JRE，最低 1.8 LTS
3. **版本策略**：
   - MySQL：8.0.36, 8.4.0（均为 LTS，最低 8+）
   - JRE：1.8 (8u421), 11.0.22, 17.0.10, 21.0.2（均为 LTS）
   - Redis：7.2.4, 7.4.0（偶数版本作为稳定版）
   - Nginx：只跟踪最新版（不列多版本），随远程 catalog 更新
4. **Catalog 来源**：内置 + 远程 catalog 合并（远程按 key 整体覆盖）
5. **多版本共存**：同 key 可同时安装多个版本
6. **JRE 全局默认**：`settings.json` 存 `jre_default_id`，安装时可勾选"设为默认"
7. **Nginx 可更新提示**：本阶段在 UI 标记，更新操作留到下阶段

## 架构与模块边界

### 后端分层

遵循现有 `commands/ services/ models/ utils/` 分层。

```
src-tauri/src/
├── commands/software.rs          # Tauri 命令层（薄壳，参数校验后转调 service）
├── services/software_manager/    # 业务服务层（新增子目录，替换现有空文件）
│   ├── mod.rs                    # SoftwareManager：编排下载→校验→解压→登记
│   ├── catalog.rs                # 内置 catalog + 远程合并
│   ├── installer.rs              # 通用安装器（下载/解压/进度事件/查重/清理）
│   └── providers/                # 每种软件一个 provider
│       ├── mod.rs                # SoftwareProvider trait
│       ├── mysql.rs
│       ├── jre.rs                # ← 替代原设计 jdk.rs，用 JRE
│       ├── redis.rs
│       └── nginx.rs
├── models/software.rs            # 扩展：多版本字段、镜像源、校验信息、InstallSource
└── utils/                        # 复用现有 archive.rs / paths.rs；升级 download.rs
```

**关键改动**：现有 `services/software_manager.rs` 是空文件，本阶段改为 `services/software_manager/mod.rs` 子目录形式，把职责拆分到 catalog/installer/providers 子模块。

### 前端结构

```
src/modules/software-manager/
├── pages/
│   └── RepositoryPage.vue        # 替换占位符，改为完整实现
└── components/                   # 新增目录
    ├── SoftwareCard.vue           # 单个软件卡片
    ├── InstallDialog.vue          # 安装对话框（在线镜像，通用）
    ├── InstallJreDialog.vue       # JRE 安装对话框（多出"设为默认"勾选）
    ├── InstallProgressDialog.vue  # 安装进度对话框
    └── CustomInstallDialog.vue    # 自定义上传对话框
```

`SoftwareListPage.vue` 本阶段不动（仍占位，下一阶段做）。

### 复用现有基础设施

- `utils/paths::apps_dir()` — 安装根目录
- `utils/archive::{extract_zip, extract_tar_gz}` — 解压
- `utils/paths::config_dir()` / `tmp_dir()` — 配置与临时目录
- Tauri event 机制（与启动加载遮罩、`StopProgressDialog` 同套 emit/listen 模式）
- 设计令牌与组件风格（与 `design-mockup.html`、`CloseDialog`、`StopProgressDialog` 一致）

## 数据模型

扩展 `models/software.rs`，新增 catalog 相关模型，改造 `InstalledSoftware` 支持多版本共存。

### Catalog 模型（新增）

```rust
pub struct CatalogVersion {
    pub version: String,              // "8.0.36"
    pub mirrors: Vec<MirrorSource>,   // 多个镜像源
    pub archive: ArchiveInfo,         // 压缩包信息
}

pub struct MirrorSource {
    pub name: String,      // "清华镜像" / "华为镜像" / "官方"
    pub url: String,       // 完整下载 URL
}

pub struct ArchiveInfo {
    pub format: ArchiveFormat,  // Zip / TarGz
    pub size: Option<u64>,      // 字节数（可选，用于显示）
    pub sha256: Option<String>, // 校验和（可选，有则校验）
}

pub enum ArchiveFormat { Zip, TarGz }

pub struct CatalogEntry {
    pub key: String,                  // "mysql"
    pub name: String,                 // "MySQL"
    pub description: String,
    pub category: SoftwareCategory,   // Database / Runtime / Cache / WebServer
    pub icon: String,                 // iconify 图标名，前端展示用
    pub versions: Vec<CatalogVersion>,
    pub default_version: String,
}

pub enum SoftwareCategory { Database, Runtime, Cache, WebServer }

pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
    pub updated_at: Option<String>,   // 远程 catalog 的更新时间，本地内置为 None
}
```

### InstalledSoftware 改造（支持多版本）

```rust
pub struct InstalledSoftware {
    pub id: String,                    // uuid
    pub key: String,                   // "mysql"
    pub version: String,               // "8.0.36" —— 多版本共存的关键
    pub name: String,                  // "MySQL 8.0.36"
    pub install_path: String,          // "apps/mysql/8.0.36" —— 含版本子目录
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,        // 本阶段固定 Unknown（不启动），下阶段实现
    pub port: u16,                     // 0 表示未配置
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
    pub source: InstallSource,         // 新增：来源
}

pub enum InstallSource {
    Mirror { mirror_name: String, url: String },  // 在线镜像安装
    Custom { archive_name: String },              // 用户上传压缩包
}
```

**关键变更**：
- `install_path` 从 `apps/mysql` → `apps/mysql/8.0.36`（多版本共存，自动派生，不让用户填）
- `version` 字段保留，配合 path 共同定位
- 新增 `InstallSource` 记录来源，便于"软件管理"阶段区分启停方式
- `status` 本阶段固定 `Unknown`，下阶段实现启停后改为实际状态
- 删除原 `InstallParams.install_path` 字段——安装路径由 `apps/{key}/{version}/` 自动派生

### 安装参数

```rust
pub struct InstallParams {
    pub key: String,                  // "mysql" / "jre" / ...
    pub version: String,              // "8.0.36"
    pub mirror_index: usize,          // 选第几个镜像源（0-based）
    pub set_as_default_jre: bool,     // 仅 key=="jre" 时有效
}

pub struct CustomInstallParams {
    pub name: String,                 // 用户自定义名称
    pub archive_path: String,         // 本地压缩包路径（Tauri 弹窗选择）
}
```

### AppSettings 扩展

`settings.json` 新增字段：

```rust
pub jre_default_id: Option<String>,  // None 表示未设默认 JRE
```

### 持久化文件

`config/installed.json` 结构（与原设计兼容，`software` 数组中同 key 可有多条）：

```json
{
  "software": [
    { "id": "uuid", "key": "mysql", "version": "8.0.36", "install_path": "apps/mysql/8.0.36", "source": { "Mirror": { "mirror_name": "清华镜像", "url": "..." } }, ... },
    { "id": "uuid", "key": "mysql", "version": "8.4.0", "install_path": "apps/mysql/8.4.0", ... },
    { "id": "uuid", "key": "my-tool", "version": "", "install_path": "apps/custom/my-tool", "is_custom": true, "source": { "Custom": { "archive_name": "my-tool-1.2.0.zip" } }, ... }
  ]
}
```

## Catalog 设计

### 内置 Catalog

每个 provider 在代码里声明自己的 catalog 条目，`providers/mod.rs` 聚合所有 provider 的 `catalog_entry()` 构成内置 catalog。

```rust
trait SoftwareProvider {
    fn key(&self) -> &str;
    fn catalog_entry(&self) -> CatalogEntry;        // 内置版本+镜像
    fn post_install(&self, ctx: &InstallContext) -> Result<()>;  // 最小化钩子
    // 下一阶段再加 start_command / stop_command 等
}
```

### 内置版本与镜像（初版）

| 软件 | 版本（均为 LTS/稳定） | 镜像源 | format |
|---|---|---|---|
| **MySQL** | 8.0.36, 8.4.0 | 清华、华为、官方 | Zip |
| **JRE** | 1.8 (8u421), 11.0.22, 17.0.10, 21.0.2 | 华为、清华、官方 | TarGz (Linux/Mac) / Zip (Windows) |
| **Redis** | 7.2.4, 7.4.0 | 华为、GitHub Release | Zip (Windows) / TarGz (Linux/Mac) |
| **Nginx** | 最新版（随远程 catalog 更新） | 华为、官方 | Zip |

**JRE 选型**：Eclipse Temurin（原 AdoptOpenJDK）JRE 包，LTS 版本 8/11/17/21，最低 1.8。Windows 用 `.zip`，Linux/Mac 用 `.tar.gz`。

**平台适配**：`catalog_entry()` 根据 `cfg!(target_os)` 返回对应格式的 archive。同一版本在不同平台是不同的 URL 和格式。

**Nginx 特殊处理**：`default_version` 永远指向最新版本，`versions` 数组只含一个元素（最新版）。下阶段"软件管理"页对比 `installed.version != catalog.default_version` 判断可更新。

### 远程 Catalog

**位置**：`{settings.mirrorUrl}/catalog.json`（默认 `https://mirrors.opx.dev/catalog.json`，可配置）

**结构**：与内置 `Catalog` 完全相同的 JSON schema，远程可只包含部分软件/版本。

**合并策略**：
1. 启动时异步拉取（不阻塞 UI），10 秒超时，失败静默回退内置
2. 合并规则：以 `key` 为单位，远程条目**整体覆盖**内置同名 key（而非逐版本 merge）——简单可控
3. 远程出现新 key（内置没有）→ 直接加入 catalog
4. 合并结果缓存到内存，不写盘（每次启动重新合并，避免脏数据）
5. 提供 `refresh_catalog()` 命令供前端手动刷新

### Catalog 加载流程

```
应用启动
  └─ catalog::load()
      ├─ 1. 收集所有 provider 的 catalog_entry() → 内置 catalog
      ├─ 2. 异步 tokio::spawn 拉取 {mirrorUrl}/catalog.json
      ├─ 3. 拉取成功 → 按 key 覆盖合并
      ├─ 4. 拉取失败 → 仅用内置
      └─ 5. 存入 RwLock<Catalog> 全局状态，commands 读取
```

**全局状态**：用 `tauri::State<RwLock<Catalog>>` 管理。`list_available_software()` 命令从 State 读取。

**为什么用 RwLock 而非 Mutex**：catalog 读多写少（每次打开仓库页读一次，远程合并时写一次）。

## 安装流程

### 在线镜像安装流程

```
前端：选软件→选版本→选镜像源→点"安装"
  │
  ▼ invoke install_software(InstallParams)
后端 install_software 命令：
  1. 校验：key 有效？version 在 catalog？mirror_index 合法？
  2. 查重：installed.json 是否已有同 key+version？已装则报错"该版本已安装"
  3. 查重：内存 install_tasks 是否有同 key+version 正在安装？是则报错"正在安装中"
  4. 派生 install_path = apps/{key}/{version}/
  5. 创建 install_path 目录
  6. 派生临时下载路径 = tmp/{key}-{version}.{ext}
  7. 生成 install_id (UUID v4)，登记到 install_tasks
  8. 立即返回 install_id（不阻塞 IPC）
  │
  ▼ 后台 tokio task
  9. 流式下载：reqwest 异步 stream → 写文件，周期 emit "install-progress" 事件
     ├── 阶段 Downloading：payload { install_id, phase:"downloading", downloaded, total, percent }
     ├── 下载完成
     10. SHA256 校验（若 catalog 提供 sha256）：失败 emit error 事件，清理临时文件
     11. 解压：阶段 Extracting：payload { phase:"extracting", percent:0..100 }
         └── 复用 utils/archive::{extract_zip, extract_tar_gz}
     12. post_install(provider)：最小化钩子
         ├── JRE/Redis/Nginx：空实现
         └── MySQL：写默认 my.ini 模板（含 basedir/datadir/port=3306），不初始化 data
     13. 登记：写 installed.json，新增 InstalledSoftware 记录
         ├── 若 key=="jre" && set_as_default_jre → 同步 settings.jre_default_id
     14. emit "install-completed" { install_id, installed_id }
  └── 任一步失败：emit "install-failed" { install_id, error, stage } + 清理临时文件 + 清理半成品目录
```

### 流式下载（升级 utils/download.rs）

现有 `download.rs` 用 `response.bytes()` 全量读入内存——MySQL ~200MB 会爆。需改造为异步流式：

```rust
// utils/download.rs 新增（保留旧 blocking::download 供其他场景用）
pub async fn download_with_progress<F>(
    url: &str,
    dest: &Path,
    mut on_progress: F,
) -> Result<()>
where F: FnMut(u64, Option<u64>)  // (downloaded, total)
```

用 `reqwest::Response::bytes_stream()` 异步分块写入文件，每收一块调用 `on_progress` 回调。`total` 来自 `Content-Length` 头（可能缺失，则 `None`，进度只显示已下载字节）。

### 进度事件协议

Tauri event 频道名：`install-progress`（前端用 `@tauri-apps/api/event::listen` 监听）

```typescript
type InstallEvent =
  | { install_id: string; phase: 'downloading'; downloaded: number; total: number | null; percent: number | null }
  | { install_id: string; phase: 'extracting'; percent: number }
  | { install_id: string; phase: 'completed'; installed_id: string }
  | { install_id: string; phase: 'failed'; error: string; stage?: 'download' | 'extract' | 'post_install' }
```

`percent` 为 null 的情况（下载阶段 total 未知）：前端显示"已下载 XX MB"而非百分比。

### 并发与取消

- **并发**：同一时间允许多个安装任务并行（每个 task 独立 install_id 和临时文件）。但同 key+version 查重时若发现"正在安装中"则拒绝
- **取消**：本阶段**不实现**取消（YAGNI，下阶段若需要再补）。安装失败会自动清理，用户重启即可重来
- **install_id 生成**：UUID v4

### 自定义上传安装流程

```
前端：填名称→选压缩包（Tauri dialog open）→点"安装"
  │
  ▼ invoke install_custom(CustomInstallParams)
后端：
  1. 校验：name 非空且匹配 ^[a-zA-Z0-9_-]+$；name 不与已有 custom 冲突；archive_path 存在；扩展名 zip/tar.gz
  2. 派生 install_path = apps/custom/{name}/
  3. 直接用原 archive_path（避免大文件拷贝），无需复制到 tmp
  4. 解压到 install_path
  5. 登记：is_custom=true, version="", source=Custom
  6. emit "install-completed"
```

自定义安装无下载阶段、无校验、无 post_install，进度只走 extracting→completed。

### 失败清理策略

| 失败阶段 | 清理动作 |
|---|---|
| 下载中 | 删 tmp 临时文件 |
| 校验失败 | 删 tmp 临时文件 |
| 解压失败 | 删 tmp 临时文件 + 删 install_path 半成品目录 |
| post_install 失败 | 删 install_path（tmp 可留作排查） |
| 登记失败 | 删 install_path |

清理用 `std::fs::remove_dir_all`，但需容忍清理本身失败（避免清理失败掩盖原始错误）：

```rust
fn cleanup_path(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_dir_all(path) {
            log::warn!("清理 {} 失败：{}", path.display(), e);
        }
    }
}
```

清理顺序：先清理 tmp 临时文件，再清理 install_path 半成品目录。两者独立，互不影响。

## Tauri 命令 API

```rust
// 获取可安装软件列表（catalog）
list_available_software() -> Result<Vec<CatalogEntry>>

// 手动刷新 catalog（重新拉取远程）
refresh_catalog() -> Result<()>

// 获取已安装软件列表（供"软件管理"阶段用，本阶段也可用于查重展示）
list_installed_software() -> Result<Vec<InstalledSoftware>>

// 安装预置软件（在线镜像）
install_software(params: InstallParams) -> Result<String>  // 返回 install_id

// 安装用户上传软件
install_custom(params: CustomInstallParams) -> Result<String>  // 返回 install_id
```

**注册**：所有命令在 `lib.rs` 的 `invoke_handler!` 注册。

## 前端 UI 设计

### 整体布局

复用 `PageHeader` + 卡片网格，与 Dashboard 风格一致。按 `SoftwareCategory` 分组：数据库 / 运行时 / Web 服务器 / 自定义。

### 软件卡片

每张卡片含已安装标记。同 key 多版本共存时，卡片显示已装版本 pill + 默认版本标记（JRE）/ 可更新提示（Nginx）。

卡片元素：
- 图标（iconify）
- 名称 + LTS 标签（适用时）
- 描述（一行截断）
- 已装版本 pill（绿色，多个版本多个 pill）
- 默认版本标记（JRE 蓝色 / Nginx 可更新黄色）
- 可选版本数 / 最新版本号
- 安装按钮

### 安装对话框（InstallDialog / InstallJreDialog）

模态对话框，复用 `CloseDialog` 模式（`fixed inset-0 z-50 bg-black/50 backdrop-blur-sm` 遮罩 + `border-border bg-card shadow-popover` 卡片体）。

字段：
- 版本列表（带 LTS 徽标 + 选中勾）
- 镜像源下拉（列出所有 mirrors）
- 安装路径（只读，自动派生 `apps/{key}/{version}/`）
- ☐ 设为全局默认 JRE（仅 JRE 对话框显示）
- 取消 / 开始安装

### 安装进度对话框（InstallProgressDialog）

点"开始安装"后弹出。监听 `install-progress` 事件更新 UI。

元素：
- 阶段图标 + 阶段名（下载中 / 解压中）
- 进度条（`h-2 bg-muted rounded-full` + `h-full bg-primary transition-all`，与 `StopProgressDialog` 一致）
- 已下载/总量 或 百分比
- 元信息（镜像源、速度、剩余时间 或 目标路径、已解压量）

下载 total 未知时进度条用 indeterminate 动画。完成/失败：关闭对话框 + toast 提示。失败时显示错误信息 + "重试"按钮。

### 自定义上传对话框（CustomInstallDialog）

字段：
- 名称输入
- 压缩包选择（Tauri dialog open，显示已选路径 + 更换按钮）
- 安装路径（只读，自动派生 `apps/custom/{name}/`）
- 取消 / 开始安装

### 组件清单

```
src/modules/software-manager/
└── components/
    ├── SoftwareCard.vue
    ├── InstallDialog.vue
    ├── InstallJreDialog.vue
    ├── InstallProgressDialog.vue
    └── CustomInstallDialog.vue
```

`RepositoryPage.vue` 改为：分组渲染 catalog + 管理对话框状态。

### 风格一致性要点

- **对话框**：复用 `CloseDialog`/`StopProgressDialog` 模式——遮罩 `bg-black/50 backdrop-blur-sm` + 卡片体 `border border-border bg-card shadow-popover p-5`
- **按钮**：主按钮 `bg-primary text-primary-foreground hover:bg-primary/90`；次按钮 `hover:bg-muted`
- **输入框/下拉**：`border border-border rounded-md bg-card` + `focus:border-primary`
- **进度条**：与 `StopProgressDialog` 一致——`h-2 w-full bg-muted rounded-full overflow-hidden` + `h-full bg-primary rounded-full transition-all duration-300 ease-out`
- **卡片网格**：参考 Dashboard `StatCard` 的圆角阴影 token
- **数字**：`tnum` class 等宽显示
- **图标**：`@iconify/vue` 的 `mdi:` 系列
- **设计令牌**：复用 `design-mockup.html` 的 oklch 颜色变量、字体、圆角、阴影

UI 预览已生成到 `software-repo-mockup.html`，含 6 种场景切换（主页 / 安装对话框 / JRE 安装 / 下载中 / 解压中 / 自定义上传）。

## 错误处理

后端统一用 `anyhow::Result`，错误以中文消息返回前端。前端按 phase 区分展示。

| 错误场景 | 处理方式 | 用户可见消息 |
|---|---|---|
| key 不在 catalog | 安装前校验，立即返回 Err | "未知软件：{key}" |
| version 不在 catalog | 安装前校验 | "MySQL 不支持版本 8.5.0" |
| mirror_index 越界 | 安装前校验 | "镜像源选择无效" |
| 同 key+version 已安装 | 查重 installed.json | "MySQL 8.4.0 已安装" |
| 同 key+version 正在安装 | 内存 install_tasks 查重 | "MySQL 8.4.0 正在安装中" |
| 下载失败（网络/404） | emit failed 事件 + 清理 tmp | "下载失败：{具体原因}（请尝试其他镜像源）" |
| SHA256 校验失败 | emit failed + 清理 tmp | "文件校验失败，可能已损坏（请尝试其他镜像源）" |
| 解压失败 | emit failed + 清理 tmp + 清理 install_path | "解压失败：{原因}" |
| post_install 失败 | emit failed + 清理 install_path | "安装后配置失败：{原因}" |
| 写 installed.json 失败 | emit failed + 清理 install_path | "登记安装信息失败：{原因}" |
| 自定义：name 含非法字符 | 校验 `^[a-zA-Z0-9_-]+$` | "名称仅允许字母、数字、下划线、连字符" |
| 自定义：name 已存在 | 查重 installed.json 中所有 `is_custom=true` 记录的 key 字段 | "名称 {name} 已存在" |
| 自定义：压缩包格式不支持 | 校验扩展名 | "仅支持 .zip 和 .tar.gz" |

**错误事件 payload**：
```typescript
{ install_id: string; phase: 'failed'; error: string; stage?: 'download' | 'extract' | 'post_install' }
```

`stage` 字段让前端能提示"在哪一步失败"，便于用户判断是网络问题还是文件问题。

### 并发安全

- `installed.json` 读写用 `RwLock<InstalledSoftwareList>`（State 管理），写时持锁
- catalog 用 `RwLock<Catalog>`
- 进行中的安装任务用 `Mutex<HashMap<String, InstallTask>>` 记录 install_id → 任务信息，用于查重和（未来）取消
- 所有锁通过 `tauri::State` 注入，Tauri 自动管理生命周期

## 测试策略

### 后端单元测试

| 模块 | 测试内容 |
|---|---|
| `services/software_manager/catalog.rs` | 内置 catalog 生成（4 软件、版本数正确）；远程合并按 key 覆盖；远程拉取失败回退内置；远程含新 key 合并 |
| `services/software_manager/installer.rs` | install_path 派生 `apps/{key}/{version}/`；查重逻辑（已装/安装中）；SHA256 校验通过/失败 |
| `services/software_manager/providers/*` | 每个 provider 的 `catalog_entry()` 字段完整；`post_install` 钩子不 panic |
| `utils/download.rs` | `download_with_progress` 流式写入正确（mock HTTP server）；total 缺失时进度回调仍触发 |
| `utils/archive.rs` | 已有 zip/tar.gz 测试，无需新增 |
| `models/software.rs` | `InstalledSoftware` 序列化/反序列化 round-trip；多版本共存 JSON 正确解析 |

mock HTTP server 用 `mockito` crate。下载测试用 mockito 返回固定内容，验证文件写入和进度回调。

### 集成测试（Tauri 命令层）

```rust
// tests/software_install.rs
#[test]
fn install_software_writes_to_installed_json() { ... }
#[test]
fn install_duplicate_version_returns_error() { ... }
#[test]
fn install_custom_with_invalid_name_returns_error() { ... }
```

用临时目录隔离（`tempfile` crate），不污染真实 apps/ 目录。

### 前端测试

本阶段不引入测试框架（YAGNI，与现有项目一致——现有前端无测试）。靠手动验证。

## 验证清单

实现完成后手动跑：

1. `cd src-tauri && cargo test` — 后端单元 + 集成测试全绿
2. `npm run build` — vue-tsc 类型检查 + vite 构建无错
3. `npm run tauri:dev` 启动后：
   - 打开软件仓库页，4 个分组、5 张卡片（MySQL/Redis/JRE/Nginx + 自定义）渲染正确
   - 点 MySQL 安装 → 选 8.4.0 → 选清华镜像 → 开始安装 → 进度对话框显示下载→解压→完成
   - 安装完成后 `apps/mysql/8.4.0/` 目录存在，`config/installed.json` 新增一条记录
   - 再次安装同版本 → 报错"已安装"
   - JRE 安装勾选"设为默认" → `settings.json` 的 `jre_default_id` 被更新
   - 自定义上传一个 zip → 解压到 `apps/custom/{name}/` → installed.json 新增 is_custom=true 记录
   - 断网或改错 mirrorUrl → 下载失败，错误提示清晰，tmp 无残留
   - 关闭重启应用 → 已装记录仍存在（持久化验证）

## 依赖新增

### Cargo.toml（src-tauri）

- `uuid` — install_id 生成（features: `v4`）
- `mockito`（dev-dependency）— 下载测试 mock
- `tempfile`（dev-dependency）— 集成测试临时目录
- 现有 `reqwest` 需启用 `stream` feature（流式下载）

### package.json

无新增前端依赖（复用现有 `@tauri-apps/api`、`@iconify/vue`、`pinia`、`vue-i18n`）。

## 国际化

`locales/zh-CN.ts` 和 `locales/en-US.ts` 新增软件仓库相关文案键：

```
installNewSoftware / selectVersion / selectMirror / mirrorSource / installPath / setAsDefaultJre /
defaultJre / installed / installing / downloading / extracting / downloadFailed / extractFailed /
installCompleted / installFailed / uploadCustom / customName / selectArchive / supportedFormats /
refreshCatalog / catalogUpdateFailed / versionAvailable / canUpdate / latestVersion / retry
```

## 设计决策总结

| 决策点 | 选择 | 理由 |
|---|---|---|
| 模块范围 | 仅安装新软件 | 聚焦，启动/卸载属下阶段 |
| 下载源 | 国内多镜像可选 | 国内用户体验 |
| Catalog 来源 | 内置 + 远程合并 | 离线可用 + 可更新 |
| JRE vs JDK | JRE（Eclipse Temurin） | 运行环境足够，体积小 |
| 多版本共存 | 允许 | 灵活，路径 `apps/{key}/{version}/` |
| JRE 默认版本存储 | settings.json `jre_default_id` | 与 installed.json 解耦 |
| Nginx 版本 | 只跟踪最新 | 用户需求，更新逻辑下阶段 |
| 安装路径 | 自动派生 | 避免冲突，不让用户填 |
| 取消功能 | 本阶段不做 | YAGNI |
| 进度事件 | 单一频道 + phase 区分 | 简单，与现有 stop-progress 模式一致 |
| MySQL post_install | 只写 my.ini 模板 | data 初始化需首次启动，属下阶段 |
| 前端测试 | 不引入框架 | 与现有项目一致 |

## 规格自检

| 检查项 | 状态 | 说明 |
|---|---|---|
| 占位符扫描 | ✅ 无 | 没有 TODO 或未完成章节 |
| 内部一致性 | ✅ 通过 | 架构、数据模型、API、UI、测试各节一致 |
| 范围检查 | ✅ 合适 | 聚焦安装，一个实现计划可覆盖 |
| 模糊性检查 | ✅ 通过 | 所有需求明确定义，边界清晰 |

---

设计文档已编写完成，请审查。
