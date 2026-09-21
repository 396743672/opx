# OPX 跨平台运维管理工具 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 创建一个轻量级跨平台运维管理桌面应用，提供系统监控、软件管理（MySQL/JDK/Redis/Nginx）、SpringBoot 应用完整生命周期管理，支持 OPX 自身注册为系统服务并按配置顺序自动启动所有服务。

**架构：** 采用 Tauri 2 + Rust + Vue 3 架构，模块化单应用设计。后端 Rust 提供系统操作能力，前端 Vue 提供现代化交互界面。前后端通过 Tauri IPC 通信，配置以 JSON 文件形式本地存储，软件安装在相对路径便于迁移。

**技术栈：** Tauri 2, Rust, Vue 3, TypeScript, Tailwind CSS v4, shadcn-vue, vue-i18n, Chart.js

---

## 文件结构概览

```
opx/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── mod.rs          # 导出所有命令
│   │   │   ├── system.rs       # 系统监控命令
│   │   │   ├── software.rs     # 软件管理命令
│   │   │   ├── springboot.rs   # SpringBoot 管理命令
│   │   │   ├── config.rs       # 配置管理命令
│   │   │   └── service.rs     # 系统服务注册命令
│   │   ├── services/
│   │   │   ├── mod.rs
│   │   │   ├── system_monitor/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── info.rs     # 系统信息获取
│   │   │   │   └── history.rs  # 历史数据存储
│   │   │   ├── software_manager/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── types.rs
│   │   │   │   ├── installer.rs
│   │   │   │   ├── manager.rs
│   │   │   │   └── providers/
│   │   │   │       ├── mod.rs
│   │   │   │       ├── mysql.rs
│   │   │   │       ├── jdk.rs
│   │   │   │       ├── redis.rs
│   │   │   │       └── nginx.rs
│   │   │   ├── springboot_manager/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── types.rs
│   │   │   │   ├── manager.rs
│   │   │   │   ├── process.rs
│   │   │   │   └── starter.rs   # 顺序启动逻辑
│   │   │   └── service_registry/
│   │   │       ├── mod.rs
│   │   │       ├── windows.rs
│   │   │       ├── linux.rs
│   │   │       └── macos.rs
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   ├── system.rs
│   │   │   ├── software.rs
│   │   │   ├── springboot.rs
│   │   │   └── settings.rs
│   │   └── utils/
│   │       ├── mod.rs
│   │       ├── file.rs
│   │       ├── download.rs
│   │       └── archive.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── main.ts
│   ├── App.vue
│   ├── components/
│   │   ├── ProgressBar.vue
│   │   ├── StatusBadge.vue
│   │   └── CardHeader.vue
│   ├── modules/
│   │   ├── system-monitor/
│   │   │   ├── components/
│   │   │   │   ├── CpuCard.vue
│   │   │   │   ├── MemoryCard.vue
│   │   │   │   ├── DiskCard.vue
│   │   │   │   ├── NetworkCard.vue
│   │   │   │   └── ProcessTable.vue
│   │   │   ├── pages/
│   │   │   │   └── DashboardPage.vue
│   │   │   └── composables/
│   │   │       └── use-system-monitor.ts
│   │   ├── software-manager/
│   │   │   ├── components/
│   │   │   │   ├── SoftwareList.vue
│   │   │   │   ├── SoftwareCard.vue
│   │   │   │   ├── InstallDialog.vue
│   │   │   │   └── ConfigEditor.vue
│   │   │   ├── pages/
│   │   │   │   ├── SoftwareListPage.vue
│   │   │   │   └── RepositoryPage.vue
│   │   │   └── composables/
│   │   │       └── use-software-manager.ts
│   │   └── springboot-manager/
│   │       ├── components/
│   │       │   ├── AppList.vue
│   │       │   ├── AppCard.vue
│   │       │   ├── EditDialog.vue
│   │       │   ├── GroupConfig.vue
│   │       │   ├── JvmMonitor.vue
│   │       │   └── LogViewer.vue
│   │       ├── pages/
│   │       │   └── SpringBootPage.vue
│   │       └── composables/
│   │           └── use-springboot-manager.ts
│   ├── layouts/
│   │   ├── MainLayout.vue
│   │   └── Sidebar.vue
│   ├── router/
│   │   └── index.ts
│   ├── stores/
│   │   ├── app.ts
│   │   └── settings.ts
│   ├── utils/
│   │   ├── i18n.ts
│   │   ├── tauri.ts
│   │   └── format.ts
│   ├── locales/
│   │   ├── zh-CN.ts
│   │   └── en-US.ts
│   ├── assets/
│   ├── styles/
│   │   └── main.css
│   └── vite-env.d.ts
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.ts
└── README.md
```

---

## 阶段 1：项目初始化

### 任务 1.1：初始化 Tauri 2 项目

**文件：**
- 创建：`package.json`
- 创建：`tsconfig.json`
- 创建：`vite.config.ts`
- 创建：`index.html`
- 创建：`.gitignore`
- 创建：`src-tauri/Cargo.toml`
- 创建：`src-tauri/tauri.conf.json`

- [ ] **步骤 1：创建 package.json**

```json
{
  "name": "opx",
  "version": "0.1.0",
  "description": "Lightweight cross-platform operations management tool",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "vue": "^3.4.0",
    "vue-router": "^4.3.0",
    "pinia": "^2.1.0",
    "vue-i18n": "^9.9.0",
    "chart.js": "^4.4.0",
    "shadcn-vue": "^0.9.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@tauri-apps/api": "^2.0.0",
    "@vitejs/plugin-vue": "^5.0.0",
    "autoprefixer": "^10.4.0",
    "tailwindcss": "^4.0.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0",
    "vue-tsc": "^1.8.0"
  }
}
```

