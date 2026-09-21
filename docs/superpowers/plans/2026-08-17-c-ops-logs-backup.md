# C 扩展方向：实例运维能力（日志查看器 + 备份/恢复）实施计划

> 计划类型：有序任务清单（含依赖、P0/P1/P2 分级、验收标准）
> 关联设计：`docs/superpowers/specs/2026-08-17-c-ops-logs-backup-design.md`
> 关联 PRD：`docs/superpowers/specs/2026-08-17-c-ops-logs-backup-prd.md`
> 分支：`feat/c-logs-backup`（本环境不编译，仅产出设计 + 计划文档）

## 分级总览

- **P0（核心交付）**：T1 前置改造 → T2 日志后端 → T3 备份后端 → T4 前端入口/store/i18n → T5 日志查看器组件 → T6 备份/恢复/重置组件
- **P1（应该有）**：T7 增强（级别过滤 / 快照备注 / 跨版本校验 / 恢复后提示）
- **P2（锦上添花）**：T8 增强（文件监听 / 自动清理 / 多实例对比）

依赖链：`T1 → (T2, T3)` → `(T5, T6)`；`T4` 与 `T2/T3` 可并行（仅调用命令）；`T7/T8` 依赖 `T5/T6`。

---

## P0 任务

### T1 ｜ [P0] 前置改造：stdout 落盘 + 新增 trait 方法与上下文/模型
- [ ] **涉及文件**：`src-tauri/src/services/software_manager/lifecycle.rs`（修改）、`providers/mod.rs`（修改）、`src-tauri/src/models/software.rs`（修改）
- [ ] **步骤要点**
  1. `lifecycle.rs` `spawn_process`（L124-142）：新增 `stdout_log_path(install_path, installed_id)` 辅助；将 L137-140 的 `.stdout(Stdio::null()).stderr(Stdio::null())` 改为打开 `logs/opx-<installed_id>.log` 并以 `Stdio::from(file)` 重定向（先 `create_dir_all(logs_dir)`；调用方在 spawn 前传入该路径）。
  2. `providers/mod.rs`：新增 `LogSource`/`LogSourceKind`/`LogContext`/`DataDirContext`；trait 增加默认方法 `log_sources()`（返回 StdoutRedirect，文件不存在返回空）、`data_dirs()`（返回 `<install_path>/data`）、`log_level_pattern()`（返回 None）。
  3. `models/software.rs`：新增可序列化 `LogSource`、`LogSourceKind`、`SnapshotMeta`、`LogChunk`、`BackupMode`。
- [ ] **验收标准**：`cargo build` 通过（如可编译）；`spawn_process` 启动后 `logs/opx-<id>.log` 生成且包含 stdout/stderr；新 trait 方法对所有现有 provider 编译通过（默认实现，零改动）。

### T2 ｜ [P0] 日志查看器后端：LogService + 命令
- [ ] **涉及文件**：`src-tauri/src/services/software_manager/log_viewer.rs`（新增）、`commands/software.rs`（修改）、`lib.rs`（修改）
- [ ] **步骤要点**
  1. 新增 `log_viewer.rs`：`list_log_sources(manager, id)`（取 provider 调 `log_sources`）；`read_log(path, offset, limit, keyword, regex, level)`（tail 模式 `offset=None` 读末尾 2000 行并返回 `end_offset`；历史模式按 offset 向前分页；关键字/正则/级别过滤）；`download_log(path, dest)`（拷贝）。
  2. `commands/software.rs` 新增 `get_log_sources`、`read_log`、`download_log`（沿用 `State<Arc<SoftwareManager>>` + `load_software_for_id` 模式）。
  3. `lib.rs` `generate_handler!` 注册上述 3 个命令。
- [ ] **验收标准**：启动实例后 `get_log_sources` 返回含 StdoutRedirect 来源；`read_log` 无 offset 返回末尾 2000 行 + `end_offset`；带 offset 轮询返回增量；关键字/级别过滤生效；`download_log` 将日志拷贝到指定路径。

