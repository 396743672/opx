# 单例运行 + 关闭行为 + 语言设置 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 让 OPX 单例运行；点关闭按钮时弹窗让用户选择「收起到托盘」或「退出程序」；退出时级联停止所有已注册子服务并显示进度；补齐设置读写后端；语言默认中文且可在设置切换。

**架构：** 后端用 `tauri-plugin-single-instance` 实现单例并激活已有窗口；`on_window_event` 拦截 `CloseRequested` 转交前端决策；新增 `process_registry` 服务维护子进程注册表并提供 `stop_all`（优雅停止→5s 超时强杀，逐个 emit 进度事件）；新增 `commands/app.rs` 的 `quit_app`/`hide_main_window` 与 `commands/config.rs` 的设置读写。前端 `App.vue` 编排关闭流程，`CloseDialog`/`StopProgressDialog` 两个模态组件，`SettingsPage` 实装外观/语言/关闭行为。

**技术栈：** Tauri v2（Rust）、Vue 3 + TypeScript + Pinia、vue-i18n、Tailwind v4。

**关键约定：**
- 后端命令均注册到 `lib.rs` 的 `invoke_handler`。
- 前端调用命令统一用 `@tauri-apps/api/core` 的 `invoke`，事件用 `@tauri-apps/api/event` 的 `listen`。
- 进度事件名：`stop-progress`（payload `{ current, total, name, status }`）、`stop-complete`（无 payload）、关闭请求事件 `close-requested`（无 payload）。
- `stop_all` 为同步阻塞循环：子进程数有限，且本次骨架注册表预期为空或少量；每个进程 emit 后事件进入 Tauri 事件队列，前端在主循环让出时渲染。可接受。若未来子服务多需改异步，计划末尾有说明。

---

## 文件结构

**后端（Rust）— 创建/修改：**
- `src-tauri/Cargo.toml` — 加 `tauri-plugin-single-instance = "2"` 依赖
- `src-tauri/src/models/settings.rs` — `CloseWindowAction` 枚举调整 + `ask_on_close` 字段
- `src-tauri/src/commands/config.rs` — 实装 `get_settings`/`save_settings`（当前为空）
- `src-tauri/src/services/process_registry.rs` — 新建，进程注册表 + `stop_all`
- `src-tauri/src/services/mod.rs` — 注册 `process_registry` 模块
- `src-tauri/src/commands/app.rs` — 新建，`quit_app`/`hide_main_window`
- `src-tauri/src/commands/mod.rs` — 注册 `app` 模块
- `src-tauri/src/lib.rs` — 单例插件、托盘、关闭事件拦截、注册新命令

**前端（TypeScript/Vue）— 创建/修改：**
- `src/models/settings.ts` — 同步枚举 + `ask_on_close`
- `src/stores/settings.ts` — `setLanguage` + 加载后应用语言
- `src/components/CloseDialog.vue` — 新建，关闭选择模态
- `src/components/StopProgressDialog.vue` — 新建，退出进度模态
- `src/App.vue` — 关闭流程编排
- `src/modules/settings/pages/SettingsPage.vue` — 实装外观/语言/关闭行为
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts` — 补充文案 key

---

## 任务 1：后端 settings 模型调整

**文件：**
- 修改：`src-tauri/src/models/settings.rs`

- [ ] **步骤 1：替换 `CloseWindowAction` 枚举与 `AppSettings`**

将 `src-tauri/src/models/settings.rs` 全文替换为：

```rust
use serde::{Deserialize, Serialize};

/// 关闭窗口时的默认行为
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CloseWindowAction {
    /// 收起到系统托盘，进程存活
    CloseToTray,
    /// 退出程序（级联停止子服务后结束）
    Exit,
}

impl Default for CloseWindowAction {
    fn default() -> Self {
        Self::CloseToTray
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: String,
    pub language: String,
    pub sidebar_collapsed: bool,
    pub software_root: String,
    pub config_root: String,
    pub mirror_url: String,
    pub auto_check_update: bool,
    pub close_window_action: CloseWindowAction,
    /// 关闭窗口时是否每次弹窗询问
    pub ask_on_close: bool,
    pub register_as_system_service: bool,
    pub auto_start_managed_services: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "auto".to_string(),
            language: "zh-CN".to_string(),
            sidebar_collapsed: false,
            software_root: "apps".to_string(),
            config_root: "config".to_string(),
            mirror_url: "https://mirrors.aliyun.com".to_string(),
            auto_check_update: true,
            close_window_action: CloseWindowAction::default(),
            ask_on_close: true,
            register_as_system_service: false,
            auto_start_managed_services: true,
        }
    }
}
```

- [ ] **步骤 2：编译验证**

运行：`cd src-tauri && cargo build 2>&1 | tail -20`
预期：可能因 `commands/config.rs` 等尚未使用新字段而有 warning，但**不应有 error**。若有 error 先修。

- [ ] **步骤 3：Commit**

```bash
git add src-tauri/src/models/settings.rs
git commit -m "refactor(backend): 调整 CloseWindowAction 枚举并新增 ask_on_close 字段"
```

---

## 任务 2：后端设置读写命令实装

**文件：**
- 修改：`src-tauri/src/commands/config.rs`（当前为空）

**说明：** 设置文件路径用 `AppHandle` 的 `Manager::path().app_config_dir()` 获取，文件名 `settings.json`。原子写：先写 `settings.json.tmp` 再 `rename`。

- [ ] **步骤 1：实装 `commands/config.rs`**

将 `src-tauri/src/commands/config.rs` 全文替换为：

```rust
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use crate::models::settings::AppSettings;