- [ ] **步骤 2：创建 tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "preserve",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  },
  "include": ["src/**/*.ts", "src/**/*.d.ts", "src/**/*.tsx", "src/**/*.vue"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

- [ ] **步骤 3：创建 vite.config.ts**

```typescript
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import path from 'path'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src')
    }
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true
  },
  envPrefix: ['VITE_', 'TAURI_']
})
```

- [ ] **步骤 4：创建 index.html**

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/vite.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>OPX - 运维管理工具</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [ ] **步骤 5：创建 src-tauri/Cargo.toml**

```toml
[package]
name = "opx"
version = "0.1.0"
description = "Lightweight cross-platform operations management tool"
authors = ["opx-dev"]
license = ""
repository = ""
edition = "2021"

[build-dependencies]
tauri-build = { version = "2.0", features = [] }

[dependencies]
tauri = { version = "2.0", features = [ "tray-icon", "shell-open"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sys-info = "0.14"
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.11", features = ["blocking", "json"] }
zip = "0.6"
tar = "0.4"
flate2 = "1.0"
walkdir = "2.0"
chrono = "0.4"
anyhow = "1.0"
thiserror = "1.0"
path-slash = "0.1"
windows-service = { version = "0.6", optional = true }

[target.'cfg(windows)'.dependencies]
windows-service = "0.6"
winapi = { version = "0.3", features = ["winbase", "winnt", "handleapi"] }

[features]
default = []
service = ["windows-service"]

[workspace]
```

- [ ] **步骤 6：创建 src-tauri/tauri.conf.json**

```json
{
  "productName": "OPX",
  "version": "0.1.0",
  "identifier": "dev.opx.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "OPX",
        "width": 1200,
        "height": 800,
        "minWidth": 800,
        "minHeight": 600
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **步骤 7：运行依赖安装验证项目结构**

```bash
npm install
```

预期：安装成功，无错误。

- [ ] **步骤 8：Git commit**

```bash
git add .
git commit -m "chore: initialize tauri project"
```

---

### 任务 1.2：创建 Rust 后端基础结构

**文件：**
- 创建：`src-tauri/src/main.rs`
- 创建：`src-tauri/src/lib.rs`
- 创建：`src-tauri/src/commands/mod.rs`
- 创建：`src-tauri/src/services/mod.rs`
- 创建：`src-tauri/src/models/mod.rs`
- 创建：`src-tauri/src/utils/mod.rs`

- [ ] **步骤 1：创建 main.rs**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            {
                let tray = app.tray();
                // 系统托盘菜单会在后续初始化
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            opx::commands::system::*,
            opx::commands::software::*,
            opx::commands::springboot::*,
            opx::commands::config::*,
            opx::commands::service::*,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **步骤 2：创建 lib.rs**

```rust
pub mod commands;
pub mod services;
pub mod models;
pub mod utils;
```

- [ ] **步骤 3：创建 commands/mod.rs**

```rust
pub mod system;
pub mod software;
pub mod springboot;
pub mod config;
pub mod service;
```

- [ ] **步骤 4：创建 services/mod.rs**

```rust
pub mod system_monitor;
pub mod software_manager;
pub mod springboot_manager;
pub mod service_registry;
```

- [ ] **步骤 5：创建 models/mod.rs**

```rust
pub mod system;
pub mod software;
pub mod springboot;
pub mod settings;
```

- [ ] **步骤 6：创建 utils/mod.rs**

```rust
pub mod file;
pub mod download;
pub mod archive;
```

- [ ] **步骤 7：编译验证**

```bash
cd src-tauri && cargo check
```

预期：无编译错误。

- [ ] **步骤 8：Git commit**

```bash
git add src-tauri/src
git commit -m "feat: add rust backend base structure"
```

---

### 任务 1.3：创建数据模型

**文件：**
- 创建：`src-tauri/src/models/system.rs`
- 创建：`src-tauri/src/models/software.rs`
- 创建：`src-tauri/src/models/springboot.rs`
- 创建：`src-tauri/src/models/settings.rs`

- [ ] **步骤 1：创建 system.rs - 系统信息模型**

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_usage: f64,
    pub disks: Vec<DiskInfo>,
    pub network: NetworkInfo,
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub boot_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub packets_sent: u64,
    pub packets_recv: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub timestamp: u64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}
```

- [ ] **步骤 2：创建 software.rs - 软件模型**

```rust
use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareMeta {
    pub key: String,
    pub name: String,
    pub description: String,
    pub available_versions: Vec<String>,
    pub default_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftware {
    pub id: String,
    pub key: String,
    pub name: String,
    pub version: String,
    pub install_path: String,
    pub install_time: NaiveDateTime,
    pub status: SoftwareStatus,
    pub port: u16,
    pub config: serde_json::Value,
    pub is_custom: bool,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SoftwareStatus {
    Running,
    Stopped,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallParams {
    pub key: String,
    pub version: String,
    pub install_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSoftwareList {
    pub software: Vec<InstalledSoftware>,
}

impl Default for InstalledSoftwareList {
    fn default() -> Self {
        Self { software: Vec::new() }
    }
}
```

- [ ] **步骤 3：创建 springboot.rs - SpringBoot 模型**

```rust
use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringApp {
    pub id: String,
    pub name: String,
    pub jar_path: String,
    pub version: String,
    pub env: String,
    pub port: u16,
    pub jvm_opts: String,
    pub args: String,
    pub status: AppStatus,
    pub log_path: String,
    pub start_time: Option<NaiveDateTime>,
    pub backup_enabled: bool,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub auto_start_on_app_start: bool,
    pub startup_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmInfo {
    pub heap_used: u64,
    pub heap_max: u64,
    pub non_heap_used: u64,
    pub thread_count: usize,
    pub gc_count: u64,
    pub gc_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringAppList {
    pub applications: Vec<SpringApp>,
    pub groups: Vec<AppGroup>,
}

impl Default for SpringAppList {
    fn default() -> Self {
        Self {
            applications: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl Default for AppStatus {
    fn default() -> Self {
        AppStatus::Stopped
    }
}
```

- [ ] **步骤 4：创建 settings.rs - 全局设置模型**

```rust
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CloseWindowAction {
    MinimizeToTray,
    Exit,
    BackgroundService,
}

impl Default for CloseWindowAction {
    fn default() -> Self {
        CloseWindowAction::MinimizeToTray
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
            register_as_system_service: false,
            auto_start_managed_services: true,
        }
    }
}
```

- [ ] **步骤 5：编译验证**

```bash
cd src-tauri && cargo check
```

预期：无编译错误。

- [ ] **步骤 6：Git commit**

```bash
git add src-tauri/src/models
git commit -m "feat: add data models"
```

---

### 任务 1.4：创建前端基础结构

**文件：**
- 创建：`src/main.ts`
- 创建：`src/App.vue`
- 创建：`src/styles/main.css`
- 创建：`tailwind.config.ts`
- 创建：`src/router/index.ts`
- 创建：`src/stores/app.ts`
- 创建：`src/stores/settings.ts`
- 创建：`src/vite-env.d.ts`

- [ ] **步骤 1：创建 tailwind.config.ts**

```typescript
import type { Config } from 'tailwindcss'

export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
} satisfies Config
```

- [ ] **步骤 2：创建 main.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  --background: oklch(0.9816 0.0017 247.8390);
  --foreground: oklch(0.1398 0.0384 269.0969);
  --card: oklch(1.0000 0 0);
  --border: oklch(0.8524 0.0178 251.0803);
}

