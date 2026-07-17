# OPX — 本地开发环境管理桌面工具

OPX 是一个 Windows 桌面应用，用于管理本地开发环境中的基础设施软件和服务。提供 MySQL、Redis、Nginx、MinIO 等中间件的安装、启动、停止、配置管理，以及 Java SpringBoot 应用的全生命周期管理和静态网站托管功能。

## 功能概览

| 模块 | 功能 | 状态 |
|------|------|------|
| **软件仓库** | 软件分类浏览、在线/离线安装、版本选择、镜像切换 | ✅ |
| **软件管理** | 启动/停止/重启、配置编辑（表单/源码）、健康检查、启动设置、卸载 | ✅ |
| **网站管理** | 站点增删改查、nginx 配置表单/源码编辑、静态文件上传、启用/停用 | ✅ |
| **应用管理** | SpringBoot 应用注册、JVM 参数调优（推荐参数生成）、环境变量（全局/分组/应用三级）、日志检测启动 | ✅ |
| **系统监控** | CPU/内存/磁盘/网络使用率、实时趋势图（Chart.js）、系统信息 | ✅ |
| **自定义软件** | 自定义启动命令、配置文件路径、健康检查规则 | ✅ |

## 系统架构

```
┌─────────────────────────────────────────────┐
│                 前端 (Vue 3 + TypeScript)     │
│  Dashboard  │  Software  │  Websites  │ App  │
├─────────────────────────────────────────────┤
│             Tauri IPC (invoke)              │
├─────────────────────────────────────────────┤
│               后端 (Rust)                     │
│  ├─ commands/    — Tauri 命令层              │
│  ├─ services/   — 业务逻辑层                  │
│  │  ├─ software_manager/  — 软件生命周期      │
│  │  ├─ springboot_manager/ — SpringBoot 管理  │
│  │  └─ website_manager/   — 网站管理           │
│  ├─ models/     — 数据模型                    │
│  └─ utils/      — 工具函数                    │
├─────────────────────────────────────────────┤
│          本地存储 (JSON 文件)                 │
│  apps.json  │  installed.json  │  settings   │
└─────────────────────────────────────────────┘
```

## 支持管理的软件

| 软件 | 版本管理 | 配置编辑 | 健康检查 | 离线包 |
|------|---------|---------|---------|-------|
| MySQL 8.x | ✅ 在线/离线 | ✅ 表单+源码 | ✅ TCP | ✅ |
| Redis 7.x | ✅ 在线/离线 | ✅ 源码 | ✅ TCP | ✅ |
| Nginx 1.x | ✅ 内置 | ✅ 源码 | ✅ HTTP | ❌ |
| MinIO | ✅ 离线 | ❌ 无表单 | ✅ HTTP | ✅ |
| JDK/JRE | ✅ 内置 | ❌ | ✅ Process | ✅ |
| RustFS | ✅ 在线 | ❌ | ✅ TCP | ❌ |
| 自定义软件 | ✅ 用户上传 | ✅ 自定义 | ✅ 自定义 | ❌ |

## 技术栈

- **前端**: Vue 3 + TypeScript + Vite + Pinia + Chart.js + Tailwind CSS
- **后端**: Rust + Tauri v2 + reqwest + tokio + serde + sysinfo
- **数据库**: 本地 JSON 文件存储（app.json、installed.json、settings.json）
- **日志**: tracing + tracing-appender（每日分割，保留 7 天，操作日志记录所有关键操作）
- **代理**: GitHub 加速代理（ghfast.top）+ 全局 HTTP 代理配置
- **主题**: 三套主题（浅色/深色/暖色）+ 窗口自定义标题栏

## 快速开始

```bash
# 安装依赖
npm install

# 开发模式运行
npm run tauri:dev

# 生产构建
npm run tauri:build
```

## 功能详情

### 软件管理
- 内置软件仓库，支持在线获取版本列表和镜像切换
- 离线内置包（MySQL、Redis、MinIO、JDK/JRE）无需网络即可安装
- 启动/停止/重启，支持首次初始化（如 MySQL 自动执行 --initialize-insecure）
- 配置编辑（表单模式 + 源码模式），保存后自动生效并 reload
- 健康检查（TCP 端口探测 / HTTP 状态码 / 进程存活）

### 网站管理
- 基于 nginx 的静态站点和反向代理站点管理
- 表单编辑路由规则，支持静态目录和代理两种模式
- 源码模式直接编辑 .conf 文件
- 站点启用/停用即 reload nginx