/// 设置文件路径：<app_config_dir>/settings.json
fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法获取配置目录: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {}", e))?;
    Ok(dir.join("settings.json"))
}

/// 读取设置；文件缺失或解析失败返回默认值
#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    let path = match settings_path(&app) {
        Ok(p) => p,
        Err(_) => return AppSettings::default(),
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 保存设置（原子写）
#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    let tmp = path.with_extension("json.tmp");
    let content =
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
    Ok(())
}
```

- [ ] **步骤 2：在 `commands/mod.rs` 注册模块（已存在 `pub mod config;`，确认即可）**

确认 `src-tauri/src/commands/mod.rs` 含 `pub mod config;`（当前已有，无需改）。

- [ ] **步骤 3：编译验证**

运行：`cd src-tauri && cargo build 2>&1 | tail -20`
预期：编译通过，可能有「未使用」warning（命令尚未注册到 invoke_handler，下个任务注册）。

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/commands/config.rs
git commit -m "feat(backend): 实装 get_settings/save_settings 命令"
```

---

## 任务 3：后端进程注册表服务

**文件：**
- 创建：`src-tauri/src/services/process_registry.rs`
- 修改：`src-tauri/src/services/mod.rs`

**说明：** 全局 `Lazy<Mutex<ProcessRegistry>>`。`stop_all` 同步循环：对每个注册进程优雅停止（Windows `taskkill /PID` 不带 `/F`；Unix `kill -TERM`），轮询 5s 检查是否退出，仍存活则强杀（`/F` / `kill -9`）。每处理一个 emit `stop-progress`，全部完成 emit `stop-complete`。用 `sysinfo`（已依赖）的 `System` 检查进程是否存活。

- [ ] **步骤 1：创建 `services/process_registry.rs`**

写入 `src-tauri/src/services/process_registry.rs`：

```rust
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use tauri::{AppHandle, Emitter, Manager};

/// 已注册的子进程
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisteredProcess {
    pub id: i64,
    pub pid: u32,
    pub name: String,
    pub kind: String,
    pub started_at: i64,
}

/// 单个进程停止结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopResult {
    pub pid: u32,
    pub name: String,
    pub success: bool,
}

/// 进度事件 payload
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopProgressEvent {
    pub current: usize,
    pub total: usize,
    pub name: String,
    pub status: String, // "stopped" | "killed" | "failed"
}

pub struct ProcessRegistry {
    next_id: i64,
    processes: HashMap<i64, RegisteredProcess>,
}

impl ProcessRegistry {
    fn new() -> Self {
        Self {
            next_id: 1,
            processes: HashMap::new(),
        }
    }

    pub fn register(&mut self, pid: u32, name: String, kind: String) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        let started_at = chrono::Local::now().timestamp();
        self.processes.insert(
            id,
            RegisteredProcess {
                id,
                pid,
                name,
                kind,
                started_at,
            },
        );
        id
    }

    pub fn unregister(&mut self, id: i64) {
        self.processes.remove(&id);
    }

    pub fn list(&self) -> Vec<RegisteredProcess> {
        self.processes.values().cloned().collect()
    }

    pub fn drain(&mut self) -> Vec<RegisteredProcess> {
        let v: Vec<RegisteredProcess> = self.processes.values().cloned().collect();
        self.processes.clear();
        v
    }
}

static REGISTRY: Lazy<Mutex<ProcessRegistry>> =
    Lazy::new(|| Mutex::new(ProcessRegistry::new()));

/// 注册一个子进程，返回 id
pub fn register(pid: u32, name: String, kind: String) -> i64 {
    REGISTRY.lock().unwrap().register(pid, name, kind)
}

/// 注销
pub fn unregister(id: i64) {
    REGISTRY.lock().unwrap().unregister(id);
}

/// 列出所有已注册进程
pub fn list() -> Vec<RegisteredProcess> {
    REGISTRY.lock().unwrap().list()
}

/// 进程是否存活
fn is_alive(pid: u32) -> bool {
    let mut sys = System::new();
    sys.refresh_processes();
    sys.process(Pid::from_u32(pid)).is_some()
}

/// 停止单个进程：优雅停止→等5s→强杀。返回 (是否成功, 状态字符串)
fn stop_one(pid: u32) -> (bool, String) {
    if !is_alive(pid) {
        return (true, "stopped".to_string());
    }
    // 优雅停止
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .output();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();

    // 轮询等待最多 5s
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if !is_alive(pid) {
            return (true, "stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // 超时强杀
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output();

    std::thread::sleep(Duration::from_millis(300));
    if is_alive(pid) {
        (false, "failed".to_string())
    } else {
        (true, "killed".to_string())
    }
}

/// 停止所有已注册子进程，逐个 emit 进度，完成后 emit stop-complete
pub fn stop_all(app: &AppHandle) -> Vec<StopResult> {
    let procs = REGISTRY.lock().unwrap().drain();
    let total = procs.len();
    let mut results = Vec::new();

    for (i, p) in procs.iter().enumerate() {
        let (success, status) = stop_one(p.pid);
        let _ = app.emit(
            "stop-progress",
            StopProgressEvent {
                current: i + 1,
                total,
                name: p.name.clone(),
                status: status.clone(),
            },
        );
        results.push(StopResult {
            pid: p.pid,
            name: p.name.clone(),
            success,
        });
    }

    let _ = app.emit("stop-complete", ());
    results
}
```