.dark {
  --background: oklch(0.1398 0.0384 269.0969);
  --foreground: oklch(0.9816 0.0017 247.8390);
  --card: oklch(0.1906 0.0335 266.3079);
  --border: oklch(0.2741 0.0247 260.0310);
}

body {
  @apply bg-background text-foreground;
}
```

- [ ] **步骤 3：创建 vite-env.d.ts**

```typescript
/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}
```

- [ ] **步骤 4：创建 router/index.ts**

```typescript
import { createRouter, createWebHashHistory } from 'vue-router'
import SystemMonitorDashboard from '@/modules/system-monitor/pages/DashboardPage.vue'
import SoftwareListPage from '@/modules/software-manager/pages/SoftwareListPage.vue'
import RepositoryPage from '@/modules/software-manager/pages/RepositoryPage.vue'
import SpringBootPage from '@/modules/springboot-manager/pages/SpringBootPage.vue'
import SettingsPage from '@/modules/settings/pages/SettingsPage.vue'

const routes = [
  {
    path: '/',
    redirect: '/dashboard'
  },
  {
    path: '/dashboard',
    name: 'dashboard',
    component: SystemMonitorDashboard,
    meta: { title: 'systemMonitor' }
  },
  {
    path: '/software',
    name: 'software',
    component: SoftwareListPage,
    meta: { title: 'softwareManagement' }
  },
  {
    path: '/repository',
    name: 'repository',
    component: RepositoryPage,
    meta: { title: 'softwareRepository' }
  },
  {
    path: '/springboot',
    name: 'springboot',
    component: SpringBootPage,
    meta: { title: 'springBoot' }
  },
  {
    path: '/settings',
    name: 'settings',
    component: SettingsPage,
    meta: { title: 'settings' }
  }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

export default router
```

- [ ] **步骤 5：创建 stores/app.ts**

```typescript
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export const useAppStore = defineStore('app', () => {
  const sidebarCollapsed = ref(false)
  const currentTitle = ref('OPX')

  const toggleSidebar = () => {
    sidebarCollapsed.value = !sidebarCollapsed.value
  }

  return {
    sidebarCollapsed,
    currentTitle,
    toggleSidebar,
    setCurrentTitle: (title: string) => currentTitle.value = title,
  }
})
```

- [ ] **步骤 6：创建 stores/settings.ts**

```typescript
import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { AppSettings } from '@/models/settings'
import { invoke } from '@tauri-apps/api/core'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<AppSettings | null>(null)

  const theme = computed(() => settings.value?.theme ?? 'auto')
  const language = computed(() => settings.value?.language ?? 'zh-CN')

  async function loadSettings() {
    settings.value = await invoke('get_settings')
  }

  async function saveSettings() {
    if (settings.value) {
      await invoke('save_settings', { settings: settings.value })
    }
  }

  return {
    settings,
    theme,
    language,
    loadSettings,
    saveSettings,
    updateSettings: (newSettings: AppSettings) => {
      settings.value = newSettings
    }
  }
})
```

- [ ] **步骤 7：创建 main.ts**

```typescript
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import { createI18n } from 'vue-i18n'
import App from './App.vue'
import router from './router'
import './styles/main.css'
import { loadMessages } from './utils/i18n'

const pinia = createPinia()
const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: loadMessages()
})

const app = createApp(App)
app.use(pinia)
app.use(router)
app.use(i18n)
app.mount('#app')
```

- [ ] **步骤 8：创建 App.vue**

```vue
<template>
  <MainLayout />
</template>

<script setup lang="ts">
import MainLayout from '@/layouts/MainLayout.vue'
import { onMounted } from 'vue'
import { useSettingsStore } from '@/stores/settings'

const settingsStore = useSettingsStore()