### T3 ｜ [P0] 备份/恢复/重置后端：BackupService + 命令 + provider 覆盖
- [ ] **涉及文件**：`src-tauri/src/services/software_manager/backup.rs`（新增）、`commands/software.rs`（修改）、`lib.rs`（修改）、`providers/mongodb.rs`（修改）、`providers/minio.rs`（修改）、`providers/rustfs.rs`（修改）、`lifecycle.rs`（复用 `wipe_data_dir_if_nonempty`，新增 `reset_data_dirs`）
- [ ] **步骤要点**
  1. 新增 `backup.rs`：`create_snapshot`（压缩 `data_dirs()` → `<app_data>/backups/<id>/<ts>.zip`，写/更新 `manifest.json`）、`list_snapshots`（读 manifest）、`restore_snapshot`（运行态要求先停服；解压覆盖）、`delete_snapshot`、`reset_instance`（对每个 `data_dir` 重建空态，含「必须位于 install_path 下且为目录」护栏）。复用 `zip` crate `ZipWriter` + `walkdir`。
  2. `commands/software.rs` 新增 `create_snapshot`/`list_snapshots`/`restore_snapshot`/`delete_snapshot`/`reset_instance`。
  3. `lib.rs` 注册上述 5 个命令。
  4. `mongodb.rs` 覆盖 `log_sources`（追加 `ProviderFile(data/mongod.log, has_levels=true)`）+ `data_dirs`（按 config `dbpath`）；`minio.rs`/`rustfs.rs` 覆盖 `data_dirs`（按 config `data_dir` 解析）。
- [ ] **验收标准**：对运行中实例创建快照 → 生成 zip + manifest；列表可展示；恢复前未停服报错、停服后可覆盖；删除更新 manifest；一键重置后 `data_dir` 为空目录且安装根未受影响；mongodb 日志源含 mongod.log、minio 备份含自定义 data_dir。

### T4 ｜ [P0] 前端入口与 store/i18n 骨架
- [ ] **涉及文件**：`src/modules/software-manager/components/SoftwareInstanceRow.vue`（修改）、`pages/SoftwareListPage.vue`（修改）、`stores/ops.ts`（新增）、`src/models/software.ts`（修改）、`src/locales/zh-CN.ts` + `en-US.ts`（修改）
- [ ] **步骤要点**
  1. `SoftwareInstanceRow.vue`：在 `card-actions` 追加「日志」`mdi:file-document-outline`、「备份」`mdi:backup-restore` 按钮；`jre/jdk` 禁用（复用 `isRuntime`）；`defineEmits` 增加 `log`/`backup`。
  2. `SoftwareListPage.vue`：新增 `logTarget`/`backupTarget` ref 与 `onLog(item)`/`onBackup(item)`；挂载 `LogViewerDialog`/`BackupRestoreDialog`（`v-if` 模式，仿 L61-81）。
  3. `stores/ops.ts`：Pinia store 封装 8 个命令调用；日志轮询定时器（1500ms）、快照列表缓存、重置态。
  4. `software.ts`：新增 `LogSource`/`LogSourceKind`/`SnapshotMeta`/`LogChunk`/`BackupMode` 及命令参数/返回接口。
  5. i18n：新增 `logs`/`logViewer`/`backup`/`restore`/`reset`/`snapshot` 相关键（中英文）。
- [ ] **验收标准**：实例卡片出现「日志」「备份」按钮，jre/jdk 置灰；点击分别打开对应对话框；store 能成功 `invoke` 后端命令（联调）；i18n 文案无缺失 key。

### T5 ｜ [P0] 前端日志查看器组件
- [ ] **涉及文件**：`src/modules/software-manager/components/LogViewerDialog.vue`（新增）、依赖 `stores/ops.ts`、`SoftwareInstanceRow` 事件
- [ ] **步骤要点**
  1. 顶栏：实例选择下拉（默认当前实例，可切换其他非 Runtime 实例）+ 状态徽标 + 关闭。
  2. 日志源标签栏：列出 `get_log_sources` 结果，点击切换。
  3. 工具栏：关键字输入 + 正则开关、级别下拉（仅 `has_levels` 时显示）、实时开关、下载、自动滚动开关。
  4. 主区：默认渲染末尾 2000 行（等宽字体）；实时开启时持 `end_offset` 每 1.5s 轮询追加并自动滚动；顶部「加载更多历史」按 offset 向前分页；过滤命中高亮；「仅错误」快捷视图。
