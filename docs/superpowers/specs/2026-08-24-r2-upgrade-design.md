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

### 4. 升级动作（一键直装）

在 `SoftwareInstanceRow` 的「可升级」徽标/按钮点击：
- `invoke('install_software', { params: { key, version: target_version, mirror_index: 0, set_as_default_jre: false } })`
- 返回 install_id → `installStore.createTask(...)`，复用现有多任务进度面板。
- **新旧并存**：`install_software` 天然装到 `apps/{key}/{version}` 新目录 + 新 `InstalledSoftware` 记录，旧版保留可回滚。
- **不自动启动**：升级只负责安装，避免新版本端口冲突；用户自行停旧启新。
- 升级中该行禁用升级按钮并显示任务状态；完成后刷新已装列表。

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

- `src-tauri/src/commands/software.rs`：`check_upgrades` 命令 + `UpgradeInfo` 定义（可放 models）。
- `src-tauri/src/models/software.rs`：`UpgradeInfo`（serde）。
- 前端 `src/modules/software-manager/pages/SoftwareListPage.vue`：加载调 check_upgrades + 并行刷新 + 升级动作。
- 前端 `src/modules/software-manager/components/SoftwareInstanceRow.vue`：可升级徽标/按钮。
- `src/locales/zh-CN.ts` / `en-US.ts`：文案。

## 测试

- `compare_versions` 单测：跨大版本（5.7.44 vs 8.0.36）、同版本、前缀（1.31.2 vs 1.31）、非数字字段（v1 vs v2）。
- `check_upgrades` 用构造 catalog 数据验证返回 target 取最高。

## 不做（YAGNI）

- 大版本分支策略（仅同 major 可升）——默认跨版本可升，用户自行判断。
- 升级时自动停止旧版/迁移数据——新旧并存，用户手动切换。
- 全局「检查更新」聚合页或批量升级——首版只做行内一键。
- 升级完成后自动启动新版——明确不启动。