onMounted(async () => {
  await settingsStore.loadSettings()
})
</script>
```

- [ ] **步骤 9：类型检查**

```bash
npx vue-tsc --noEmit
```

预期：无类型错误。

- [ ] **步骤 10：Git commit**

```bash
git add src
git commit -m "feat: add frontend base structure"
```

---

### 任务 1.5：添加国际化支持

**文件：**
- 创建：`src/utils/i18n.ts`
- 创建：`src/locales/zh-CN.ts`
- 创建：`src/locales/en-US.ts`

- [ ] **步骤 1：创建 zh-CN.ts - 中文语言文件**

```typescript
export default {
  // 菜单
  systemMonitor: '系统监控',
  softwareManagement: '软件管理',
  softwareRepository: '软件仓库',
  springBoot: 'SpringBoot',
  settings: '系统设置',

  // 通用
  search: '搜索',
  refresh: '刷新',
  add: '添加',
  edit: '编辑',
  delete: '删除',
  start: '启动',
  stop: '停止',
  restart: '重启',
  save: '保存',
  cancel: '取消',
  confirm: '确认',
  close: '关闭',
  install: '安装',
  uninstall: '卸载',
  status: '状态',
  name: '名称',
  version: '版本',
  port: '端口',
  action: '操作',
  description: '描述',

  // 状态
  running: '运行中',
  stopped: '已停止',
  error: '错误',
  unknown: '未知',
  starting: '启动中',
  stopping: '停止中',

  // 系统监控
  cpuUsage: 'CPU 使用率',
  memoryUsage: '内存使用率',
  diskUsage: '磁盘使用率',
  networkTraffic: '网络流量',
  processList: '进程列表',
  systemInfo: '系统信息',
  os: '操作系统',
  hostname: '主机名',
  bootTime: '启动时间',
  killProcess: '结束进程',
  confirmKillProcess: '确认要结束这个进程吗？',

  // 软件管理
  installedSoftware: '已安装软件',
  installNewSoftware: '安装新软件',
  selectVersion: '选择版本',
  installPath: '安装路径',
  autoStart: '自动启动',
  startupOrder: '启动顺序',
  editConfig: '编辑配置',
  viewLogs: '查看日志',
  confirmUninstall: '确认要卸载这个软件吗？这会删除安装目录下的所有文件。',
  uploadCustom: '上传自定义安装包',
  dragFileHere: '拖拽压缩包到这里，或点击选择',
  supportedFormats: '支持 zip、tar.gz 格式',

  // SpringBoot
  applicationList: '应用列表',
  addApplication: '添加应用',
  editApplication: '编辑应用',
  jarPath: 'Jar 文件路径',
  jvmOptions: 'JVM 参数',
  environment: '环境',
  group: '分组',
  jvmMonitor: 'JVM 监控',
  heapMemory: '堆内存',
  nonHeapMemory: '非堆内存',
  threadCount: '线程数',
  viewLogs: '查看日志',
  uploadJar: '上传更新 Jar',
  rollback: '回滚',
  confirmDelete: '确认要删除这个应用吗？',
  groupConfig: '分组配置',
  addGroup: '添加分组',
  editGroup: '编辑分组',
  deleteGroup: '删除分组',
  startAll: '批量启动',
  stopAll: '全部停止',
  startingInOrder: '按顺序启动中...',

  // 设置
  appearance: '外观',
  theme: '主题',
  language: '语言',
  light: '浅色',
  dark: '深色',
  auto: '跟随系统',
  systemIntegration: '系统集成',
  registerAsService: '注册为系统服务',
  autoStartServices: '启动时自动启动管理的服务',
  closeWindowAction: '关闭窗口行为',
  minimizeToTray: '最小化到系统托盘',
  exitDirectly: '直接退出程序',
  backgroundService: '后台服务模式',
  about: '关于',
  version: '版本',
  checkUpdate: '检查更新',

  // 状态栏
  cpu: 'CPU',
  memory: '内存',
  softwareDir: '软件目录',
}
```

- [ ] **步骤 2：创建 en-US.ts - 英文语言文件**

```typescript
export default {
  // menu
  systemMonitor: 'System Monitor',
  softwareManagement: 'Software Management',
  softwareRepository: 'Software Repository',
  springBoot: 'SpringBoot',
  settings: 'Settings',

  // common
  search: 'Search',
  refresh: 'Refresh',
  add: 'Add',
  edit: 'Edit',
  delete: 'Delete',
  start: 'Start',
  stop: 'Stop',
  restart: 'Restart',
  save: 'Save',
  cancel: 'Cancel',
  confirm: 'Confirm',
  close: 'Close',
  install: 'Install',
  uninstall: 'Uninstall',
  status: 'Status',
  name: 'Name',
  version: 'Version',
  port: 'Port',
  action: 'Action',
  description: 'Description',

  // status
  running: 'Running',
  stopped: 'Stopped',
  error: 'Error',
  unknown: 'Unknown',
  starting: 'Starting',
  stopping: 'Stopping',

  // system monitor
  cpuUsage: 'CPU Usage',
  memoryUsage: 'Memory Usage',
  diskUsage: 'Disk Usage',
  networkTraffic: 'Network Traffic',
  processList: 'Process List',
  systemInfo: 'System Info',
  os: 'OS',
  hostname: 'Hostname',
  bootTime: 'Boot Time',
  killProcess: 'Kill Process',
  confirmKillProcess: 'Confirm to kill this process?',

  // software management
  installedSoftware: 'Installed Software',
  installNewSoftware: 'Install New Software',
  selectVersion: 'Select Version',
  installPath: 'Install Path',
  autoStart: 'Auto Start',
  startupOrder: 'Startup Order',
  editConfig: 'Edit Config',
  viewLogs: 'View Logs',
  confirmUninstall: 'Confirm to uninstall this software? This will delete all files in the install directory.',
  uploadCustom: 'Upload Custom Package',
  dragFileHere: 'Drag file here, or click to select',
  supportedFormats: 'Supported formats: zip, tar.gz',

  // SpringBoot
  applicationList: 'Applications',
  addApplication: 'Add Application',
  editApplication: 'Edit Application',
  jarPath: 'Jar Path',
  jvmOptions: 'JVM Options',
  environment: 'Environment',
  group: 'Group',
  jvmMonitor: 'JVM Monitor',
  heapMemory: 'Heap Memory',
  nonHeapMemory: 'Non-Heap Memory',
  threadCount: 'Thread Count',
  viewLogs: 'View Logs',
  uploadJar: 'Upload Jar Update',
  rollback: 'Rollback',
  confirmDelete: 'Confirm to delete this application?',
  groupConfig: 'Group Configuration',
  addGroup: 'Add Group',
  editGroup: 'Edit Group',
  deleteGroup: 'Delete Group',
  startAll: 'Start All',
  stopAll: 'Stop All',
  startingInOrder: 'Starting in order...',

  // settings
  appearance: 'Appearance',
  theme: 'Theme',
  language: 'Language',
  light: 'Light',
  dark: 'Dark',
  auto: 'Auto',
  systemIntegration: 'System Integration',
  registerAsService: 'Register as System Service',
  autoStartServices: 'Auto-start managed services on boot',
  closeWindowAction: 'Close window action',
  minimizeToTray: 'Minimize to system tray',
  exitDirectly: 'Exit directly',
  backgroundService: 'Background service mode',
  about: 'About',
  version: 'Version',
  checkUpdate: 'Check Update',

  // status bar
  cpu: 'CPU',
  memory: 'Memory',
  softwareDir: 'Software Dir',
}
```

- [ ] **步骤 3：创建 i18n.ts**

```typescript
import zhCN from '../locales/zh-CN'
import enUS from '../locales/en-US'

