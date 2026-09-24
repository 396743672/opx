# OPX — 本地开发环境管理桌面工具

> `README.md`（英文）与 `README_zh.md`（本文件，中文）为官方使用文档；新增或变更功能时，两份须同步更新。

OPX 是一个 Windows 桌面应用，用于管理本地开发环境中的基础设施软件和服务。提供 MySQL、PostgreSQL、MongoDB、Redis、Nginx、MinIO、Kafka、Elasticsearch、InfluxDB、Nacos 等中间件的安装、启动、停止、配置管理与日志查看，Java SpringBoot 应用与 Node.js 应用的全生命周期管理，以及静态网站托管和服务组（Stack）编排功能。

## 功能概览

| 模块 | 功能 | 状态 |
|------|------|------|
| **软件仓库** | 按分类浏览、在线/离线安装、版本选择、镜像切换、批量安装、升级检测与一键升级/回滚 | ✅ |
| **软件管理** | 启动/停止/重启、配置编辑（表单/源码）、健康检查、启动设置、备份/恢复、日志查看（归档续接/搜索/导出）、卸载 | ✅ |
| **应用管理（SpringBoot）** | 应用注册、JVM 参数调优（推荐参数生成）、环境变量（全局/分组/应用三级）、分级别日志（debug/info/warn/error + 归档）、日志检测启动、本机 IP 钉死 | ✅ |
| **Node 应用管理** | JS/TS 服务注册（入口文件+启动参数+环境变量+自启顺序）、Node.js 运行时选择、一键启停、日志尾部查看 | ✅ |
| **服务组（Stack）** | 编排/成员管理、一键启停整组、启动报告、成员端口/日志快捷入口 | ✅ |
| **网站管理** | 站点增删改查、nginx 配置表单/源码编辑、静态文件上传、启用/停用、ACME 证书自动化 | ✅ |
| **证书自动化（ACME）** | Let's Encrypt DNS-01 一键签发、自动续期、多 DNS 服务商、staging 开关 | ✅ |
| **动态域名（DDNS）** | Cloudflare/阿里云/DNSPod(腾讯)/华为云、多源公网 IP 检测、5 分钟调度 + 立即同步 | ✅ |
| **DNS 账号** | 集中管理各服务商凭证、站点绑定账号、旧配置迁移、引用保护 | ✅ |
| **操作记录（审计）** | 可视化页面 + 结果态、CSV 导出、保留 7 天 | ✅ |
| **监控与告警** | 指标历史持久化、阈值告警、webhook/SMTP 通知渠道、监控中心页 | ✅ |
| **崩溃自愈（看门狗）** | 软件/SpringBoot/Node 意外退出延迟 2s 自动拉起（连续 3 次放弃）、主动停止不误拉起 | ✅ |
| **系统监控** | CPU/内存/磁盘/网络使用率、实时趋势图（Chart.js）、系统信息 | ✅ |
| **自定义软件** | 自定义启动命令、配置文件路径、健康检查规则 | ✅ |

## 系统架构

```
┌─────────────────────────────────────────────┐
│                 前端 (Vue 3 + TypeScript)     │
│  Dashboard │ Software │ NodeApps │ Stack     │
│  Websites  │ SpringBoot │ …                  │
├─────────────────────────────────────────────┤
│             Tauri IPC (invoke)              │
├─────────────────────────────────────────────┤
│               后端 (Rust)                     │
│  ├─ commands/    — Tauri 命令层              │
│  ├─ services/   — 业务逻辑层                  │
│  │  ├─ software_manager/  — 软件生命周期      │
│  │  ├─ springboot_manager/ — SpringBoot 管理  │
│  │  ├─ node_app_manager/  — Node 应用管理     │
│  │  ├─ stack_manager/     — 服务组编排        │
│  │  └─ website_manager/   — 网站管理           │
│  ├─ models/     — 数据模型                    │
│  └─ utils/      — 工具函数                    │
├─────────────────────────────────────────────┤
│          本地存储 (JSON 文件)                 │
│  apps.json │ node-apps.json │ installed.json │
│  stack*.json │ settings                        │
└─────────────────────────────────────────────┘
```

## 支持管理的软件

按分类划分（软件仓库页以分类 Tab 区分展示）：

