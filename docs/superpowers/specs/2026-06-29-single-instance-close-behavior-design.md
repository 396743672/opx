# OPX 单例运行 + 关闭行为 + 语言设置 设计规格

日期：2026-06-29
主题：应用单例、窗口关闭行为、退出级联停止子服务、语言设置

## Context

OPX 是 Tauri v2 + Vue 3 桌面运维管理工具，管理 SpringBoot / SpringCloud / MySQL / Redis / Nginx 等子服务。当前存在几处缺口需一并补齐：

1. **无单例控制**：可启动多个实例，无意义且会冲突。
2. **关闭窗口即退出程序**：点 ✕ 直接结束进程，对带托盘的常驻工具不友好；用户无法选择「收起到托盘」还是「真退出」。
3. **退出不停止子服务**：退出主程序时由本程序启动的子进程（SpringBoot/MySQL 等）成为孤儿继续运行。
4. **设置读写未实现**：前端 `settings store` 调用 `invoke('get_settings')`/`invoke('save_settings')`，但后端 `invoke_handler` 未注册这两条命令，`config.rs` 为空文件——设置功能当前完全不可用。
5. **语言默认中文但不可切换**：`main.ts` 硬编码 `zh-CN`，无法在设置里改。

本需求实现单例、关闭弹窗与设置、退出级联停止骨架、语言切换，并补齐设置读写后端。

## 已确认决策

- 关闭语义：**关闭窗口**=收起到系统托盘（进程存活）；**退出程序**=级联停止所有已注册子服务后结束进程。
- 弹窗协作：设置「默认关闭行为」+「每次询问」开关。默认关闭窗口、默认每次询问。点 ✕ 时若开询问则弹窗（默认勾选=默认行为），关询问则直接执行默认行为。
- 退出说明：弹窗「退出程序」选项下显示「将停止所有由本程序管理的服务」。
- 退出进度：退出时弹出模态进度对话框，后端事件流式推送每个子服务停止进度，完成/超时后退出。
- 单例：用 `tauri-plugin-single-instance`，第二实例拒绝启动并激活已有窗口。
- 子进程停止机制：本次搭骨架（进程注册表 + `stop_all` + `quit_app`），软件管理/SpringBoot 管理后续启动子进程时往注册表注册。

## 架构

### 后端（Rust）

**新增依赖**：`tauri-plugin-single-instance = "2"`（Cargo.toml）。

**`models/settings.rs` 扩展**：
- `CloseWindowAction` 枚举重命名/调整为：`CloseToTray`（关闭窗口/收起托盘）、`Exit`（退出程序）。删除 `MinimizeToTray`/`BackgroundService`（后者语义并入未来服务管理，关闭行为不再需要）。`Default = CloseToTray`。
- `AppSettings` 新增字段：`ask_on_close: bool`（默认 true）。
- 其余字段保留；`language` 默认值已是 `zh-CN`，保留。

**新增 `commands/config.rs`（实装，当前为空）**：
- `get_settings() -> AppSettings`：从 `config_root` 下 `settings.json` 读取；文件不存在或解析失败返回 `AppSettings::default()`。
- `save_settings(settings: AppSettings) -> Result<(), String>`：写回 `settings.json`（原子写：写临时文件再 rename）。
- 设置文件路径：`app_config_dir/settings.json`（用 `app.path().app_config_dir()`，通过 `State` 注入路径或命令内获取）。

**新增 `services/process_registry.rs`**：
- 全局 `Lazy<Mutex<ProcessRegistry>>`，`ProcessRegistry` 内 `HashMap<i64, RegisteredProcess>`。
- `RegisteredProcess { id: i64, pid: u32, name: String, kind: String, started_at: i64 }`。
- `register(pid, name, kind) -> id`、`unregister(id)`、`list() -> Vec<RegisteredProcess>`。
- `stop_all(app: AppHandle) -> Vec<StopResult>`：遍历注册表，逐个停止：
  - 优雅停止：Windows `taskkill /PID <pid>`（不带 `/F`，触发 WM_CLOSE）；Unix `kill -TERM <pid>`。
  - 等待最多 5s，仍存活则 `taskkill /PID <pid> /F` / `kill -9`。
  - 每停一个就 `app.emit("stop-progress", StopProgressEvent { current, total, name, status })`。
  - 清空注册表，返回结果列表。

**新增 `commands/app.rs`**：
- `quit_app(app: AppHandle) -> Result<(), String>`：调用 `stop_all(app)`，完成后 `app.exit(0)`。注意 stop_all 是同步阻塞循环（子进程数有限，可接受），事件在循环中 emit。
- `hide_main_window(app: AppHandle) -> Result<(), String>`：隐藏主窗口（收起托盘）。

**`lib.rs` 改造**：
- `.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| { 激活主窗口：unminimize+show+set_focus }))` —— 必须在 builder 链最前。
- `.setup` 内构建托盘：`TrayIconBuilder` + 右键菜单（显示窗口 / 退出），托盘双击显示窗口。
- `.on_window_event` 拦截主窗口 `WindowEvent::CloseRequested`：`api.prevent_close()` + `app.emit("close-requested", ())`，把关闭决策交给前端。
- `invoke_handler` 注册：`get_settings`、`save_settings`、`quit_app`、`hide_main_window`（原有 system 命令保留）。

### 前端（Vue 3）

**`stores/settings.ts` 扩展**：
- `setLanguage(lang)`：写 settings + `i18n.global.locale.value = lang` + saveSettings。
- `loadSettings` 后应用语言：`i18n.global.locale.value = settings.language`。