- [ ] **步骤 2：在 `services/mod.rs` 注册模块**

将 `src-tauri/src/services/mod.rs` 替换为：

```rust
pub mod process_registry;
pub mod service_registry;
pub mod software_manager;
pub mod springboot_manager;
pub mod system_monitor;
```

- [ ] **步骤 3：确认 `chrono` 依赖可用**

`Cargo.toml` 已含 `chrono = { version = "0.4", features = ["serde"] }`（任务前已确认）。无需改动。若编译报 chrono 缺失，在 `[dependencies]` 加该行。

- [ ] **步骤 4：编译验证**

运行：`cd src-tauri && cargo build 2>&1 | tail -30`
预期：编译通过。若有 error（如 `Emitter`/`Manager` trait 未导入）按报错修。注意 `Emitter` trait 需在作用域内才能调 `app.emit`（已 `use tauri::Emitter`）。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/src/services/process_registry.rs src-tauri/src/services/mod.rs
git commit -m "feat(backend): 新增进程注册表与 stop_all 级联停止骨架"
```

---

## 任务 4：后端 app 命令（quit_app / hide_main_window）

**文件：**
- 创建：`src-tauri/src/commands/app.rs`
- 修改：`src-tauri/src/commands/mod.rs`

- [ ] **步骤 1：创建 `commands/app.rs`**

写入 `src-tauri/src/commands/app.rs`：

```rust
use tauri::{AppHandle, Manager};
use crate::services::process_registry;

/// 退出程序：先级联停止所有已注册子服务，再退出
#[tauri::command]
pub fn quit_app(app: AppHandle) -> Result<(), String> {
    // stop_all 内部 emit 进度事件并最终 emit stop-complete
    let _results = process_registry::stop_all(&app);
    // 无论 stop_all 是否全部成功，都退出
    app.exit(0);
    Ok(())
}

/// 隐藏主窗口（收起到系统托盘）
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .hide()
            .map_err(|e| format!("隐藏窗口失败: {}", e))?;
    }
    Ok(())
}
```

- [ ] **步骤 2：在 `commands/mod.rs` 注册模块**

将 `src-tauri/src/commands/mod.rs` 替换为：

```rust
pub mod app;
pub mod config;
pub mod service;
pub mod software;
pub mod springboot;
pub mod system;
```

- [ ] **步骤 3：编译验证**

运行：`cd src-tauri && cargo build 2>&1 | tail -20`
预期：编译通过。

- [ ] **步骤 4：Commit**

```bash
git add src-tauri/src/commands/app.rs src-tauri/src/commands/mod.rs
git commit -m "feat(backend): 新增 quit_app 与 hide_main_window 命令"
```

---

## 任务 5：后端 lib.rs 整合（单例 + 托盘 + 关闭拦截 + 注册命令）

**文件：**
- 修改：`src-tauri/Cargo.toml`（加依赖）
- 修改：`src-tauri/src/lib.rs`

**说明：** 单例插件必须在 builder 链最前注册。托盘含右键菜单（显示窗口/退出）。`on_window_event` 拦截主窗口 `CloseRequested`，`prevent_close` 后 emit `close-requested` 给前端。

- [ ] **步骤 1：加单例插件依赖**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 段（`tauri-plugin-opener = "2"` 之后）加一行：

```toml
tauri-plugin-single-instance = "2"
```

- [ ] **步骤 2：重写 `lib.rs`**

将 `src-tauri/src/lib.rs` 全文替换为：

```rust
pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // 单例：第二实例启动时激活已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = set_focus_safe(&window);
            }
        }))
        .setup(|app| {
            #[cfg(desktop)]
            {
                // 托盘右键菜单
                let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

                let app_handle = app.handle().clone();
                let _tray = TrayIconBuilder::new()
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = set_focus_safe(&window);
                            }
                        }
                        "quit" => {
                            let _ = app.emit("close-requested", ());
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(move |tray, event| {
                        // 双击托盘显示窗口
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = set_focus_safe(&window);
                            }
                        }
                    })
                    .build(app)?;
                let _ = app_handle; // 保留 handle 引用避免提前释放
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // 拦截主窗口关闭，交由前端决策
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.app_handle().emit("close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::system::process_list,
            commands::system::kill_process,
            commands::system::system_history,
            commands::config::get_settings,
            commands::config::save_settings,
            commands::app::quit_app,
            commands::app::hide_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while starting tauri application");
}