| 分类 | 软件 | 版本管理 | 配置编辑 |
|------|------|---------|---------|
| 数据库 | MySQL、PostgreSQL、MongoDB | ✅ 在线 | ✅ 表单+源码 |
| 缓存 | Redis | ✅ 在线 | ✅ 表单+源码 |
| Web 服务器 | Nginx | ✅ 内置 | ✅ 表单+源码 |
| 对象存储 | MinIO、RustFS | ✅ 在线 | ✅ 表单+源码 |
| 运行时 | JDK、JRE、Node.js | ✅ 在线 | ❌ |
| 注册中心 | Nacos | ✅ 在线 | ✅ 表单+源码 |
| 消息队列 | Kafka（KRaft 模式） | ✅ 在线 | ✅ 表单+源码 |
| 搜索 | Elasticsearch | ✅ 在线 | ✅ 表单+源码 |
| 时序数据库 | InfluxDB、InfluxDB 3 | ✅ 在线 | ✅ 表单+源码 |
| 自定义软件 | 用户上传 | ✅ 用户上传 | ✅ 自定义 |

软件仓库支持批量安装、升级检测与一键升级（替换式：停旧 → 备份 → 装新 → 迁移数据 → 合并记录）、升级后可一键回滚。

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
- 内置软件仓库，支持在线获取版本列表和镜像切换、批量安装
- 升级检测与一键升级（替换式：停旧 → 备份 `.bak` → 装新 → 迁移数据 → 合并记录），升级后可一键回滚
- 备份/恢复：数据快照备份、历史快照恢复、一键重置；升级备份为 zip 压缩包
- 启动/停止/重启，支持首次初始化（如 MySQL 自动执行 --initialize-insecure）
- 配置编辑（表单模式 + 源码模式），保存后自动生效并 reload
- 健康检查（TCP 端口探测 / HTTP 状态码 / 进程存活）
- 日志查看器：实时刷新、自动滚动、关键字/正则/级别过滤、时间范围筛选、历史归档无缝续接（自动跨档跨 gzip 续读）、合并导出归档、下载、全局跨软件日志搜索
- 运行时类软件（JDK/JRE/Node.js）仅提供卸载

### 网站管理
- 基于 nginx 的静态站点和反向代理站点管理
- 表单编辑路由规则，支持静态目录和代理两种模式
- 源码模式直接编辑 .conf 文件
- 站点启用/停用即 reload nginx

### 应用管理（SpringBoot）
- SpringBoot JAR 应用注册和全生命周期管理
- JVM 参数推荐生成（基于 Xmx 自动计算 G1 调优参数）
- 三级环境变量覆盖（全局 → 分组 → 应用）
- UTF-8 字符集勾选、全局/分组环境变量配置
- 日志检测启动状态（每 5s 检测 console.log 的 "Started " 标记，不等固定超时）
- 分级别日志查看：debug/info/warn/error 多来源标签页，支持 `logs/<日期目录>/<类型>.日期.序号.log.gz` 归档历史、关键字/正则/级别/仅错误过滤、实时与自动滚动、时间范围筛选
- **本机 IP 钉死**：表单「本机 IP」字段可钉死广播 IP；留空时后端自动探测真实网卡 IP 并剔除虚拟网卡（VMware / VirtualBox / Hyper-V / Docker / VPN），修复 Nacos / Spring Boot 误取虚拟网卡 IP（如 `192.168.200.1`）的常见问题

### Node 应用管理
- 注册 JS / TS 服务：入口文件 + 启动参数 + 环境变量 + 自启顺序
- 选择已装 Node.js 运行时作为执行引擎（或自动选择）
- 一键启停、运行状态、日志尾部查看
- 应用启动时按自启顺序自动拉起勾选应用
- 运行中的应用禁止编辑/删除，操作带忙态防抖

### 服务组（Stack）
- 编排/成员管理，一键启动/停止整组服务
- 启动报告：最近一次启动展示整组耗时与每个成员的启动耗时/结果（运行/失败及原因），持久化保存
- 成员卡片提供端口一键打开浏览器、日志查看快捷入口

### 系统监控
- 实时 CPU、内存、磁盘、网络使用率
- 60 秒历史趋势图，每秒刷新
- 已安装软件、运行中服务、运行应用概览卡片

