# OPX 路径便携化设计规格

日期：2026-06-30
主题：配置/安装/运行/临时目录改为相对 exe 所在目录的便携布局

## Context

OPX 当前把 `settings.json` 存在 `%APPDATA%\com.tauri-app.opx\settings.json`（外部用户目录），暴露在外部易被人为修改导致异常。`system_history.json` 已用 `current_exe().parent/data/`（便携）。`AppSettings` 的 `software_root`/`config_root` 字段值是 "apps"/"config" 但未相对 exe 解析。

**目标**：所有运行时路径（配置、安装、运行、临时）统一相对 exe 所在目录，整个安装目录拷贝即用，符合便携软件惯例。OPX 自身配置不暴露在外部。

## 已确认决策

- **基准目录**：exe 文件所在目录（`current_exe().parent`），失败回退 `std::env::current_dir`
- **目录结构**：exe 同级 5 项——`settings.json` + `apps/` + `config/` + `data/` + `tmp/`
- **迁移**：不迁移外部旧数据，便携版首次启动用默认值生成新文件
- `software_root`/`config_root` 字段保留（值 "apps"/"config"），后续软件管理功能用 paths 层解析为绝对路径

## 架构

### 新增 `src-tauri/src/utils/paths.rs`

集中便携路径解析，所有函数返回 `PathBuf`，目录不存在时自动 `create_dir_all`：

- `app_root() -> PathBuf` — exe 所在目录；`current_exe().parent` 失败回退 `std::env::current_dir`，再失败回退 `PathBuf::from(".")`
- `settings_path() -> PathBuf` — `app_root().join("settings.json")`
- `apps_dir() -> PathBuf` — `app_root().join("apps")`，自动创建
- `config_dir() -> PathBuf` — `app_root().join("config")`，自动创建
- `data_dir() -> PathBuf` — `app_root().join("data")`，自动创建
- `tmp_dir() -> PathBuf` — `app_root().join("tmp")`，自动创建
- `resolve_under(root: &Path, name: &str) -> PathBuf` — 通用辅助：name 空或绝对路径时回退 `root.join(name)`，否则 `root.join(name)`

### `src-tauri/src/utils/mod.rs`
注册 `pub mod paths;`

### `src-tauri/src/commands/config.rs` 改造
`settings_path` 私有函数改用 `crate::utils::paths::settings_path()`。`get_settings`/`save_settings` 不再用 `app_config_dir()`，改为接收 `AppHandle` 仅用于错误上下文（或完全移除 AppHandle 参数依赖，直接调 paths）。

实际改动：`settings_path` 函数删除，`get_settings`/`save_settings` 直接调 `paths::settings_path()`。命令签名保持 `(app: AppHandle)` 不变（Tauri 命令约定），但内部不再用 `app.path()`。

### `src-tauri/src/commands/system.rs` 改造
`system_history` 改用 `crate::utils::paths::data_dir().join("system_history.json")`，替代当前 `current_exe().parent/data/system_history.json` 内联逻辑。

## 数据流

```
后端任何需要路径的代码
  → utils::paths::{settings_path|apps_dir|config_dir|data_dir|tmp_dir}
  → app_root() = current_exe().parent（失败回退 current_dir）
  → 返回绝对 PathBuf，目录自动 create_dir_all
```

## 关键文件

- 新增：`src-tauri/src/utils/paths.rs`
- 修改：`src-tauri/src/utils/mod.rs`（注册 paths）
- 修改：`src-tauri/src/commands/config.rs`（用 paths::settings_path）
- 修改：`src-tauri/src/commands/system.rs`（system_history 用 paths::data_dir）

## 错误处理

- `current_exe` 失败回退 `current_dir`，再失败回退 `PathBuf::from(".")`（相对当前工作目录）
- 目录 `create_dir_all` 失败时：`get_settings` 用默认值兜底，`save_settings` 返回 Err 字符串
- 不影响现有 system 监控命令

## 验证

1. `cd src-tauri && cargo build` 编译通过。
2. `npm run build` 前端构建通过（本任务无前端改动，确认无回归）。
3. `npm run tauri dev` 实机验证：
   - 启动后 exe 同级生成 `settings.json`（首次）+ `apps/`/`config/`/`data/`/`tmp/` 目录
   - 修改设置保存后，`settings.json` 在 exe 同级更新（不在 `%APPDATA%`）
   - `%APPDATA%\com.tauri-app.opx\` 不再有新写入（旧文件残留但不影响）
   - `data/system_history.json` 正常读写
4. 便携性验证：把整个 exe 目录拷到其他路径，配置与数据跟随，可正常运行。

## 范围说明

- 本次只做 paths 层 + config/system 命令迁移。
- `apps/`/`config/`/`tmp/` 的实际业务使用（软件安装、配置编辑、下载解压）等后续软件管理功能实装时接入，本次只确保目录自动创建。
- 不修改前端代码。
