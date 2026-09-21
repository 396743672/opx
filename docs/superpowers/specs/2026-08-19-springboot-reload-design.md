# R4 SpringBoot 快速重启（替换 Jar 并重启）设计

> 分支：`feat/roadmap2-r4`
> 日期：2026-08-19
> 前序：roadmap-2-design.md（R1 定时备份、R3 配置预设已合并 dev）。本设计为 R4：SpringBoot 快速重启。

## 现状

- SpringBoot 模块已有完整 JVM 参数模板（`AppFormDialog` 的 `JvmOptsTemplate`：xms/xmx/metaspace/gc_type/extra_flags），R4 的模板部分无需再做。
- 已有命令：
  - `restart_springboot_app`：`stop_app` → sleep 2s → `start_app`。
  - `replace_springboot_jar`：**运行中禁止换包**（Running 时返回「运行中的应用不可换包」）。流程：备份旧 jar → 复制覆盖 → 读新版本 → `update_version`。不重启。
- 前端现状：换新 jar 需手动「停止 → 替换Jar → 启动」三步。

## 目标

新增「替换并重启」：一键完成 停（若运行中）→ 换包 → 启动，运行中/停止时均可操作。保留原「替换Jar」按钮（停止时只换包不启动）。

## 方案

### 后端（src-tauri）

1. **提取共享函数** `replace_jar_file(old_jar: &Path, new_jar: &Path) -> anyhow::Result<(PathBuf, String)>`，放 `commands/springboot.rs`：
   - 校验新/原 jar 存在
   - 备份旧 jar 到 `{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak`
   - 复制新 jar 覆盖
   - 读新版本（`read_jar_version`，失败回退 "unknown"）
   - 返回 `(backup_path, new_version)`
   - 纯文件操作，不依赖 `SpringBootManager`，可直接单测

2. **重构现有 `replace_springboot_jar`**：保留 Running 校验 → 调 `replace_jar_file` → `update_version` → 返回 `ReplaceResult`。语义不变。

3. **新增命令** `replace_springboot_jar_and_restart(id, new_jar_path) -> Result<ReplaceResult, String>`：
   - 若状态非 Stopped/Error → `lifecycle::stop_app`（优雅停止）
   - `replace_jar_file` 换包
   - `update_version`
   - `lifecycle::start_app` 重启
   - 返回 `ReplaceResult`
   - `oplog!` 记录

4. 注册到 `lib.rs` 的 `invoke_handler`。

### 前端

1. `src/modules/springboot-manager/stores/springboot.ts`：新增 `replaceJarAndRestart(id, jarPath)` → `invoke('replace_springboot_jar_and_restart', { id, newJarPath })`。
2. `AppCard.vue`：新增「替换并重启」按钮（icon `mdi:package-up`），`Starting`/`Stopping` 时禁用，`Running`/`Stopped` 均可点；emit `replaceRestart` 事件。
3. `SpringBootPage.vue`：新增 `handleReplaceRestart`——复用 `open`（Tauri dialog）选 jar → `store.replaceJarAndRestart` → `fetchApps()` → 结果用 **toast**（成功 `replaceRestartSuccess` / 失败错误信息）。
4. i18n：`replaceAndRestart`、`replaceRestartSuccess`（中/英）。

## 边界

- 启动失败时 jar 已替换、备份已生成，数据安全；失败信息经 toast 呈现。
- 本功能为单应用操作，不影响服务组（栈）成员的重启路径。
- 原「替换Jar」语义保留（运行中禁止），供只换包不启动场景。

## 改动文件

- `src-tauri/src/commands/springboot.rs`（`replace_jar_file`、重构、新命令）
- `src-tauri/src/lib.rs`（命令注册）
- `src/modules/springboot-manager/stores/springboot.ts`
- `src/modules/springboot-manager/components/AppCard.vue`
- `src/modules/springboot-manager/pages/SpringBootPage.vue`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`

## 测试

- `replace_jar_file` 单测（临时目录假 jar）：备份文件生成、jar 内容被覆盖为新内容、新版本读取正确、原 jar 不存在时报错。
- 验证命令：`cargo test --lib` + `npx vue-tsc --noEmit`。

## 验收

1. 应用运行中，点「替换并重启」选新 jar → 自动停止 → 换包 → 启动，toast 提示成功，卡片状态 Running、版本更新。
2. 应用停止时点「替换并重启」→ 换包并启动。
3. 点原「替换Jar」在运行中仍被禁止，停止时可只换包不启动。