**`main.ts`**：保持 `zh-CN` 初始默认；`loadSettings` 后由 store 覆盖。

**新增 `components/CloseDialog.vue`**：自定义模态对话框（不用浏览器 confirm）。
- 两个选项卡式按钮：「关闭窗口」（默认勾选=默认行为）/「退出程序」。
- 「退出程序」下小字：「将停止所有由本程序管理的服务」。
- 「记住选择」勾选框：勾选后保存为默认行为并关闭「每次询问」。
- emit `choose('tray'|'exit')`、`remember: boolean`。

**新增 `components/StopProgressDialog.vue`**：退出进度模态。
- 监听 `listen('stop-progress', ...)`，显示「正在停止服务…」+ 进度 `current/total` + 当前服务名 + 进度条。
- 全部完成（监听 `stop-complete`）后自动关闭并提示「已安全退出」。
- 不可手动关闭（防止中断）。

**`App.vue` 关闭流程编排**：
- `onMounted`：`listen('close-requested', ...)` → 读 settings：
  - `ask_on_close` 为真 → 打开 `CloseDialog`。用户选 tray→`invoke('hide_main_window')`；选 exit→打开 `StopProgressDialog` + `invoke('quit_app')`。若 remember→更新 settings（默认行为+ask_on_close=false）并 saveSettings。
  - `ask_on_close` 为假 → 按默认行为直接执行。

**`SettingsPage.vue` 实装外观/行为区**：
- 主题（auto/light/dark）、语言（中文/英文）下拉——改语言即时生效。
- 关闭行为（关闭窗口/退出程序）、「每次询问」开关。
- 保存按钮 → saveSettings。

**`models/settings.ts` 同步**：`CloseWindowAction` 改 `{ CloseToTray: 'CloseToTray', Exit: 'Exit' }`，新增 `ask_on_close: boolean`。

### 数据流

```
点 ✕ → 后端 on_window_event(CloseRequested)
  → prevent_close + emit "close-requested"
  → 前端 App.vue 监听
  → ask_on_close? 
     是 → CloseDialog → 选「关闭窗口」: invoke hide_main_window
                     → 选「退出程序」: 打开 StopProgressDialog + invoke quit_app
     否 → 默认行为: CloseToTray→hide_main_window / Exit→quit_app

quit_app(后端) → stop_all:
  for each 注册子进程: 优雅停止→等5s→强杀; emit "stop-progress"
  → emit "stop-complete"
  → app.exit(0)
前端 StopProgressDialog 收到 stop-complete → 关闭对话框

第二实例启动 → single-instance 回调 → 激活已有主窗口
```

## 关键文件

后端：
- `src-tauri/Cargo.toml`（加 single-instance 依赖）
- `src-tauri/src/lib.rs`（单例插件、托盘、关闭事件拦截、注册命令）
- `src-tauri/src/models/settings.rs`（枚举调整、ask_on_close）
- `src-tauri/src/commands/config.rs`（实装 get/save_settings）
- `src-tauri/src/commands/app.rs`（新增 quit_app、hide_main_window）
- `src-tauri/src/services/process_registry.rs`（新增进程注册表 + stop_all）
- `src-tauri/src/commands/mod.rs`、`src-tauri/src/services/mod.rs`（注册新模块）

前端：
- `src/models/settings.ts`（同步枚举、ask_on_close）
- `src/stores/settings.ts`（setLanguage、语言应用）
- `src/components/CloseDialog.vue`（新增）
- `src/components/StopProgressDialog.vue`（新增）
- `src/App.vue`（关闭流程编排）
- `src/modules/settings/pages/SettingsPage.vue`（实装）
- `src/locales/{zh-CN,en-US}.ts`（补充关闭/进度相关文案 key）

## 错误处理

- `get_settings` 文件缺失/解析失败 → 返回默认值（语言 zh-CN、CloseToTray、ask_on_close true），不报错。
- `save_settings` 写入失败 → 返回 Err 字符串，前端提示。
- `stop_all` 单个进程停止失败不中断整体；超时强杀；结果含失败项记录日志。
- `quit_app` 即使 stop_all 部分失败，最终仍 `app.exit(0)`（保证能退出）。
- single-instance 回调里激活窗口失败忽略（不影响主流程）。

## 验证

1. `npm install` + `npm run build`（前端类型检查 + 构建，无错误）。
2. `cargo build`（后端编译，无错误）。
3. `npm run tauri dev` 实机验证：
   - **单例**：运行中再启动 exe，不出现第二窗口，原窗口激活到前台。
   - **关闭弹窗**：点 ✕ 弹出 CloseDialog，「关闭窗口」收托盘（窗口隐藏、托盘可见），「退出程序」显示说明文字。
   - **托盘**：双击托盘恢复窗口；右键「显示窗口」「退出」可用。
   - **退出进度**：选退出后弹出 StopProgressDialog 显示进度，完成后退出（本次注册表为空时显示 0/0 快速完成）。
   - **设置**：改语言即时切换中英文；改关闭行为/每次询问后保存，下次点 ✕ 按新设置行为。
   - **设置读写**：首次启动无 settings.json 时用默认值，修改后重启应用配置保留。
   - **级联停止骨架**：可临时往注册表注册一个测试子进程验证 stop_all 流程。
4. 检查 `prefers-reduced-motion` 下对话框动画降级。

## 范围说明

- 本次只搭「退出级联停止」**机制骨架**（process_registry + stop_all + quit_app + 进度事件），不实现软件管理/SpringBoot 管理的子进程启动逻辑——那属于后续功能。退出时注册表为空则进度对话框快速完成。
- 不修改 Rust 后端已有的 system 监控命令。