/// 跨平台安全的聚焦窗口（set_focus 在某些平台返回 Result）
fn set_focus_safe(window: &tauri::WebviewWindow) -> Result<(), tauri::Error> {
    window.set_focus()
}
```

- [ ] **步骤 3：确认托盘图标资源存在**

`tauri.conf.json` 的 `bundle.icon` 已列出 `icons/icon.ico` 等。`TrayIconBuilder::new()` 未指定图标时会用默认；若需自定义图标后续再加 `.icon(app.default_window_icon().unwrap().clone())`。本次先用默认，编译能过即可。

- [ ] **步骤 4：编译验证**

运行：`cd src-tauri && cargo build 2>&1 | tail -30`
预期：编译通过。若 `set_focus` 返回 `()` 而非 `Result`（取决于 tauri 版本），把 `set_focus_safe` 改为直接 `window.set_focus();` 并返回 `Ok(())`。按实际报错调整。

- [ ] **步骤 5：Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs
git commit -m "feat(backend): 集成单例插件、系统托盘、关闭事件拦截并注册新命令"
```

---

## 任务 6：前端 settings 模型与 store 扩展

**文件：**
- 修改：`src/models/settings.ts`
- 修改：`src/stores/settings.ts`

- [ ] **步骤 1：更新 `models/settings.ts`**

将 `src/models/settings.ts` 全文替换为：

```typescript
export enum CloseWindowAction {
  CloseToTray = 'CloseToTray',
  Exit = 'Exit',
}

export type ThemeMode = 'auto' | 'light' | 'dark'

export interface AppSettings {
  theme: string
  language: string
  sidebar_collapsed: boolean
  software_root: string
  config_root: string
  mirror_url: string
  auto_check_update: boolean
  close_window_action: CloseWindowAction
  ask_on_close: boolean
  register_as_system_service: boolean
  auto_start_managed_services: boolean
}
```

- [ ] **步骤 2：扩展 `stores/settings.ts`，增加语言应用与 setLanguage**

将 `src/stores/settings.ts` 替换为：

```typescript
import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import type { AppSettings, ThemeMode } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'
import { i18n } from '@/utils/i18n' // 注意：需导出 i18n 实例，见步骤 3

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

  const systemPrefersDark = ref(
    typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches
  )

  if (typeof window !== 'undefined') {
    const mql = window.matchMedia('(prefers-color-scheme: dark)')
    mql.addEventListener('change', (e) => {
      systemPrefersDark.value = e.matches
    })
  }

  const theme = computed<ThemeMode>(
    () => (settings.value?.theme as ThemeMode) ?? 'auto'
  )
  const language = computed(() => settings.value?.language ?? 'zh-CN')

  const isDark = computed(
    () =>
      theme.value === 'dark' ||
      (theme.value === 'auto' && systemPrefersDark.value)
  )

  function applyTheme() {
    document.documentElement.classList.toggle('dark', isDark.value)
  }

  function applyLanguage() {
    const lang = language.value
    if (i18n.global.locale.value !== lang) {
      i18n.global.locale.value = lang
    }
  }

  watch(isDark, applyTheme, { immediate: true })

  async function loadSettings() {
    settings.value = await invoke('get_settings')
    applyTheme()
    applyLanguage()
  }

  async function saveSettings() {
    if (settings.value) {
      await invoke('save_settings', { settings: settings.value })
    }
  }

  async function setTheme(mode: ThemeMode) {
    if (!settings.value) return
    settings.value.theme = mode
    applyTheme()
    await saveSettings()
  }

  async function setLanguage(lang: string) {
    if (!settings.value) return
    settings.value.language = lang
    applyLanguage()
    await saveSettings()
  }

  return {
    settings,
    theme,
    language,
    isDark,
    systemPrefersDark,
    loadSettings,
    saveSettings,
    setTheme,
    setLanguage,
    updateSettings: (newSettings: AppSettings) => {
      settings.value = newSettings
    },
  }
})
```

