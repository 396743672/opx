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