### 应用管理
- SpringBoot JAR 应用注册和全生命周期管理
- JVM 参数推荐生成（基于 Xmx 自动计算 G1 调优参数）
- 三级环境变量覆盖（全局 → 分组 → 应用）
- UTF-8 字符集勾选、全局/分组环境变量配置
- 日志检测启动状态（每 5s 检测 console.log 的 "Started " 标记，不等固定超时）

### 系统监控
- 实时 CPU、内存、磁盘、网络使用率
- 60 秒历史趋势图，每秒刷新
- 已安装软件、运行中服务、运行应用概览卡片

### 自定义标题栏
- 原生窗口装饰禁用，自定义标题栏
- 支持拖拽、双击最大化/还原、最小化/最大化/关闭按钮
- 随主题切换自动更新样式

## 主题切换

支持三种主题模式，点击顶部栏主题图标循环切换：

- **浅色** — 中性灰底 + 蓝宝石强调
- **深色** — 中性暖灰基底 + 琥珀强调
- **暖色** — 暖白纸张基底 + 琥珀强调

主题偏好保存在系统设置中，支持跟随系统设置（auto）。

## 下载代理

系统设置支持两种代理配置：

1. **GitHub 加速代理** — 下载 GitHub 资源时前缀代理 URL（默认 `https://ghfast.top`）
2. **全局 HTTP 代理** — 所有下载请求通过指定代理（VPN 类优先，直连 GitHub）

代理配置保存后立即生效，无需重启。

## 贡献

本项目为个人开发环境管理工具，欢迎提 Issue 和 PR。

## 许可

MIT

___

> 以下内容为 Tauri + Vue 3 项目构建与打包指南

## Tauri + Vue 3 + TypeScript + I18n 项目最佳实践

- **项目初始化检查清单**：
  - 必须创建 `src-tauri/icons` 目录并添加应用图标
  - 必须完善 `Cargo.toml` 中的元数据（license、repository、authors）
  - 必须安装 `@types/node` 以支持 Node.js 类型提示
  - 构建命令需在 `package.json` 中正确配置
- **验证流程**：
  - 始终先运行 `npm install` 安装依赖
  - 运行 `npm run build` 验证生产构建
  - 运行 `npm run tauri info` 检查 Tauri 环境

## 打包步骤

本项目用 Tauri v2 打包，`package.json` 已配置 `"tauri": "tauri"` 脚本，`tauri.conf.json` 中 `build.beforeBuildCommand = "npm run build"` 会在打包前自动执行前端构建（vue-tsc 类型检查 + vite 构建），`bundle.targets = "all"` 在 Windows 上生成 `.msi` 与 `-setup.exe` 安装包。

### 环境前置条件（Windows）
- **Rust 工具链**：`rustup` 安装（https://rustup.rs/），`cargo --version` 可用
- **Visual Studio Build Tools**：含「使用 C++ 的桌面开发」工作负载（MSVC + Windows SDK），首次打包需编译全部 Rust 依赖，耗时较长
- **WebView2 Runtime**：Windows 11 已预装；Windows 10 需安装（打包出的安装包会自动引导安装）
- **Node.js + npm**：`npm install` 安装前端依赖

### 打包命令

`package.json` 已配置便捷脚本，可直接 `npm run <脚本名>` 调用。

#### 1. 准备依赖（首次或 package.json/Cargo.toml 变更后）
```bash
npm install              # 安装前端依赖
```

#### 2. Release 打包（生成可分发安装包）
```bash
# 一键打包：自动跑前端构建(vue-tsc+vite) + Rust release 编译 + 生成 MSI 与 NSIS 安装包 + 免安装 exe
npm run tauri:build

# 仅生成 NSIS .exe 安装包（跳过 MSI，无需 WiX）—— 国内推荐
npm run tauri:build:nsis

# 仅生成 MSI 安装包（跳过 NSIS，需 WiX 工具）
npm run tauri:build:msi
```

> **重要**：免安装 exe（`target/release/opx.exe`）由 `tauri build` 流程产出，**前端资源已嵌入**，可独立运行。不要用 `cargo build --release` 产免安装 exe——它不触发 Tauri 资源嵌入流程，产出的 exe 运行后页面空白。