export type Messages = typeof zhCN

export function loadMessages() {
  return {
    'zh-CN': zhCN,
    'en-US': enUS,
  }
}
```

- [ ] **步骤 4：Git commit**

```bash
git add src/locales src/utils/i18n.ts
git commit -m "feat: add internationalization i18n support"
```

---

### 任务 1.6：创建布局组件

**文件：**
- 创建：`src/layouts/MainLayout.vue`
- 创建：`src/layouts/Sidebar.vue`

- [ ] **步骤 1：创建 Sidebar.vue**

```vue
<template>
  <div
    class="h-full border-r border-border transition-all duration-300"
    :class="sidebarCollapsed ? 'w-16' : 'w-64'"
  >
    <div class="p-4 border-b border-border flex items-center justify-between">
      <h1 v-show="!sidebarCollapsed" class="text-xl font-bold">OPX</h1>
      <button @click="toggleSidebar" class="p-2 rounded-md hover:bg-accent">
        <span class="iconify" data-icon="mdi:menu"></span>
      </button>
    </div>
    <nav class="p-2">
      <RouterLink
        v-for="item in menuItems"
        :key="item.name"
        :to="item.path"
        class="flex items-center gap-3 px-3 py-2 rounded-md mb-1 hover:bg-accent transition-colors"
        :class="{ 'bg-primary/10 text-primary': $route.path === item.path }"
      >
        <span class="iconify" :data-icon="item.icon"></span>
        <span v-show="!sidebarCollapsed" class="font-medium">
          {{ $t(item.meta.title) }}
        </span>
      </RouterLink>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { useAppStore } from '@/stores/app'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const appStore = useAppStore()
const route = useRoute()
const { sidebarCollapsed, toggleSidebar } = appStore

const menuItems = [
  {
    path: '/dashboard',
    name: 'dashboard',
    icon: 'mdi:gauge',
    meta: { title: 'systemMonitor' }
  },
  {
    path: '/software',
    name: 'software',
    icon: 'mdi:package-variant',
    meta: { title: 'softwareManagement' }
  },
  {
    path: '/repository',
    name: 'repository',
    icon: 'mdi:store',
    meta: { title: 'softwareRepository' }
  },
  {
    path: '/springboot',
    name: 'springboot',
    icon: 'mdi:spring-boot',
    meta: { title: 'springBoot' }
  },
  {
    path: '/settings',
    name: 'settings',
    icon: 'mdi:cog',
    meta: { title: 'settings' }
  },
]
</script>

<style scoped>
</style>
```

- [ ] **步骤 2：创建 MainLayout.vue**

```vue
<template>
  <div class="h-screen flex flex-col overflow-hidden">
    <!-- 顶部导航栏 -->
    <header class="border-b border-border h-14 flex items-center px-4 justify-between">
      <div class="flex items-center gap-4">
        <h2 class="text-lg font-semibold">{{ currentTitle }}</h2>
      </div>
      <div class="flex items-center gap-2">
        <button
          @click="toggleTheme"
          class="p-2 rounded-md hover:bg-accent"
          title="Toggle theme"
        >
          <span class="iconify" :data-icon="isDark ? 'mdi:weather-sunny' : 'mdi:weather-night'"></span>
        </button>
      </div>
    </header>

    <!-- 主内容区 -->
    <div class="flex-1 flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 overflow-auto p-4">
        <RouterView />
      </main>
    </div>

    <!-- 底部状态栏 -->
    <footer v-show="systemInfo" class="border-t border-border h-10 flex items-center px-4 text-sm text-muted-foreground gap-6">
      <div>
        {{ t('cpu') }}: {{ systemInfo.cpuUsage.toFixed(1) }}%
      </div>
      <div>
        {{ t('memory') }}: {{ formatBytes(systemInfo.memoryUsed) }} / {{ formatBytes(systemInfo.memoryTotal) }} ({{ systemInfo.memoryUsage.toFixed(1) }}%)
      </div>
      <div>
        {{ t('softwareDir') }}: {{ softwareRoot }}
      </div>
    </footer>
  </div>
</template>

<script setup lang="ts">
import Sidebar from './Sidebar.vue'
import { RouterView } from 'vue-router'
import { useAppStore } from '@/stores/app'
import { useSettingsStore } from '@/stores/settings'
import { useI18n } from 'vue-i18n'
import { onMounted, onUnmounted, ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SystemInfo } from '@/models/system'

const { t } = useI18n()
const appStore = useAppStore()
const settingsStore = useSettingsStore()
const currentTitle = computed(() => {
  const title = appStore.currentTitle
  return title || 'OPX'
})

const systemInfo = ref<SystemInfo | null>(null)
const softwareRoot = computed(() => settingsStore.settings?.software_root ?? 'apps')

let refreshInterval: number | null = null

const isDark = ref(document.documentElement.classList.contains('dark'))

function toggleTheme() {
  document.documentElement.classList.toggle('dark')
  isDark.value = !isDark.value
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

async function refreshSystemInfo() {
  systemInfo.value = await invoke('system_info')
}

onMounted(() => {
  refreshSystemInfo()
  refreshInterval = window.setInterval(refreshSystemInfo, 2000)
})

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval)
  }
})
</script>

