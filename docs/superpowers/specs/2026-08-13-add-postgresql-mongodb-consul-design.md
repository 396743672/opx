# 新增 PostgreSQL / MongoDB / Consul 设计规格

## 概述

在 OPX 软件仓库中新增三个常驻服务：PostgreSQL、MongoDB、Consul。复用现有 `SoftwareProvider` trait 与启停机制，无新抽象。

## 范围

- 首批新增三个常驻服务：PostgreSQL、MongoDB、Consul。
- **Node.js 暂缓**：Node.js 属 Runtime 类（同 JDK/JRE），但当前无消费 Node 的模块，装了没有用武之地，待有「Node 应用管理」模块时再加。

## 架构

完全复用现有 `SoftwareProvider` trait。每个软件一个 provider 文件 + `all_providers()` 注册；`build_builtin_catalog()` 会自动聚合进 catalog，无需额外改动。

- 启动：复用 `spawn_process`（前台 spawn + 记录 PID）。
- 停止：复用 `stop_one`（taskkill 优雅 → 强杀）。
- **不扩展 trait**：不为 PostgreSQL 引入 `stop_command`，改用前台 `postgres.exe` 而非 `pg_ctl` 守护模式（见下），保持 PID 管理与通用启停一致。

## Provider 设计

### 1. PostgreSQL — `providers/postgresql.rs`

| 项 | 值 |
|----|----|
| key / name | `postgresql` / `PostgreSQL` |
| category | `Database` |
| icon | `mdi:database` |
| 版本 | 硬编码 EDB 官方 Windows zip（16.x / 15.x） |
| 首次初始化 | `FirstRunInit`：`initdb.exe -D <install>/data -U postgres --auth=trust --encoding=UTF8`（类似 MySQL `--initialize-insecure`） |
| 启动 | `postgres.exe -D <install>/data`（前台主进程；端口由 `postgresql.conf` 的 `port` 控制） |
| 健康检查 | TCP 5432 |
| 配置 | 表单（port / listen_addresses / shared_buffers / max_connections）+ 源码 `postgresql.conf` |

**说明**：官方 `pg_ctl start` 是后台守护进程（`pg_ctl` 自身会退出），会破坏现有 PID 记录与 `stop_one` 优雅杀。改用前台 `postgres.exe` 直接跑主进程，PID 正确、停止逻辑通用。

### 2. MongoDB — `providers/mongodb.rs`

| 项 | 值 |
|----|----|
| key / name | `mongodb` / `MongoDB` |
| category | `Database` |
| icon | `mdi:leaf` |
| 版本 | 硬编码官方 Windows zip（7.x / 6.x） |
| 首次初始化 | 无（`mongod` 自动创建 dbpath） |
| 启动 | `mongod.exe --config mongod.cfg` |
| 健康检查 | TCP 27017 |
| 配置 | 表单（port / bind_ip / dbpath）+ 源码 `mongod.cfg` |

### 3. Consul — `providers/consul.rs`

| 项 | 值 |
|----|----|
| key / name | `consul` / `Consul` |
| category | `Registry`（新增） |
| icon | `mdi:hexagon-multiple` |
| 版本 | 硬编码 HashiCorp 官方 Windows zip（1.x） |
| 首次初始化 | 无 |
| 启动 | 按 `mode` 分支：`dev` → `agent -dev -bind 127.0.0.1`；`server` → `agent -server -bootstrap-expect=1 -data-dir <install>/data -bind 127.0.0.1` |
| 健康检查 | HTTP 8500（`/v1/status/leader`，预期 200） |
| 配置 | 表单（mode / bind / http_port；server 模式加 data_dir），无源码编辑 |

## 跨层改动

1. **`SoftwareCategory` 枚举**（后端 `models/software.rs` + 前端 `models/software.ts`）：新增 `Registry`。前端 `catalog.ts` 的 `groupedEntries` 记录同步加 `Registry` 键。
2. **i18n**（`zh-CN.ts` / `en-US.ts`）：新增
   - `categoryRegistry`（注册中心）
   - `catalogDesc.postgresql` / `catalogDesc.mongodb` / `catalogDesc.consul`
   - 各 `configField.*`（port / listen_addresses / shared_buffers / max_connections / bind_ip / dbpath / mode / bind / http_port / data_dir 等）
   - mirror 名称（`i18n:postgresqlOfficial` / `mongodbOfficial` / `consulOfficial`）
3. **图标**：在 provider `catalog_entry()` 写 `mdi:xxx`，跑 `npm run icons:gen` 自动扫描收录进 `mdi-icons.json`。图标名以 `gen-icons.mjs` 校验为准（不存在的名字会报错 exit 1）。

## 版本策略

第一批全部**硬编码 2-3 个版本号**，URL 指向官方下载源。不实现 `fetch_remote_versions`（官网下载 URL 固定、无干净 API，动态拉取意义不大）。后续要实时版本再补。

## 测试策略

- 后端：每个 provider 的 `config_schema` / `health_check` / `start_command` 单元测试（断言字段与命令参数，不真正 spawn）。
- 前端：`npm run build`（vue-tsc 类型检查 + vite 构建）。
- 手动验证：安装 → 首次初始化（仅 PG）→ 启动 → 健康检查 → 改配置 reload → 停止 → 卸载。

## 验证

- `cargo test`（后端单测）
- `npm run build`（前端类型 + 构建）
- `npm run icons:gen`（图标子集生成，校验图标名存在）