- [ ] **步骤 3：从 `utils/i18n.ts` 导出 i18n 实例**

当前 `src/utils/i18n.ts` 只有 `loadMessages`。`main.ts` 里 `createI18n` 在 `main.ts` 内创建，store 无法引用。需把 i18n 实例创建移到 `utils/i18n.ts` 并导出。

将 `src/utils/i18n.ts` 替换为：

```typescript
import { createI18n } from 'vue-i18n'
import zhCN from '../locales/zh-CN'
import enUS from '../locales/en-US'

export type Messages = typeof zhCN

export function loadMessages() {
  return {
    'zh-CN': zhCN,
    'en-US': enUS,
  }
}

export const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: loadMessages(),
})
```

- [ ] **步骤 4：更新 `main.ts` 使用导出的 i18n**

将 `src/main.ts` 替换为：

```typescript
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import './styles/main.css'
import { i18n } from './utils/i18n'

const pinia = createPinia()

const app = createApp(App)
app.use(pinia)
app.use(router)
app.use(i18n)
app.mount('#app')
```

- [ ] **步骤 5：构建验证**

运行：`npm run build 2>&1 | tail -15`
预期：vue-tsc 类型检查 + 构建通过。若报 `i18n` 循环导入，确认 `utils/i18n.ts` 不依赖 store。

- [ ] **步骤 6：Commit**

```bash
git add src/models/settings.ts src/stores/settings.ts src/utils/i18n.ts src/main.ts
git commit -m "feat(frontend): 同步 settings 模型，i18n 实例集中导出，新增 setLanguage"
```

---

## 任务 7：i18n 文案补充

**文件：**
- 修改：`src/locales/zh-CN.ts`
- 修改：`src/locales/en-US.ts`

- [ ] **步骤 1：在 `zh-CN.ts` 末尾（`comingSoonDesc` 行后、闭合 `}` 前）追加**

```typescript

  // 关闭与退出
  closeDialogTitle: '关闭窗口',
  closeToTray: '关闭窗口',
  exitProgram: '退出程序',
  exitHint: '将停止所有由本程序管理的服务',
  rememberChoice: '记住选择（不再询问）',
  cancel: '取消',
  trayShow: '显示窗口',
  trayQuit: '退出',
  stoppingServices: '正在停止服务…',
  stopProgress: '停止 {current}/{total}：{name}',
  safelyExited: '已安全退出',

  // 设置页
  closeBehavior: '关闭窗口时',
  askOnClose: '每次询问',
  languageLabel: '语言',
  themeLabel: '主题',
```

注意：`cancel` 可能已存在于 common 区（已有 `cancel: '取消'`）。若已存在则**不要重复定义**——只追加不存在的 key。重复定义会导致 TS 报错。

- [ ] **步骤 2：在 `en-US.ts` 末尾对应追加**

```typescript

  // close & exit
  closeDialogTitle: 'Close Window',
  closeToTray: 'Close to Tray',
  exitProgram: 'Exit Program',
  exitHint: 'All services managed by this app will be stopped',
  rememberChoice: 'Remember choice (do not ask again)',
  cancel: 'Cancel',
  trayShow: 'Show Window',
  trayQuit: 'Quit',
  stoppingServices: 'Stopping services…',
  stopProgress: 'Stopping {current}/{total}: {name}',
  safelyExited: 'Safely exited',

  // settings
  closeBehavior: 'On Window Close',
  askOnClose: 'Ask every time',
  languageLabel: 'Language',
  themeLabel: 'Theme',
```

同样避免与已有 `cancel` 重复。

- [ ] **步骤 3：构建验证**

运行：`npm run build 2>&1 | tail -10`
预期：通过。

- [ ] **步骤 4：Commit**

```bash
git add src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(i18n): 补充关闭弹窗、退出进度、设置页文案"
```

---

## 任务 8：CloseDialog 组件

**文件：**
- 创建：`src/components/CloseDialog.vue`

**说明：** 模态对话框，两个选项按钮 + 退出说明 + 记住选择勾选。emit `choose('tray'|'exit')` 与 `cancel`。`defaultChoice` prop 决定默认高亮项（来自 settings.close_window_action）。

- [ ] **步骤 1：创建 `components/CloseDialog.vue`**