### 证书自动化（ACME，DNS-01）
- **一键签发 Let's Encrypt 证书**：站点对话框新增「证书来源」选择（自签 / Let's Encrypt），走 DNS-01 验证，保存站点即完成签发，全程弹窗展示 ACME 阶段进度（10/35/65/90/100%）。
- **自动续期**：每小时检查一次，距到期不足 30 天自动续期；续期按实际结果记审计（reload 失败记为失败，不再无条件记成功）。
- **多 DNS 服务商**（v1 支持 Cloudflare，trait 已预留扩展）：签发与续期按站点绑定的 DNS 账号取凭证。
- **staging 开关**：设置页「域名」Tab 可切换 Let's Encrypt staging 环境（警示态 UI，签发结果明确标注证书不被信任），用于调试避免触发生产速率限制。
- 证书路径统一存相对路径（`sites-data/certs/<域名>.{crt,key}`），nginx 重装后不再失效。

### 动态域名（DDNS）
- **四家服务商**：Cloudflare、阿里云、DNSPod（腾讯云）、华为云，各自的 provider 选择与凭证配置**与网站证书完全解耦**（证书可以是其他家的）。
- **多源公网 IP 检测**：IPv4 / IPv6 分家族专用端点，逐个尝试并汇总全部失败原因。
- **5 分钟调度器** + 设置页「立即同步」按钮；`sync_once` 串行化，避免调度器与手动同步并发创建重复记录。
- 设置页 DDNS 区块：四家凭证切换 + 域名列表 + 手动同步。

### DNS 账号独立管理
- **新增「DNS 账号」页面**：集中管理各家 DNS 服务商凭证（名称 / 服务商 / 凭证 / zone 缓存 / 测试时间），支持「测试连通」真实拉取 zone 列表。
- **站点绑定账号**：站点 SSL 配置改为下拉选择 DNS 账号，不同站点可用不同服务商；站点域名支持从账号缓存的 zone 辅助填充。
- **旧配置自动迁移**：启动时检测旧全局 Cloudflare Token，自动生成「默认 Cloudflare」账号并绑定已勾 ACME 的站点（幂等，重启不重复迁移）。
- **引用保护**：删除被站点引用的账号会被拦截并提示站点名。
- 设置页移除全局 DNS 凭证区块（signature 层零改动，仅 `DnsProvider` trait 抽象重组）。

### 操作记录（审计日志）
- **可视化页面**：侧边栏新增「操作记录」，支持概览（今日 / 总数 / 按操作计数 / 最近时间）、按天 / 操作类型 / 关键字过滤、表格展示、CSV 导出（含 BOM 与转义）。
- **存储**：`oplog!` 宏落点由 tracing 改为独立 JSONL（`logs/audit-YYYY-MM-DD.jsonl`，保留 7 天）。
- **操作结果（二期）**：每条记录带结果态（进行中 / 成功 / 失败）与失败原因，页面新增结果列与失败红标过滤；`install` / `upgrade` / `start` 等后台任务记录进行中与完成结果。覆盖 26 处同步命令 + 服务组拉起的成员启动。
- **分页**：每页 20/50/100/200 可选 + 总数与上下页。
- 启停 / 重启记录补充 detail（版本 + 实例 id）。

### 系统监控与告警
- **指标历史持久化**：新增 30s 常驻采样器（整机 + 运行中软件 / SpringBoot），落盘保留 7 天（此前该文件有读无写、曲线只在内存保留 2 分钟）；保留天数可在设置页配置（1–90 天，默认 7）。
- **趋势曲线改读后端持久化序列**，移除前端历史累积与硬编码阈值。
- **告警规则**：4 个阈值（默认 90，可配）入设置；触发 / 恢复记审计 + 全局 toast 提示；顺带修掉「告警只在监控页打开时才生效」。
- **告警通知渠道**：webhook（支持 json / 钉钉加签 / 企业微信 / 飞书四种格式）+ SMTP 邮件；设置页提供「测试」按钮走真实发送。
- **监控中心页**（`/monitor`）：KPI×3 + 进程资源列表 + 搜索；与首页共用 `useRunningSoftware()` composable。
- **首页瘦身**：只保留整机监控，「运行中服务与应用」改为末尾折叠块。
- **进程资源监控**：静态缓存 `System` 跨调用计算 CPU%；修正 mysqld 等 fork 型服务统计（累加后代进程）；落盘采样用独立实例取 30s 平均 CPU。

