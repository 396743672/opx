# 路径便携化实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** OPX 所有配置/安装/运行/临时/日志目录改为相对 exe 所在目录的便携布局，OPX 自身配置不再暴露在外部 APPDATA。

**架构：** 新增 `utils/paths.rs` 集中便携路径解析，基准为 `current_exe().parent`（失败回退 `current_dir`）；`config.rs` 的 settings 读写与 `system.rs` 的历史数据改用 paths 模块；目录自动 `create_dir_all`。

**技术栈：** Rust、Tauri v2、sysinfo、std::fs / std::path。

**关键约定：**
- 基准目录 `app_root()` = `current_exe().parent`，失败回退 `std::env::current_dir`，再失败回退 `PathBuf::from(".")`
- 所有目录函数（apps/config/data/tmp/logs）返回绝对 `PathBuf`，自动 `create_dir_all`
- 命令签名保持 `(app: AppHandle)` 不变（Tauri 约定），但内部不再用 `app.path().app_config_dir()`

---

## 文件结构

- 新增：`src-tauri/src/utils/paths.rs` — 便携路径解析（app_root/settings_path/apps_dir/config_dir/data_dir/tmp_dir/logs_dir）
- 修改：`src-tauri/src/utils/mod.rs` — 注册 `pub mod paths;`
- 修改：`src-tauri/src/commands/config.rs` — settings 读写改用 `paths::settings_path()`
- 修改：`src-tauri/src/commands/system.rs` — system_history 改用 `paths::data_dir()`

---

## 任务 1：新增 utils/paths.rs 便携路径模块

**文件：**
- 创建：`src-tauri/src/utils/paths.rs`
- 修改：`src-tauri/src/utils/mod.rs`

- [ ] **步骤 1：创建 `utils/paths.rs`**

写入 `D:\object\opx\src-tauri\src\utils\paths.rs`：

```rust
use std::fs;
use std::path::{Path, PathBuf};

/// 应用根目录 = exe 所在目录。
/// current_exe 失败回退 current_dir，再失败回退 ".".
pub fn app_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd;
    }
    PathBuf::from(".")
}

/// settings.json 路径（exe 同级）。父目录即 app_root，确保存在。
pub fn settings_path() -> PathBuf {
    let root = app_root();
    let _ = fs::create_dir_all(&root);
    root.join("settings.json")
}

/// 通用：在 root 下解析子目录名，自动创建。
/// name 为空时使用 dir_name 兜底。
fn ensure_dir(root: &Path, name: &str, fallback: &str) -> PathBuf {
    let n = if name.trim().is_empty() { fallback } else { name };
    let p = root.join(n);
    let _ = fs::create_dir_all(&p);
    p
}

/// 用户安装的软件目录：<app_root>/apps
pub fn apps_dir() -> PathBuf {
    ensure_dir(&app_root(), "apps", "apps")
}

/// 用户软件配置目录：<app_root>/config
pub fn config_dir() -> PathBuf {
    ensure_dir(&app_root(), "config", "config")
}

/// OPX 运行数据目录：<app_root>/data
pub fn data_dir() -> PathBuf {
    ensure_dir(&app_root(), "data", "data")
}

/// 临时目录：<app_root>/tmp
pub fn tmp_dir() -> PathBuf {
    ensure_dir(&app_root(), "tmp", "tmp")
}

/// 日志目录：<app_root>/logs
pub fn logs_dir() -> PathBuf {
    ensure_dir(&app_root(), "logs", "logs")
}

/// 通用辅助：在指定 root 下解析 name。
/// name 空或为绝对路径时回退 root.join(name)。
pub fn resolve_under(root: &Path, name: &str) -> PathBuf {
    if name.is_empty() {
        root.to_path_buf()
    } else {
        root.join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_root_is_directory() {
        let root = app_root();
        assert!(root.is_dir() || root.to_string_lossy() == ".", "app_root 应为目录");
    }

    #[test]
    fn settings_path_ends_with_settings_json() {
        let p = settings_path();
        assert!(p.ends_with("settings.json"));
        assert!(p.parent().is_some());
    }

    #[test]
    fn dir_funcs_return_app_root_subdir() {
        let root = app_root();
        assert_eq!(apps_dir(), root.join("apps"));
        assert_eq!(config_dir(), root.join("config"));
        assert_eq!(data_dir(), root.join("data"));
        assert_eq!(tmp_dir(), root.join("logs").parent().unwrap().join("tmp"));
        assert_eq!(logs_dir(), root.join("logs"));
    }

    #[test]
    fn dir_funcs_create_directory() {
        // 调用后目录应存在
        let _ = apps_dir();
        let _ = config_dir();
        let _ = data_dir();
        let _ = tmp_dir();
        let _ = logs_dir();
        assert!(apps_dir().is_dir());
        assert!(config_dir().is_dir());
        assert!(data_dir().is_dir());
        assert!(tmp_dir().is_dir());
        assert!(logs_dir().is_dir());
    }

    #[test]
    fn resolve_under_handles_empty_and_normal() {
        let root = app_root();
        assert_eq!(resolve_under(&root, ""), root);
        assert_eq!(resolve_under(&root, "foo"), root.join("foo"));
    }
}
```

- [ ] **步骤 2：在 `utils/mod.rs` 注册 paths 模块**

将 `D:\object\opx\src-tauri\src\utils\mod.rs` 替换为（保留已有模块，新增 paths）：

```rust
pub mod paths;
```

注意：当前 `utils/mod.rs` 内容需先 Read 确认，可能已有 `archive`/`download`/`file` 等子模块。保留全部已有 `pub mod xxx;`，在最上方加 `pub mod paths;`。