```vue
<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
    @click.self="$emit('cancel')"
  >
    <div class="w-[360px] rounded-lg border border-border bg-card shadow-popover p-5">
      <h2 class="text-base font-semibold mb-4">{{ $t('closeDialogTitle') }}</h2>

      <div class="space-y-2 mb-4">
        <button
          class="w-full flex items-center gap-3 px-3 py-2.5 rounded-md border text-left transition-colors cursor-pointer"
          :class="choice === 'tray'
            ? 'border-primary bg-primary/10 text-foreground'
            : 'border-border hover:bg-muted'"
          @click="choice = 'tray'"
        >
          <Icon icon="mdi:window-close" class="text-xl text-primary" />
          <span class="text-sm font-medium">{{ $t('closeToTray') }}</span>
        </button>

        <button
          class="w-full flex items-center gap-3 px-3 py-2.5 rounded-md border text-left transition-colors cursor-pointer"
          :class="choice === 'exit'
            ? 'border-destructive bg-destructive/10 text-foreground'
            : 'border-border hover:bg-muted'"
          @click="choice = 'exit'"
        >
          <Icon icon="mdi:power" class="text-xl text-destructive" />
          <span class="text-sm font-medium">{{ $t('exitProgram') }}</span>
        </button>
      </div>

      <p
        v-if="choice === 'exit'"
        class="text-xs text-muted-foreground mb-4 flex items-start gap-1.5"
      >
        <Icon icon="mdi:information-outline" class="text-sm mt-0.5 flex-shrink-0" />
        {{ $t('exitHint') }}
      </p>

      <label class="flex items-center gap-2 mb-4 text-xs text-muted-foreground cursor-pointer select-none">
        <input v-model="remember" type="checkbox" class="accent-primary" />
        {{ $t('rememberChoice') }}
      </label>

      <div class="flex justify-end gap-2">
        <button
          class="px-3 py-1.5 text-sm rounded-md hover:bg-muted transition-colors cursor-pointer"
          @click="$emit('cancel')"
        >
          {{ $t('cancel') }}
        </button>
        <button
          class="px-3 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors cursor-pointer"
          @click="$emit('choose', choice, remember)"
        >
          {{ $t('confirm') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'

useI18n()

interface Props {
  defaultChoice?: 'tray' | 'exit'
}
const props = withDefaults(defineProps<Props>(), {
  defaultChoice: 'tray',
})

const choice = ref<'tray' | 'exit'>(props.defaultChoice)
const remember = ref(false)

defineEmits<{
  choose: [choice: 'tray' | 'exit', remember: boolean]
  cancel: []
}>()
</script>
```

- [ ] **步骤 2：构建验证**

运行：`npm run build 2>&1 | tail -10`
预期：通过。

- [ ] **步骤 3：Commit**

```bash
git add src/components/CloseDialog.vue
git commit -m "feat(frontend): 新增 CloseDialog 关闭选择对话框"
```

---

## 任务 9：StopProgressDialog 组件

**文件：**
- 创建：`src/components/StopProgressDialog.vue`

**说明：** 监听 `stop-progress` 与 `stop-complete` 事件，显示进度。不可手动关闭。完成后短暂显示「已安全退出」再由后端 `app.exit` 收尾。

- [ ] **步骤 1：创建 `components/StopProgressDialog.vue`**

```vue
<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm animate-fade-in"
  >
    <div class="w-[360px] rounded-lg border border-border bg-card shadow-popover p-5">
      <div class="flex items-center gap-3 mb-4">
        <Icon
          :icon="done ? 'mdi:check-circle' : 'mdi:progress-clock'"
          class="text-2xl"
          :class="done ? 'text-success' : 'text-primary'"
        />
        <div>
          <h2 class="text-base font-semibold">
            {{ done ? $t('safelyExited') : $t('stoppingServices') }}
          </h2>
          <p v-if="!done && total > 0" class="text-xs text-muted-foreground tnum">
            {{ current }} / {{ total }}
          </p>
        </div>
      </div>

      <div class="h-2 w-full bg-muted rounded-full overflow-hidden mb-3">
        <div
          class="h-full bg-primary rounded-full transition-all duration-300 ease-out"
          :style="{ width: progressPercent + '%' }"
        ></div>
      </div>

      <p v-if="!done" class="text-xs text-muted-foreground truncate">
        {{ currentName }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

useI18n()

interface StopProgressPayload {
  current: number
  total: number
  name: string
  status: string
}

const current = ref(0)
const total = ref(0)
const currentName = ref('')
const done = ref(false)

const progressPercent = computed(() => {
  if (total.value === 0) return done.value ? 100 : 30
  return Math.round((current.value / total.value) * 100)
})

let unlistenProgress: UnlistenFn | null = null
let unlistenComplete: UnlistenFn | null = null

onMounted(async () => {
  unlistenProgress = await listen<StopProgressPayload>('stop-progress', (e) => {
    current.value = e.payload.current
    total.value = e.payload.total
    currentName.value = e.payload.name
  })
  unlistenComplete = await listen('stop-complete', () => {
    done.value = true
    current.value = total.value
  })
})

onUnmounted(() => {
  unlistenProgress?.()
  unlistenComplete?.()
})
</script>
```