#### 3. Debug 打包（快速验证，不优化，编译快）
```bash
# Debug 模式打包，产物在 target/debug/ 下，生成安装包 + 免安装 exe
npm run tauri:build:debug

# 开发模式运行（热重载，不打包）
npm run tauri:dev
```

#### 4. 仅前端构建（验证类型 + 产物）
```bash
# vue-tsc 类型检查 + vite 构建，产出 dist/ 静态资源
npm run build
```

#### 5. 查看环境信息
```bash
npm run tauri:info        # 检查 Tauri/Rust/Node 版本与插件状态
```

#### 6. 清理构建产物
```bash
# 清理 Rust 构建缓存（release + debug），下次打包会全量重编译
cd src-tauri && cargo clean

# 仅清理前端 dist
rm -rf dist
```

### 命令-产物对照表

| 命令 | 产物 | 路径 | 用途 |
|---|---|---|---|
| `npm run tauri:build` | MSI + NSIS 安装包 + 免安装 exe | `src-tauri/target/release/bundle/{msi,nsis}/` + `target/release/opx.exe` | 完整发版分发 |
| `npm run tauri:build:nsis` | 仅 NSIS 安装包 + 免安装 exe | `src-tauri/target/release/bundle/nsis/opx_0.1.0_x64-setup.exe` + `target/release/opx.exe` | 普通用户分发（国内推荐，无需 WiX） |
| `npm run tauri:build:msi` | 仅 MSI 安装包 + 免安装 exe | `src-tauri/target/release/bundle/msi/opx_0.1.0_x64_zh-CN.msi` + `target/release/opx.exe` | 企业部署（组策略友好） |
| `npm run tauri:build:debug` | Debug 安装包 + Debug 免安装 exe | `src-tauri/target/debug/bundle/nsis/` + `target/debug/opx.exe` | 快速验证打包流程 |
| `npm run tauri:dev` | 开发模式运行（不产文件） | — | 开发调试，热重载 |
| `npm run build` | 前端静态资源 | `dist/` | 仅前端构建验证 |
| `npm run tauri:info` | 环境信息（不产文件） | — | 检查工具链 |
| `npm run tauri dev` | 开发模式运行（不产文件） | — | 开发调试，热重载 |
| `cd src-tauri && cargo build --release` | 免安装 exe | `src-tauri/target/release/opx.exe` | 仅要 exe，不要安装包 |
| `npm run build` | 前端静态资源 | `dist/` | 仅前端构建验证 |

### 产物位置总览
打包产物在 `src-tauri/target/release/bundle/` 下：
- `msi/opx_0.1.0_x64_zh-CN.msi` — MSI 安装包（推荐企业分发，组策略部署友好）
- `nsis/opx_0.1.0_x64-setup.exe` — NSIS 安装程序（推荐普通用户分发）
- `src-tauri/target/release/opx.exe` — 免安装可执行文件（需目标机有 WebView2，Win11 自带）

> **注意**：MSI 与 NSIS 安装包内已内嵌 WebView2 Bootstrapper，安装时会自动引导安装 WebView2 Runtime。免安装 exe 不含，需目标机预装 WebView2。

### 打包前检查清单
1. `npm run build` 必须无类型错误、无构建报错（vue-tsc + vite）
2. `cargo build --release`（或 `cd src-tauri && cargo build`）后端无编译错误
3. `src-tauri/icons/` 图标齐全（`tauri.conf.json` 的 `bundle.icon` 引用的文件必须存在）
4. `tauri.conf.json` 的 `version`、`identifier`、`productName` 正确
5. `Cargo.toml` 元数据完整（license、repository、authors）
6. 发版前确认 `git status` 干净、已提交推送

### 版本号管理
打包版本来自 `tauri.conf.json` 的 `version` 字段（当前 `0.1.0`）。发新版本时同步更新 `tauri.conf.json` 和 `package.json` 的 `version`，保持一致。

- **常见问题**：
  - 缺少 Rust 环境：需从 https://rustup.rs/ 安装
  - 缺少 Visual Studio 构建工具：需安装 VS Build Tools
  - 首次 `npm run tauri build` 很慢：需编译全部 Rust 依赖（含 Tauri、WebView2 绑定），后续增量编译会快很多
  - 打包报图标错误：检查 `src-tauri/icons/` 下文件是否齐全，必要时用 `npm run tauri icon <源图>` 重新生成全套图标
  - MSI 打包失败：Windows 上 MSI 需要 WiX，Tauri 会自动下载；若网络受限可改用 `--bundles nsis`