<style scoped>
</style>
```

- [ ] **步骤 3：Git commit**

```bash
git add src/layouts
git commit -m "feat: add layout components"
```

---

## 阶段 2：系统监控模块

### 任务 2.1：后端 - 系统监控服务

**文件：**
- 创建：`src-tauri/src/utils/file.rs`
- 创建：`src-tauri/src/utils/download.rs`
- 创建：`src-tauri/src/utils/archive.rs`
- 创建：`src-tauri/src/services/system_monitor/mod.rs`
- 创建：`src-tauri/src/services/system_monitor/info.rs`
- 创建：`src-tauri/src/services/system_monitor/history.rs`
- 创建：`src-tauri/src/commands/system.rs`

- [ ] **步骤 1：创建 file.rs - 文件工具**

```rust
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let content = fs::read_to_string(path)?;
    let value = serde_json::from_str(&content)?;
    Ok(value)
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, content)?;
    Ok(())
}
```

- [ ] **步骤 2：创建 utils 其他文件（download.rs, archive.rs） - 占位实现**

download.rs:
```rust
use anyhow::Result;
use reqwest::blocking;

pub fn download(url: &str, destination: &std::path::Path) -> Result<()> {
    let response = blocking::get(url)?;
    let content = response.bytes()?;
    std::fs::write(destination, content)?;
    Ok(())
}
```

archive.rs:
```rust
use anyhow::Result;
use std::path::Path;

pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(dest_dir)?;
    Ok(())
}

pub fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let bufreader = std::io::BufReader::new(file);
    let gzr = flate2::read::GzDecoder::new(bufreader);
    let mut ar = tar::Archive::new(gzr);
    ar.unpack(dest_dir)?;
    Ok(())
}
```

- [ ] **步骤 3：创建 system_monitor/info.rs**

```rust
use sysinfo::{System, SystemExt, ProcessorExt, DiskExt, NetworkExt};
use crate::models::system::{SystemInfo, DiskInfo, NetworkInfo, ProcessInfo};

pub fn get_system_info(system: &mut System) -> SystemInfo {
    system.refresh_all();

    let cpu_usage = system.global_processor_info().cpu_usage();

    let memory_used = system.used_memory();
    let memory_total = system.total_memory();
    let memory_usage = if memory_total > 0 {
        (memory_used as f64 / memory_total as f64) * 100.0
    } else {
        0.0
    };

    let mut disks = Vec::new();
    for disk in system.disks() {
        let total = disk.total_size();
        let available = disk.available_space();
        let used = total - available;
        let usage = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        disks.push(DiskInfo {
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total,
            used,
            usage,
        });
    }

    let mut bytes_sent = 0;
    let mut bytes_recv = 0;
    let mut packets_sent = 0;
    let mut packets_recv = 0;
    for (_, network) in system.networks() {
        bytes_sent += network.transmitted();
        bytes_recv += network.received();
        packets_sent += network.transmitted_packets();
        packets_recv += network.received_packets();
    }

    let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
    let boot_time = System::boot_time();

    SystemInfo {
        cpu_usage,
        memory_used,
        memory_total,
        memory_usage,
        disks,
        network: NetworkInfo {
            bytes_sent,
            bytes_recv,
            packets_sent,
            packets_recv,
        },
        os_name,
        os_version,
        hostname,
        boot_time,
    }
}

pub fn get_process_list(system: &mut System) -> Vec<ProcessInfo> {
    system.refresh_processes();

    let mut processes = Vec::new();
    for (pid, process) in system.processes() {
        processes.push(ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string(),
            cpu_usage: process.cpu_usage(),
            memory_usage: process.memory_usage(),
            status: process.status().to_string(),
        });
    }

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes
}

pub fn kill_process(pid: u32) -> bool {
    use sysinfo::ProcessExt;
    let mut system = System::new();
    system.refresh_processes();
    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) {
        process.kill()
    } else {
        false
    }
}
```

- [ ] **步骤 4：创建 system_monitor/history.rs**

```rust
use crate::models::system::HistoryPoint;
use anyhow::Result;
use std::path::Path;

const MAX_HISTORY_POINTS: usize = 1440; // 24 hours at 1-minute intervals

pub fn load_history(path: &Path) -> Result<Vec<HistoryPoint>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let history = crate::utils::file::read_json::<Vec<HistoryPoint>>(path)?;
    Ok(history)
}

pub fn save_history(path: &Path, history: &[HistoryPoint]) -> Result<()> {
    let mut history = history.to_vec();
    if history.len() > MAX_HISTORY_POINTS {
        history = history.split_off(history.len() - MAX_HISTORY_POINTS);
    }
    crate::utils::file::write_json(path, &history)?;
    Ok(())
}

pub fn add_history_point(history: &mut Vec<HistoryPoint>, cpu: f64, memory: f64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    history.push(HistoryPoint {
        timestamp: now,
        cpu_usage: cpu,
        memory_usage: memory,
    });

    if history.len() > MAX_HISTORY_POINTS {
        history.remove(0);
    }
}
```

- [ ] **步骤 5：创建 system_monitor/mod.rs**

```rust
pub mod info;
pub mod history;

pub use self::info::*;
pub use self::history::*;
```

- [ ] **步骤 6：创建 commands/system.rs**

```rust
use tauri::State;
use std::sync::Mutex;
use sysinfo::System;
use crate::models::system::{SystemInfo, ProcessInfo, HistoryPoint};
use anyhow::Result;
use crate::services::system_monitor::{self};

static SYSTEM: Mutex<System> = Mutex::new(System::new());

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let mut system = SYSTEM.lock().unwrap();
    system_monitor::info::get_system_info(&mut system)
}

#[tauri::command]
pub fn process_list() -> Vec<ProcessInfo> {
    let mut system = SYSTEM.lock().unwrap();
    system_monitor::info::get_process_list(&mut system)
}

#[tauri::command]
pub fn kill_process(pid: u32) -> Result<bool, String> {
    let success = system_monitor::info::kill_process(pid);
    if success {
        Ok(true)
    } else {
        Err("Failed to kill process".to_string())
    }
}