- [ ] **步骤 2：构建验证**

运行：`npm run build 2>&1 | tail -10`
预期：通过。

- [ ] **步骤 3：Commit**

```bash
git add src/components/StopProgressDialog.vue
git commit -m "feat(frontend): 新增 StopProgressDialog 退出进度对话框"
```

---

## 任务 10：App.vue 关闭流程编排

**文件：**
- 修改：`src/App.vue`

**说明：** 监听 `close-requested` 事件，按 `ask_on_close` 与 `close_window_action` 决策。tray→`hide_main_window`；exit→显示 StopProgressDialog + `quit_app`。记住选择时更新 settings 并保存。

- [ ] **步骤 1：重写 `src/App.vue`**

```vue
<template>
  <MainLayout />
  <CloseDialog
    v-if="showCloseDialog"
    :default-choice="defaultChoice"
    @choose="onChoose"
    @cancel="showCloseDialog = false"
  />
  <StopProgressDialog v-if="showStopProgress" />
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import MainLayout from '@/layouts/MainLayout.vue'
import CloseDialog from '@/components/CloseDialog.vue'
import StopProgressDialog from '@/components/StopProgressDialog.vue'
import { useSettingsStore } from '@/stores/settings'
import { useAppStore } from '@/stores/app'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { CloseWindowAction } from '@/models/settings'

const settingsStore = useSettingsStore()
const appStore = useAppStore()
void appStore

const showCloseDialog = ref(false)
const showStopProgress = ref(false)

const defaultChoice = computed<'tray' | 'exit'>(() =>
  settingsStore.settings?.close_window_action === CloseWindowAction.Exit
    ? 'exit'
    : 'tray'
)

let unlistenClose: UnlistenFn | null = null

async function executeTray() {
  await invoke('hide_main_window')
}

async function executeExit() {
  showStopProgress.value = true
  await invoke('quit_app')
}

async function onCloseRequested() {
  const s = settingsStore.settings
  if (!s) {
    await executeTray()
    return
  }
  if (s.ask_on_close) {
    showCloseDialog.value = true
  } else {
    if (s.close_window_action === CloseWindowAction.Exit) {
      await executeExit()
    } else {
      await executeTray()
    }
  }
}

async function onChoose(choice: 'tray' | 'exit', remember: boolean) {
  showCloseDialog.value = false
  if (remember && settingsStore.settings) {
    settingsStore.settings.close_window_action =
      choice === 'exit' ? CloseWindowAction.Exit : CloseWindowAction.CloseToTray
    settingsStore.settings.ask_on_close = false
    await settingsStore.saveSettings()
  }
  if (choice === 'exit') {
    await executeExit()
  } else {
    await executeTray()
  }
}

onMounted(async () => {
  await settingsStore.loadSettings()
  unlistenClose = await listen('close-requested', () => {
    onCloseRequested()
  })
})

onUnmounted(() => {
  unlistenClose?.()
})
</script>
```

- [ ] **步骤 2：构建验证**

运行：`npm run build 2>&1 | tail -15`
预期：通过。

- [ ] **步骤 3：Commit**

```bash
git add src/App.vue
git commit -m "feat(frontend): App.vue 编排关闭请求与退出流程"
```

---

## 任务 11：SettingsPage 实装

**文件：**
- 修改：`src/modules/settings/pages/SettingsPage.vue`

**说明：** 替换空壳为外观/语言/关闭行为设置区。语言切换即时生效，其余保存生效。

- [ ] **步骤 1：重写 `SettingsPage.vue`**

