# R2 软件升级检测 设计文档

日期：2026-08-24
分支：dev（任务分支待实现时从 dev 新建）

## 背景

软件仓库现有能力：`Catalog`（内置 + 远程合并缓存的版本列表）、`InstalledSoftware`（每条一个已装版本）、`install_software` 命令（装指定 `key`+`version` 到新目录、生成新记录，不动旧版本）。

R2 补「已装版本 vs catalog 最新对比，一键升级」。

## 方案

### 1. 数据模型（后端 `models/software.rs` 或命令层）

```rust
#[derive(Serialize, Deserialize)]
pub struct UpgradeInfo {
    pub key: String,
    pub name: String,
    pub current_version: String,
    /// 比当前高的最高可用版本；无可升级时 None
    pub target_version: Option<String>,
}
```

### 2. 检测命令 `check_upgrades`

新增 `#[tauri::command] pub async fn check_upgrades(...) -> Result<Vec<UpgradeInfo>, String>`：

- 取已装列表（`is_custom == false`）。
- 对每个已装软件，在 `catalog.entries` 中找 `e.key == sw.key`：
  - 若无 entry（自定义/已删条目）→ 跳过。
  - 在 `e.versions` 中筛 `compare_versions(v.version, sw.version) > 0`，取其中**最高**的作为 `target_version`。
  - `current_version` = 该已装软件版本。
- **纯内置对比，不联网**：立即返回，供前端首屏。

版本比较 `compare_versions(a, b)`：按 `.` 分段，逐段解析 u32 比较；含非数字段时退化为字符串比较。

### 3. 在线刷新（前端并行走现有命令）

前端 `SoftwareListPage` 挂载后：
1. 调 `check_upgrades()` → 立即渲染行内「可升级 vX」徽标。
2. 对每个已装软件的 key，**并发** `Promise.allSettled` 调已有 `fetch_remote_versions_for(key)`：
   - 成功：返回含远程版本的 entry，后端已 `merge_entry_versions` + 写 catalog 缓存；前端用返回的 `entry.versions` 重算该 key 的 target，更新徽标。
   - 失败/离线：静默忽略，保持内置判定。
   - （在线优先 + 失败回退，不阻塞首屏）

### 4. 升级动作（替换式 `upgrade_software`）

在 `SoftwareInstanceRow` 的「可升级」徽标/按钮点击，调**新后端命令** `upgrade_software(installed_id)`（不再走 `install_software` 并存）。

`upgrade_software` 流程（一次命令，进度经 installStore 面板）：
1. 目标版本 = 命令内按 catalog 计算（高于当前版本的最高版本）。
2. **若实例运行中 → 先停止**（lifecycle 停止，等待完成）。
3. **备份**：旧目录 `apps/{key}/{old_ver}` 重命名为 `apps/{key}/{old_ver}.bak`（若已存在同名 `.bak` 先删除，仅保留一份备份）。
4. **安装新版**：下载并解压新版到 `apps/{key}/{new_ver}`，执行 `post_install`（复用 installer 的下载/校验/解压流程）。
5. **自动迁移数据/配置**：从 `.bak` 复制用户数据到新版目录——`provider.data_dirs()`（默认 `data/`）、`provider.config_file_path()` 指向的文件、以及各软件保留的站点/数据区（nginx：`conf/sites/` + `sites-data/`）。
6. **记录合并**：删除旧 `InstalledSoftware` 记录；新记录（installer 生成）的 `config`、`auto_start_on_app_start`、`startup_order`、`port` 继承自旧记录（保留用户端口与启动设置）。
7. **不自动启动**；旧目录 `.bak` 保留在磁盘供回滚（列表不再出现，可手动删除）。
8. 完成后列表刷新（一条记录：新版本），不再有"Nginx 1.31.2 + 1.31.4 两条"。

### 4.5 回滚（`rollback_software`）

升级后同 key 存在 `<old_ver>.bak` 备份时，行内出现「回滚」按钮，调 `rollback_software(installed_id)`，与升级对称：
1. 检测到 `apps/{key}/*.bak` 存在（取第一个，解析旧版本），否则报"无可回滚备份"。
2. 若实例运行中 → `lifecycle::stop_one(pid)` 停止。
3. 当前新版目录 rename 为 `apps/{key}/{cur}.off`（保留，不覆盖要恢复的 `.bak`）。
4. `.bak` → rename 回 `apps/{key}/{old_ver}`。
5. installed 记录 version/install_path/name 更新为旧版（config 保留）。
6. 不自动启动；完成后列表刷新为一条旧版本记录。

前端：`UpgradeInfo` 增加 `rollback_to: Option<String>`（检测到 `.bak` 时填旧版本；由 `check_upgrades` 填充），有值时行内显示「可回滚 vX」按钮。

### 5. UI（`SoftwareListPage` + `SoftwareInstanceRow`）

- `SoftwareInstanceRow` 增加可选 prop：可升级目标版本。
- 行内展示徽标「可升级 vX」（`mdi:package-upgrade`），点击触发升级；无目标版本不显示。
- i18n 中英补充：`upgradeAvailable`、`upgrading` 等。

## 版本比较细节

纯函数 `compare_versions(a: &str, b: &str) -> Ordering`：
- 按 `.` split。
- 对应段均能 parse u32 → 逐段数字比较（a 短且前缀相等 → a 小）。
- 任一段非数字 → 直接 `a.cmp(b)`（字符串序，兜底）。

## 涉及文件

- `src-tauri/src/models/software.rs`：`UpgradeInfo`（serde）。
- `src-tauri/src/commands/software.rs`：`check_upgrades` 命令 + `upgrade_software` 命令（编排停止/备份/安装/迁移/记录合并）。
- `src-tauri/src/services/software_manager/installer.rs`：抽取下载+解压+post_install 可复用块供 upgrade 使用（避免复制逻辑）。
- `src-tauri/src/services/software_manager/providers/`：`data_dirs()`/`config_file_path()` 对齐（nginx 覆盖 data_dirs 返回 `sites-data`、`conf/sites`）。
- 前端 `src/modules/software-manager/pages/SoftwareListPage.vue`：升级动作改调 `upgrade_software`。
- 前端 `src/modules/software-manager/components/SoftwareInstanceRow.vue`：可升级徽标/按钮（已实现）。
- `src/locales/zh-CN.ts` / `en-US.ts`：文案。

## 测试

- `compare_versions` 单测：跨大版本（5.7.44 vs 8.0.36）、同版本、前缀（1.31.2 vs 1.31）、非数字字段（v1 vs v2）。
- `check_upgrades` / `compute_upgrades` 用构造 catalog 数据验证返回 target 取最高。
- `upgrade_software` 的纯逻辑部分抽函数测：备份目录改名（已存在 .bak 先删）、数据迁移复制路径集合（provider 声明的 data_dirs + config + nginx 站点区）。

## 不做（YAGNI）

- 大版本分支策略（仅同 major 可升）——默认跨版本可升，用户自行判断。
- 多版本并存的列表管理——升级即替换，列表保持一条。
- 全局「检查更新」聚合页或批量升级——首版只做行内一键。
- 升级完成后自动启动新版——明确不启动。