#[tauri::command]
pub fn system_history() -> Result<Vec<HistoryPoint>, String> {
    let app_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get app directory".to_string())?;
    let history_path = app_dir.join("data").join("system_history.json");
    let history = system_monitor::history::load_history(&history_path)
        .map_err(|e| e.to_string())?;
    Ok(history)
}
```

- [ ] **步骤 7：编译验证**

```bash
cd src-tauri && cargo check
```

预期：无编译错误。

- [ ] **步骤 8：Git commit**

```bash
git add src-tauri/src
git commit -m "feat: add system monitor backend"
```

---

### 任务 2.2：前端 - 系统监控仪表盘

**文件：**
- 创建：`src/modules/system-monitor/composables/use-system-monitor.ts`
- 创建：`src/modules/system-monitor/components/CpuCard.vue`
- 创建：`src/modules/system-monitor/components/MemoryCard.vue`
- 创建：`src/modules/system-monitor/components/DiskCard.vue`
- 创建：`src/modules/system-monitor/components/ProcessTable.vue`
- 创建：`src/modules/system-monitor/pages/DashboardPage.vue`
- 创建：`src/models/system.ts` (前端类型定义)

- [ ] **步骤 1：创建前端类型定义 src/models/system.ts**

```typescript
export interface SystemInfo {
  cpu_usage: number
  memory_used: number
  memory_total: number
  memory_usage: number
  disks: DiskInfo[]
  network: NetworkInfo
  os_name: string
  os_version: string
  hostname: string
  boot_time: number
}

export interface DiskInfo {
  mount_point: string
  total: number
  used: number
  usage: number
}

export interface NetworkInfo {
  bytes_sent: number
  bytes_recv: number
  packets_sent: number
  packets_recv: number
}

export interface ProcessInfo {
  pid: number
  name: string
  cpu_usage: number
  memory_usage: number
  status: string
}

export interface HistoryPoint {
  timestamp: number
  cpu_usage: number
  memory_usage: number
}
```

- [ ] **步骤 2：创建 composable**

```typescript
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { SystemInfo, ProcessInfo, HistoryPoint } from '@/models/system'

export function useSystemMonitor() {
  const systemInfo = ref<SystemInfo | null>(null)
  const processList = ref<ProcessInfo[]>([])
  const history = ref<HistoryPoint[]>([])
  let refreshInterval: number | null = null

  async function refresh() {
    systemInfo.value = await invoke('system_info')
  }

  async function loadProcessList() {
    processList.value = await invoke('process_list')
  }

  async function loadHistory() {
    history.value = await invoke('system_history')
  }

  async function killPid(pid: number): Promise<boolean> {
    try {
      await invoke('kill_process', { pid })
      await loadProcessList()
      return true
    } catch (e) {
      console.error(e)
      return false
    }
  }

  onMounted(() => {
    refresh()
    loadProcessList()
    loadHistory()
    refreshInterval = window.setInterval(refresh, 2000)
  })

  onUnmounted(() => {
    if (refreshInterval) {
      clearInterval(refreshInterval)
    }
  })

  return {
    systemInfo,
    processList,
    history,
    refresh,
    loadProcessList,
    killPid,
  }
}
```

- [ ] **步骤 3：创建 CpuCard.vue**

```vue
<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('cpuUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="flex items-center justify-between mb-4">
        <span class="text-3xl font-bold">{{ cpuUsage?.toFixed(1) }}%</span>
      </div>
      <div class="w-full bg-secondary rounded-full h-4">
        <div
          class="h-4 rounded-full bg-primary transition-all duration-500"
          :style="{ width: `${Math.min(cpuUsage, 100)}%` }"
        ></div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  cpuUsage: number
}>(), {})
const { t } = useI18n()
</script>
```

- [ ] **步骤 4：创建 MemoryCard.vue**

```vue
<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('memoryUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="flex items-center justify-between mb-4">
        <span class="text-3xl font-bold">{{ memoryUsage?.toFixed(1) }}%</span>
        <span class="text-muted-foreground">{{ formatBytes(memoryUsed) }} / {{ formatBytes(memoryTotal) }}</span>
      </div>
      <div class="w-full bg-secondary rounded-full h-4">
        <div
          class="h-4 rounded-full bg-primary transition-all duration-500"
          :style="{ width: `${Math.min(memoryUsage, 100)}%` }"
        ></div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
withDefaults(defineProps<{
  memoryUsed: number
  memoryTotal: number
  memoryUsage: number
}>(), {})

const { t } = useI18n()

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}
</script>
```

- [ ] **步骤 5：创建 DiskCard.vue**

```vue
<template>
  <card class="h-full">
    <card-header>
      <h3 class="font-semibold">{{ $t('diskUsage') }}</h3>
    </card-header>
    <card-content>
      <div class="space-y-4">
        <div v-for="disk in disks" :key="disk.mount_point" class="space-y-2">
          <div class="flex items-center justify-between text-sm">
            <span class="font-medium">{{ disk.mount_point }}</span>
            <span>{{ disk.usage.toFixed(1) }}%</span>
          </div>
          <div class="w-full bg-secondary rounded-full h-2">
            <div
              class="h-2 rounded-full bg-primary transition-all duration-500"
              :style="{ width: `${Math.min(disk.usage, 100)}%` }"
            ></div>
          </div>
          <div class="text-xs text-muted-foreground">
            {{ formatBytes(disk.used) }} / {{ formatBytes(disk.total) }}
          </div>
        </div>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { DiskInfo } from '@/models/system'
withDefaults(defineProps<{
  disks: DiskInfo[]
}>(), {})