```vue
<template>
  <div class="animate-fade-in max-w-2xl">
    <PageHeader
      icon="mdi:cog"
      :title="$t('settings')"
      :subtitle="$t('appearance')"
    />

    <!-- 外观 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader :title="$t('appearance')" hide-refresh />
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('themeLabel') }}</span>
          <select
            v-model="themeValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          >
            <option value="auto">{{ $t('auto') }}</option>
            <option value="light">{{ $t('light') }}</option>
            <option value="dark">{{ $t('dark') }}</option>
          </select>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('languageLabel') }}</span>
          <select
            v-model="languageValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          >
            <option value="zh-CN">中文</option>
            <option value="en-US">English</option>
          </select>
        </div>
      </div>
    </div>

    <!-- 关闭行为 -->
    <div class="rounded-lg border border-border bg-card p-4 shadow-card mb-4">
      <CardHeader :title="$t('closeBehavior')" hide-refresh />
      <div class="space-y-4">
        <div class="flex items-center justify-between">
          <span class="text-sm">{{ $t('closeBehavior') }}</span>
          <select
            v-model="closeActionValue"
            class="h-8 px-2 text-sm rounded-md bg-muted border border-border outline-none focus:border-primary"
          >
            <option value="CloseToTray">{{ $t('closeToTray') }}</option>
            <option value="Exit">{{ $t('exitProgram') }}</option>
          </select>
        </div>
        <label class="flex items-center justify-between cursor-pointer">
          <span class="text-sm">{{ $t('askOnClose') }}</span>
          <input v-model="askOnCloseValue" type="checkbox" class="accent-primary w-4 h-4" />
        </label>
      </div>
    </div>

    <div class="flex justify-end">
      <button
        class="px-4 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors cursor-pointer"
        @click="save"
      >
        {{ $t('save') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { CloseWindowAction, type ThemeMode } from '@/models/settings'
import PageHeader from '@/components/PageHeader.vue'
import CardHeader from '@/components/CardHeader.vue'

const { t } = useI18n()
void t
const settingsStore = useSettingsStore()

const themeValue = ref<ThemeMode>('auto')
const languageValue = ref('zh-CN')
const closeActionValue = ref<string>('CloseToTray')
const askOnCloseValue = ref(true)

// 初始化（settings 加载后）
watch(
  () => settingsStore.settings,
  (s) => {
    if (s) {
      themeValue.value = (s.theme as ThemeMode) || 'auto'
      languageValue.value = s.language || 'zh-CN'
      closeActionValue.value = s.close_window_action
      askOnCloseValue.value = s.ask_on_close
    }
  },
  { immediate: true }
)

// 语言即时生效
watch(languageValue, (lang) => {
  settingsStore.setLanguage(lang)
})

// 主题即时生效
watch(themeValue, (mode) => {
  settingsStore.setTheme(mode)
})

async function save() {
  if (!settingsStore.settings) return
  settingsStore.settings.close_window_action =
    closeActionValue.value === 'Exit'
      ? CloseWindowAction.Exit
      : CloseWindowAction.CloseToTray
  settingsStore.settings.ask_on_close = askOnCloseValue.value
  await settingsStore.saveSettings()
}
</script>
```

- [ ] **步骤 2：构建验证**

运行：`npm run build 2>&1 | tail -15`
预期：通过。

- [ ] **步骤 3：Commit**

```bash
git add src/modules/settings/pages/SettingsPage.vue
git commit -m "feat(frontend): 实装 SettingsPage 外观/语言/关闭行为设置"
```

---

## 任务 12：联调验证

**文件：** 无（验证步骤）

- [ ] **步骤 1：前端构建**

运行：`npm run build 2>&1 | tail -15`
预期：vue-tsc + vite 构建通过，无 error。

- [ ] **步骤 2：后端构建**

运行：`cd src-tauri && cargo build 2>&1 | tail -30`
预期：编译通过，无 error（warning 可接受）。

- [ ] **步骤 3：启动应用实机验证**

运行：`npm run tauri dev`
逐项核对：
- **设置读写**：首次启动（删除 `%APPDATA%/com.tauri-app.opx/settings.json` 测试）用默认值；改语言即时切换中英文；改关闭行为/每次询问保存后重启配置保留。
- **关闭弹窗**：点窗口 ✕ → 弹 CloseDialog，默认勾选=设置的默认行为；选「关闭窗口」→ 窗口隐藏、托盘可见；选「退出程序」→ 显示「将停止…」说明。
- **托盘**：双击托盘恢复窗口；右键「显示窗口」「退出」可用（退出走 close-requested 流程）。
- **退出进度**：选退出 → 弹 StopProgressDialog（注册表为空时 0/0 快速完成）→ 显示「已安全退出」→ 进程结束。
- **记住选择**：勾选「记住选择」选某项后，下次点 ✕ 不再弹窗，直接执行该行为。
- **单例**：应用运行中再启动 `target/debug/opx.exe` → 不出现第二窗口，原窗口激活到前台。

- [ ] **步骤 4：核对 reduced-motion**

系统开启「减少动态效果」后，对话框/进度条动画应降级（main.css 已有全局 `prefers-reduced-motion` 兜底）。

- [ ] **步骤 5：Commit（如有验证中修复）**

若验证中发现并修复了问题：

```bash
git add -A
git commit -m "fix: 联调验证修复"
```

若无修改，跳过。

---

## 范围与后续

- **级联停止为骨架**：`process_registry` 提供 register/unregister/list/stop_all，但本次无调用方注册子进程（软件管理/SpringBoot 管理尚未实装）。退出时注册表为空，StopProgressDialog 显示 0/0 快速完成。后续实装子服务启动时调 `process_registry::register(pid, name, kind)` 即可接入退出级联停止。
- **stop_all 同步阻塞**：每个进程最多等 5s。若未来子服务多（>10），应改为异步（tokio task + 通道），避免长时间阻塞 IPC。本次子进程数有限，可接受。
- **不修改**已有 system 监控命令。