- [ ] **验收标准**：打开查看器默认显示末尾日志并自动滚动；实时开关下新日志持续追加；关键字/正则过滤正确；级别下拉仅对 `has_levels` 源出现；下载可保存日志到用户目录；多实例可切换。

### T6 ｜ [P0] 前端备份/恢复/重置组件
- [ ] **涉及文件**：`src/modules/software-manager/components/BackupRestoreDialog.vue`（新增）、依赖 `stores/ops.ts`
- [ ] **步骤要点**
  1. Tab A 快照列表：顶部「创建快照」按钮（可选名称/备注 + 勾选「先停止实例」）；表格列：名称/时间/大小/来源版本/备注；行操作：恢复 / 下载 / 删除。恢复 → 确认弹窗（停服提示 + 覆盖当前 data 警告）；删除 → 二次确认。
  2. Tab B 一键重置：说明文案（清空 data 回初始空态、需重新初始化）+「重置」按钮 → 强确认弹窗（勾选风险确认 + 输入实例名比对，类名不符禁止提交）。
- [ ] **验收标准**：创建快照后列表新增条目；恢复前未停服被拦截、停服后可恢复；删除更新列表；一键重置弹窗必须勾选 + 实例名匹配才能执行，执行后 data 清空；整体风格与现有 Dialog 一致（仿 `ConfigEditDialog`/`UninstallBlockedDialog`）。

---

## P1 任务

### T7 ｜ [P1] 增强：级别过滤 / 快照备注 / 跨版本校验 / 恢复后提示
- [ ] **涉及文件**：`providers/{mysql,postgresql,redis,nginx,nacos}.rs`（修改 `log_sources` 设 `has_levels=true` + `log_level_pattern`）、`backup.rs`（修改 `restore_snapshot` 加 `major_version` 校验、`create_snapshot` 记录 `major_version`）、`BackupRestoreDialog.vue`（修改恢复确认文案 + 恢复后提示）、`software.ts`（补充类型）
- [ ] **步骤要点**
  1. 为结构化 stdout 的 provider（mysql/postgresql/redis/nginx/nacos）覆盖 `log_sources` 标记 `has_levels=true` 并提供级别正则，启用级别下拉过滤（决策 6）。
  2. `create_snapshot` 解析并存储 `major_version`；`restore_snapshot` 校验 `source_key` + `major_version` 一致，不一致且 `force=false` 返回警告错误（决策 8，P1-B3）。
  3. 创建快照支持自定义名称/备注（P1-B1，T6 已留入口，此处接通 `name`/`note` 持久化到 `SnapshotMeta`）。
  4. 恢复成功后对需初始化的软件（PG/MySQL）弹「可能需要重新初始化」提示（P1-B2）。
- [ ] **验收标准**：级别下拉对 mysql/postgresql 等生效；跨大版本恢复弹警告且可「高级覆盖」继续；快照名称/备注正确显示；PG/MySQL 恢复后提示重新初始化。

---

## P2 任务

### T8 ｜ [P2] 增强：文件监听 / 自动清理 / 多实例对比
- [ ] **涉及文件**：`log_viewer.rs`（可选引入 `notify`）、`backup.rs`（保留策略）、`LogViewerDialog.vue`（多实例对比）、`Cargo.toml`（P2 才评估加 `notify`）
- [ ] **步骤要点**
  1. `notify` 监听日志文件变更增量推前端，替代/增强 1.5s 轮询（P2-L1）。
  2. 保留策略自动清理：每实例上限 N 个 / 总占用上限，清理最旧（P2-B1，manifest 已含 size/created_at）。
  3. 多实例同屏对比 / 分屏、跨实例关键字搜索（P2-L2）。
- [ ] **验收标准**：文件监听模式下日志实时性优于轮询且无空轮询开销；快照超出上限自动清理最旧；多实例可同屏对比。

---

## 备注

- 测试代码功能验证后自动清理（团队约定）；UI 改动遵循现有 Vue 组件风格（等宽日志、mdi 图标、CSS 变量主题）。
- 分支开发；PR 前确保 `cargo fmt`/`cargo clippy` 与前端类型检查通过（本环境不编译，仅文档产出）。
