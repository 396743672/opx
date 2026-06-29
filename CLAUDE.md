<!-- superpowers-zh:begin (do not edit between these markers) -->
# Superpowers-ZH 中文增强版

本项目已安装 superpowers-zh 技能框架（20 个 skills）。

## 核心规则

1. **收到任务时，先检查是否有匹配的 skill** — 哪怕只有 1% 的可能性也要检查
2. **设计先于编码** — 收到功能需求时，先用 brainstorming skill 做需求分析
3. **测试先于实现** — 写代码前先写测试（TDD）
4. **验证先于完成** — 声称完成前必须运行验证命令

## 可用 Skills

Skills 位于 `.claude/skills/` 目录，每个 skill 有独立的 `SKILL.md` 文件。

- **brainstorming**: 在任何创造性工作之前必须使用此技能——创建功能、构建组件、添加功能或修改行为。在实现之前先探索用户意图、需求和设计。
- **chinese-code-review**: 中文 review 沟通参考——话术模板、分级标注（必须修复/建议修改/仅供参考）、国内团队常见反模式应对。仅在用户显式 /chinese-code-review 时调用，不要根据上下文自动触发。
- **chinese-commit-conventions**: 中文 commit 与 changelog 配置参考——Conventional Commits 中文适配、commitlint/husky/commitizen 中文模板、conventional-changelog 中文配置。仅在用户显式 /chinese-commit-conventions 时调用，不要根据上下文自动触发。
- **chinese-documentation**: 中文文档排版参考——中英文空格、全半角标点、术语保留、链接格式、中文文案排版指北约定。仅在用户显式 /chinese-documentation 时调用，不要根据上下文自动触发。
- **chinese-git-workflow**: 国内 Git 平台配置参考——Gitee、Coding.net、极狐 GitLab、CNB 的 SSH/HTTPS/凭据/CI 接入差异与镜像同步配置。仅在用户显式 /chinese-git-workflow 时调用，不要根据上下文自动触发。
- **dispatching-parallel-agents**: 当面对 2 个以上可以独立进行、无共享状态或顺序依赖的任务时使用
- **executing-plans**: 当你有一份书面实现计划需要在单独的会话中执行，并设有审查检查点时使用
- **finishing-a-development-branch**: 当实现完成、所有测试通过、需要决定如何集成工作时使用——通过提供合并、PR 或清理等结构化选项来引导开发工作的收尾
- **mcp-builder**: MCP 服务器构建方法论 — 系统化构建生产级 MCP 工具，让 AI 助手连接外部能力
- **receiving-code-review**: 收到代码审查反馈后、实施建议之前使用，尤其当反馈不明确或技术上有疑问时——需要技术严谨性和验证，而非敷衍附和或盲目执行
- **requesting-code-review**: 完成任务、实现重要功能或合并前使用，用于验证工作成果是否符合要求
- **subagent-driven-development**: 当在当前会话中执行包含独立任务的实现计划时使用
- **systematic-debugging**: 遇到任何 bug、测试失败或异常行为时使用，在提出修复方案之前执行
- **test-driven-development**: 在实现任何功能或修复 bug 时使用，在编写实现代码之前
- **using-git-worktrees**: 当需要开始与当前工作区隔离的功能开发，或在执行实现计划之前使用——通过原生工具或 git worktree 回退机制确保隔离工作区存在
- **using-superpowers**: 在开始任何对话时使用——确立如何查找和使用技能，要求在任何响应（包括澄清性问题）之前调用 Skill 工具
- **verification-before-completion**: 在宣称工作完成、已修复或测试通过之前使用，在提交或创建 PR 之前——必须运行验证命令并确认输出后才能声称成功；始终用证据支撑断言
- **workflow-runner**: 在 Claude Code / OpenClaw / Cursor 中直接运行 agency-orchestrator YAML 工作流——无需 API key，使用当前会话的 LLM 作为执行引擎。当用户提供 .yaml 工作流文件或要求多角色协作完成任务时触发。
- **writing-plans**: 当你有规格说明或需求用于多步骤任务时使用，在动手写代码之前
- **writing-skills**: 当创建新技能、编辑现有技能或在部署前验证技能是否有效时使用

## 如何使用

当任务匹配某个 skill 时，使用 `Skill` 工具加载对应 skill 并严格遵循其流程。绝不要用 Read 工具读取 SKILL.md 文件。

如果你认为哪怕只有 1% 的可能性某个 skill 适用于你正在做的事情，你必须调用该 skill 检查。

## Tauri + Vue 3 + TypeScript 项目最佳实践

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
```bash
# 1. 安装前端依赖（首次或 package.json 变更后）
npm install

# 2. 一键打包（自动跑前端构建 + Rust release 编译 + 生成安装包）
npm run tauri build

# 仅生成 MSI 安装包（更快，跳过 NSIS .exe）
npm run tauri build -- --bundles msi

# 仅生成 NSIS .exe 安装包
npm run tauri build -- --bundles nsis
```

### 产物位置
打包产物在 `src-tauri/target/release/bundle/` 下：
- `msi/opx_0.1.0_x64_zh-CN.msi` — MSI 安装包（推荐分发，企业部署友好）
- `nsis/opx_0.1.0_x64-setup.exe` — NSIS 安装程序
- `src-tauri/target/release/opx.exe` — 免安装可执行文件（需配合 WebView2 运行）

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

<!-- superpowers-zh:end -->