### 崩溃自愈（看门狗）
- **软件 / SpringBoot / Node 三实体**进程意外退出后延迟 2s 自动拉起；连续失败 3 次放弃（Error + toast + 审计）。
- 失败计数规则：观察到意外退出即 +1，健康即清零。
- 主动停止不再被误拉起（Node 停止先置 Stopping）；未开自动重启的进程被外部结束时统一「状态转 Error + 提示」，且状态监听提升到 `App.vue` 全局注册，切页不再丢失事件。
- 软件与 Node 的启动设置支持开启自动重启。

### 服务组与编排（增强）
- **统一启动编排**：依赖就绪后再启动依赖方（`wait_dependency_ready`）。
- **软件级依赖拓扑**可视化；**进程端口图谱**（netstat 采集运行中软件的监听端口 + 冲突诊断，卡片嵌入）。

### Nacos 集群模式
- 单节点加入外部集群（填 `cluster_nodes` 多行 `ip:port`，OPX 写 `conf/cluster.conf`，cluster 时强校验 `storage=mysql`）；配置表单新增 `Textarea` 字段类型。
- Nacos 控制台链接按版本区分（3.x 不带 `/nacos`）；数据库字段按 storage 实时显示。

### 设置与界面
- **设置页 Tab 分区**：通用 / 监控与告警 / 域名与 DNS 三个 Tab，选中态记入 URL（`?tab=`，刷新与直达均保持，非法值回退「通用」）。
- 配置面板官方文档外链。
- 站点对话框改为头部固定、仅表单区滚动。

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

本项目用 Tauri v2 打包，`package.json` 已配置 `"tauri": "tauri"` 脚本，`tauri.conf.json` 中 `build.beforeBuildCommand = "npm run build"` 会在打包前自动执行前端构建（vue-tsc 类型检查 + vite 构建），`bundle.targets = ["nsis"]` 在 Windows 上生成 `.exe` 安装包。

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
| `npm run tauri:build:nsis` | 仅 NSIS 安装包 + 免安装 exe | `src-tauri/target/release/bundle/nsis/opx_0.5.0_x64-setup.exe` + `target/release/opx.exe` | 普通用户分发（国内推荐，无需 WiX） |
| `npm run tauri:build:msi` | 仅 MSI 安装包 + 免安装 exe | `src-tauri/target/release/bundle/msi/opx_0.5.0_x64_zh-CN.msi` + `target/release/opx.exe` | 企业部署（组策略友好） |
| `npm run tauri:build:debug` | Debug 安装包 + Debug 免安装 exe | `src-tauri/target/debug/bundle/nsis/` + `target/debug/opx.exe` | 快速验证打包流程 |
| `npm run tauri:dev` | 开发模式运行（不产文件） | — | 开发调试，热重载 |
| `npm run build` | 前端静态资源 | `dist/` | 仅前端构建验证 |
| `npm run tauri:info` | 环境信息（不产文件） | — | 检查工具链 |
| `npm run tauri dev` | 开发模式运行（不产文件） | — | 开发调试，热重载 |
| `cd src-tauri && cargo build --release` | 免安装 exe | `src-tauri/target/release/opx.exe` | 仅要 exe，不要安装包 |
| `npm run build` | 前端静态资源 | `dist/` | 仅前端构建验证 |

### 产物位置总览
打包产物在 `src-tauri/target/release/bundle/` 下：
- `msi/opx_0.5.0_x64_zh-CN.msi` — MSI 安装包（推荐企业分发，组策略部署友好）
- `nsis/opx_0.5.0_x64-setup.exe` — NSIS 安装程序（推荐普通用户分发）
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
打包版本来自 `tauri.conf.json` 的 `version` 字段（当前 `0.5.0`）。发新版本时同步更新 `tauri.conf.json`、`package.json` 与 `Cargo.toml` 的 `version`，保持一致。

- **常见问题**：
  - 缺少 Rust 环境：需从 https://rustup.rs/ 安装
  - 缺少 Visual Studio 构建工具：需安装 VS Build Tools
  - 首次 `npm run tauri build` 很慢：需编译全部 Rust 依赖（含 Tauri、WebView2 绑定），后续增量编译会快很多
  - 打包报图标错误：检查 `src-tauri/icons/` 下文件是否齐全，必要时用 `npm run tauri icon <源图>` 重新生成全套图标
  - MSI 打包失败：Windows 上 MSI 需要 WiX，Tauri 会自动下载；若网络受限可改用 `--bundles nsis`