- [ ] **步骤 3：编译并跑单元测试**

运行：`cd /d/object/opx/src-tauri && cargo test paths:: --lib 2>&1 | tail -25`
预期：4-5 个 paths 测试全部 PASS。若有 error 按报错修。

- [ ] **步骤 4：Commit**

```bash
cd /d/object/opx && git add src-tauri/src/utils/paths.rs src-tauri/src/utils/mod.rs && git commit -m "feat(backend): 新增 utils/paths 便携路径模块"
```

---

## 任务 2：config.rs 改用 paths::settings_path

**文件：**
- 修改：`src-tauri/src/commands/config.rs`

- [ ] **步骤 1：重写 `config.rs`**

将 `D:\object\opx\src-tauri\src\commands\config.rs` 全文替换为：

```rust
use std::fs;
use tauri::AppHandle;
use crate::models::settings::AppSettings;
use crate::utils::paths;

/// 读取设置；文件缺失或解析失败返回默认值。
/// 路径相对 exe 所在目录（便携布局），不再使用外部 APPDATA。
#[tauri::command]
pub fn get_settings(_app: AppHandle) -> AppSettings {
    let path = paths::settings_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 保存设置（原子写：写 .tmp 再 rename）
#[tauri::command]
pub fn save_settings(_app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = paths::settings_path();
    let tmp = path.with_extension("json.tmp");
    let content =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
    Ok(())
}
```

注意：
- `app` 参数前缀 `_` 表示未使用（保留签名兼容 Tauri 命令注册）。
- 移除了 `Manager` import 和私有 `settings_path` 函数。
- `PathBuf` import 移除（不再直接用）。

- [ ] **步骤 2：编译验证**

运行：`cd /d/object/opx/src-tauri && cargo build 2>&1 | tail -15`
预期：编译通过，无 error。若有 unused import 警告（如 `tauri::Manager` 残留），按警告清理。

- [ ] **步骤 3：Commit**

```bash
cd /d/object/opx && git add src-tauri/src/commands/config.rs && git commit -m "refactor(backend): settings 读写改用便携路径，不再写外部 APPDATA"
```

---

## 任务 3：system.rs 改用 paths::data_dir

**文件：**
- 修改：`src-tauri/src/commands/system.rs`

- [ ] **步骤 1：修改 `system_history` 命令**

在 `D:\object\opx\src-tauri\src\commands\system.rs` 中，将 `system_history` 函数替换为：

```rust
#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    let history_path = crate::utils::paths::data_dir().join("system_history.json");
    let history = system_monitor::history::load_history(&history_path)
        .map_err(|e| e.to_string())?;
    Ok(history)
}
```

同时在该文件顶部 import 区确认有 `use crate::services::system_monitor;`（已有），并在文件顶部添加 `use crate::utils::paths;`（若用 `crate::utils::paths::data_dir()` 全路径则不需要，但为整洁建议 import）。

实际操作：把原 `system_history` 函数体里 `current_exe` + `parent` + `join("data")` 的内联逻辑替换为 `paths::data_dir().join("system_history.json")`。在文件顶部 `use` 区加 `use crate::utils::paths;`。

- [ ] **步骤 2：编译验证**

运行：`cd /d/object/opx/src-tauri && cargo build 2>&1 | tail -15`
预期：编译通过。若有 `paths` 未使用警告，确认 import 正确。

- [ ] **步骤 3：Commit**

```bash
cd /d/object/opx && git add src-tauri/src/commands/system.rs && git commit -m "refactor(backend): system_history 改用便携 data_dir"
```

---

## 任务 4：联调验证

**文件：** 无（验证步骤）

- [ ] **步骤 1：后端构建 + 测试**

运行：`cd /d/object/opx/src-tauri && cargo build 2>&1 | tail -10 && cargo test paths:: --lib 2>&1 | tail -10`
预期：编译无 error，paths 单测全 PASS。

- [ ] **步骤 2：前端构建**

运行：`cd /d/object/opx && npm run build 2>&1 | tail -6`
预期：vue-tsc + vite 构建通过（本任务无前端改动，确认无回归）。

- [ ] **步骤 3：实机验证便携布局**

运行：`npm run tauri dev`
逐项核对：
- 启动后 exe 同级（dev 模式下是 `src-tauri/target/debug/`）生成 `settings.json`（首次）+ `apps/`/`config/`/`data/`/`tmp/`/`logs/` 目录
- 修改设置保存后，`settings.json` 在 exe 同级更新（不在 `%APPDATA%\com.tauri-app.opx\`）
- `%APPDATA%\com.tauri-app.opx\` 不再有新写入（旧文件残留但不影响）
- `data/system_history.json` 正常读写
- 关闭重启应用，设置保留

- [ ] **步骤 4：便携性验证**

把整个 `target/debug/` 目录（含 opx.exe + settings.json + apps/config/data/tmp/logs）拷到其他路径运行，配置与数据跟随，可正常工作。

- [ ] **步骤 5：Commit（如有验证中修复）**

若验证中发现并修复了问题：
```bash
git add -A && git commit -m "fix: 便携路径联调修复"
```
无修改则跳过。

---

## 范围与后续

- 本次只做 paths 层 + config/system 命令迁移 + 单测。
- `apps/`/`config/`/`tmp/`/`logs/` 的实际业务使用（软件安装、配置编辑、下载解压、日志写入）等后续软件管理/日志功能实装时接入，本次只确保目录自动创建与单测验证。
- 不修改前端代码。
- 不迁移外部旧 `%APPDATA%` 数据（首次启动用默认值）。