const { t } = useI18n()

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}
</script>
```

- [ ] **步骤 6：创建 ProcessTable.vue**

```vue
<template>
  <card class="h-full">
    <card-header class="flex items-center justify-between">
      <h3 class="font-semibold">{{ $t('processList') }}</h3>
      <button @click="refresh" class="px-3 py-1 text-sm rounded-md bg-primary/10 hover:bg-primary/20">
        {{ $t('refresh') }}
      </button>
    </card-header>
    <card-content>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b">
              <th class="text-left py-2 px-2">PID</th>
              <th class="text-left py-2 px-2">{{ $t('name') }}</th>
              <th class="text-right py-2 px-2">CPU %</th>
              <th class="text-right py-2 px-2">Mem %</th>
              <th class="text-center py-2 px-2">{{ $t('status') }}</th>
              <th class="text-center py-2 px-2">{{ $t('action') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="proc in processes" :key="proc.pid" class="border-b">
              <td class="py-2 px-2 font-mono">{{ proc.pid }}</td>
              <td class="py-2 px-2 font-medium">{{ proc.name }}</td>
              <td class="py-2 px-2 text-right">{{ proc.cpu_usage.toFixed(1) }}</td>
              <td class="py-2 px-2 text-right">{{ proc.memory_usage.toFixed(1) }}</td>
              <td class="py-2 px-2 text-center">{{ proc.status }}</td>
              <td class="py-2 px-2 text-center">
                <button
                  @click="onKill(proc.pid)"
                  class="px-2 py-1 text-xs rounded-md bg-destructive/10 text-destructive hover:bg-destructive/20"
                >
                  {{ $t('killProcess') }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </card-content>
  </card>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import type { ProcessInfo } from '@/models/system'
import { useConfirm } from 'vue-climati'
withDefaults(defineProps<{
  processes: ProcessInfo[]
  onRefresh: () => void
  onKill: (pid: number) => Promise<void>
}>(), {})

const { t } = useI18n()
const { confirm } = useConfirm()

async function refresh() {
  onRefresh()
}

async function onKill(pid: number) {
  const ok = await confirm({
    title: t('killProcess'),
    description: t('confirmKillProcess'),
  })
  if (ok) {
    await onKill(pid)
  }
}
</script>
```

- [ ] **步骤 7：创建 DashboardPage.vue**

```vue
<template>
  <div class="space-y-4">
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      <CpuCard v-if="systemInfo" :cpu-usage="systemInfo.cpu_usage" />
      <MemoryCard
        v-if="systemInfo"
        :memory-used="systemInfo.memory_used"
        :memory-total="systemInfo.memory_total"
        :memory-usage="systemInfo.memory_usage"
      />
    </div>
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <DiskCard v-if="systemInfo" :disks="systemInfo.disks" />
      <ProcessTable
        :processes="processList"
        :on-refresh="loadProcessList"
        :on-kill="killPid"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import CpuCard from '../components/CpuCard.vue'
import MemoryCard from '../components/MemoryCard.vue'
import DiskCard from '../components/DiskCard.vue'
import ProcessTable from '../components/ProcessTable.vue'
import { useSystemMonitor } from '../composables/use-system-monitor'
import { useAppStore } from '@/stores/app'

const { t } = useI18n()
const appStore = useAppStore()
appStore.setCurrentTitle(t('systemMonitor'))

const { systemInfo, processList, loadProcessList, killPid } = useSystemMonitor()
</script>
```

- [ ] **步骤 8：类型检查**

```bash
npx vue-tsc --noEmit
```

预期：无类型错误。

- [ ] **步骤 9：Git commit**

```bash
git add src/modules/system-monitor src/models
git commit -m "feat: add system monitor dashboard frontend"
```

---

## （由于篇幅限制，剩余任务概要...）

完整后续任务包括：

---

## 阶段 3：软件管理模块

### 任务 3.1：后端 - 软件管理核心和提供者接口
- 创建 `src-tauri/src/services/software_manager/types.rs`
- 创建 `src-tauri/src/services/software_manager/installer.rs`
- 创建 `src-tauri/src/services/software_manager/manager.rs`
- 创建 `src-tauri/src/services/software_manager/providers/mod.rs`
- 创建 providers: `mysql.rs`, `jdk.rs`, `redis.rs`, `nginx.rs`
- 创建 `src-tauri/src/commands/software.rs`

### 任务 3.2：前端 - 软件管理页面
- 创建 `src/models/software.ts` 前端类型
- 创建 `src/modules/software-manager/composables/use-software-manager.ts`
- 创建组件：`SoftwareList.vue`, `SoftwareCard.vue`, `InstallDialog.vue`, `ConfigEditor.vue`
- 创建页面：`SoftwareListPage.vue`, `RepositoryPage.vue`

---

## 阶段 4：SpringBoot 管理模块

### 任务 4.1：后端 - SpringBoot 管理
- 创建 `src-tauri/src/services/springboot_manager/types.rs`
- 创建 `src-tauri/src/services/springboot_manager/process.rs`
- 创建 `src-tauri/src/services/springboot_manager/starter.rs`
- 创建 `src-tauri/src/services/springboot_manager/manager.rs`
- 创建 `src-tauri/src/commands/springboot.rs`

### 任务 4.2：前端 - SpringBoot 管理页面
- 创建 `src/models/springboot.ts` 前端类型
- 创建 `src/modules/springboot-manager/composables/use-springboot-manager.ts`
- 创建组件：`AppList.vue`, `AppCard.vue`, `EditDialog.vue`, `GroupConfig.vue`, `JvmMonitor.vue`, `LogViewer.vue`
- 创建页面：`SpringBootPage.vue`

---

## 阶段 5：配置和系统服务模块

### 任务 5.1：后端 - 配置管理和系统服务注册
- 创建 `src-tauri/src/commands/config.rs`
- 创建 `src-tauri/src/commands/service.rs`
- 创建 `src-tauri/src/services/service_registry/mod.rs`
- 创建平台相关实现：`windows.rs`, `linux.rs`, `macos.rs`

### 任务 5.2：前端 - 设置页面
- 创建 `src/modules/settings/pages/SettingsPage.vue`
- 添加所有设置项：主题、语言、关闭窗口行为、系统服务注册、镜像源等

### 任务 5.3：系统托盘和窗口行为
- 集成 `tauri-plugin-tray`
- 实现托盘菜单
- 根据配置处理关闭窗口事件

---

## 阶段 6：自动启动顺序

### 任务 6.1：后端 - 自动启动逻辑
- 在应用启动时读取所有启用自动启动的软件和应用
- 按 `startupOrder` 排序
- 顺序启动，同顺序并行
- 发送进度事件到前端

### 任务 6.2：前端 - 启动进度显示
- 在启动时显示启动进度对话框

---

## 阶段 7：构建和测试

### 任务 7.1：开发模式测试
```bash
npm run tauri dev
```
验证所有基础功能正常

### 任务 7.2：生成生产构建
```bash
npm run tauri build
```
验证构建成功

