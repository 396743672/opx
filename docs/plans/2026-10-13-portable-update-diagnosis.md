# opx 便携版更新导致已装软件"丢失" / 版本回退 —— 问题诊断与修改计划

> 诊断日期：2026-10-13 ｜ 用户机器：`C:\Users\Runic\Desktop\opx`（免安装版，原 0.5.1）
> 结论先行：**根因是「便携版」与「应用内更新器」不兼容**——点更新会装出第二份独立安装，便携目录的 exe 与数据从未被动；用户看到的"0.5.2"和"mysql 丢失"都来自那份新装，删除残留后便携 exe 仍是 0.5.1，故"版本回退"。

---

## 一、现场证据（用户机器实测）

| 观察项 | 事实 | 含义 |
|---|---|---|
| `opx.exe` 时间戳 | **Sep 21 02:10**，30.8 MB | 更新从未替换 exe → 实际仍是 **0.5.1** |
| `apps/ config/ data/ logs/ tmp/` 时间戳 | 全部 **Oct 13 16:41** | 用户手动删除后重跑 app 重建的空目录 |
| "删除除 cache + opx.exe" 后版本 | 0.5.1 | 印证 exe 一直没被替换 |
| `config/installed.json` | 含 `mysql 8.4.11` 记录（install_time 2026-10-13T08:41），`apps/mysql/8.4.11` 存在 | 便携目录里 mysql 注册/目录本就在 |
| `cache/` | 保留 mysql/minio/redis 下载（Sep 20~22） | 未删 → 重装可离线秒装 |

---

## 二、根因分析

### 根因 A（P0）：便携版没有合法更新目标，点"更新"会静默装出第二份
- 更新器走 `tauri_plugin_updater::download_and_install`（`src-tauri/src/utils/update.rs:129`），Windows 取 `latest.json` 的 `windows-x86_64` 条目。
- `latest.json` 的 windows 条目 = **NSIS 安装包**（`scripts/merge-latest.mjs:53` 显式指向 `opx_*_x64-setup.exe`，配合 `updaterJsonPreferNsis:true`，见 `release-and-network.md`）。
- Tauri 默认 NSIS `InstallDir` = `$LOCALAPPDATA\{productName}` = `C:\Users\Runic\AppData\Local\opx`。便携版从未经 NSIS 注册（无旧安装路径），故 NSIS 把 **0.5.2 装到 AppData 一份全新副本**（空 `apps/`）。
- 便携目录的 `opx.exe`（0.5.1）**完全没动**。用户看到的"更新到 0.5.2"来自 AppData 那份；`update.rs:128` 注明 NSIS 安装后会自动退出并拉起新安装，于是用户重开看到空 0.5.2 → 误判"mysql 丢失"。
- 用户后续手动删除便携目录残留（含 `apps/`），便携 mysql 数据被一并清掉；再开便携 exe 自然是 0.5.1 → "版本回退"。

### 根因 B（P0）：可变数据与 exe 同目录耦合，任何整目录更新都毁数据
- `app_root()` = exe 父目录（`crates/opx-core/src/utils/paths.rs:10,52-75`），`apps/ config/ data/ settings.json/ cache/` 全在 exe 同级（`resolve_install_path`/`resolve_data_path` 拼接 `apps_dir()`/`data_dir()`）。
- 只要更新逻辑"覆盖整个目录"就会删 `apps/` → 数据丢失。NSIS 若某天 `InstallDir` 指到便携目录即触发；将来做便携 in-place 更新若用整包解压也会触发。

### 根因 C（P1）：更新器未区分便携/安装版，UI 对便携用户开放了无效更新
- `useUpdater.ts` / `AboutPage.vue` 对所有桌面用户开放"检查更新/立即更新"，**无便携检测、无提示**。
- `release.yml:225` 发布说明已写明"便携版不含自动更新，需手动下载"，但 app 内未落实拦截。

### 根因 D（P1）：版本显示歧义，双份共存时用户无法分辨
- `app_version` 读嵌入 `tauri.conf.json`（`system.rs:34`），便携 0.5.1 与 AppData 0.5.2 各自显示自己版本，UI 无"便携/安装"标识。

---

## 三、问题清单

1. **[P0]** 便携版点"更新"静默装出第二份独立安装，数据不随更新迁移，用户迷惑于"版本回退/软件丢失"。
2. **[P0]** 数据目录与 exe 同目录，更新/解压路径若覆盖目录即丢 `apps/`（数据损毁隐患）。
3. **[P1]** 更新器对便携用户无检测、无拦截、无引导。
4. **[P1]** 关于页版本无"便携/安装"标识，双份共存时无法区分。
5. **[P2]** 缺少"覆盖 exe 目录前的数据保护"护栏（任何覆盖操作应先备份或明确拒绝 `apps/ config/ data/`）。

---

## 四、修改计划

### 方案选择
- **方案 1（推荐，最短平快，明天先做）**：便携检测 + 拦截引导。CI 打便携 zip 时写入 `portable.mode` 标记；app 识别后进入便携模式；便携模式"立即更新"**不调 NSIS**，改为提示"请到 Releases 下载新版便携 zip，仅覆盖 `opx.exe` 即可，`apps/config/data` 会在你的目录里被保留"。
- **方案 2（正确但量大，P1 后续）**：实现便携 in-place 更新——`latest.json` 增加 windows 便携条目（`opx.exe` + `resources` 的 zip + 独立 `.sig`），自定义下载/校验/原子替换（下载到 `tmp`，重启后用 `MoveFileEx` 替换 `opx.exe`，保留 `apps/config/data/cache`）。需改 `update.rs` + 前端 + CI 产物。
- **方案 3（治本但破坏性大）**：数据根与 exe 解耦，迁移到 `%APPDATA%/opx`（或 `OPX_APP_ROOT`），exe 目录只放程序；更新整目录也安全。需写从 exe 目录到 APPDATA 的迁移并兼容旧布局。

### 推荐落地（明天按此执行）
分支：`feat/portable-update-guard`（基于 dev 拉，合 dev 即删；遵循项目"合并即删分支"规范）

1. **CI（release.yml 行 172-182 打包便携 zip 处）**：在 zip 内 `opx.exe` 同级写入 `portable.mode` 空标记文件。
2. **`paths.rs`**：新增 `is_portable()`——检测 `app_root()/portable.mode` 是否存在（兜底：exe 不在 Program Files / LocalAppData 且目录可写）。
3. **`utils/update.rs`**：`install_update` 在便携模式下**不下载 NSIS**，返回明确错误 / 推 `portable-no-auto-update` 事件；`check_for_update` 仍可用（显示可用版本，安装被拦）。
4. **`useUpdater.ts` + `AboutPage.vue`**：便携模式下"立即更新"按钮变为"下载便携版"链接（指向 Releases 便携 zip），并显示提示条"便携版不走应用内更新，覆盖 `opx.exe` 即可保留数据"。
5. **数据护栏（防御性）**：installer / 解压 / 任何覆盖 exe 目录的路径，增加白名单——绝不允许删除 `apps/ config/ data/ cache/ settings.json`（即使将来做方案 2/3 也不误删）。
6. **关于页**：增加"便携版 / 安装版"标识（展示 `is_portable()` 结果）。

### 验证点
- 便携目录放 `portable.mode` → 点"更新"应**拦截并给引导**，不触发 NSIS、不生成第二份。
- 确认 NSIS `InstallDir`（build 出的 `target/release/bundle/nsis/*.nsi`）确为 `$LOCALAPPDATA`，坐实双份根因。
- 删除 `apps/` 后用保留的 `cache` 离线秒装 mysql，验证数据可恢复。
- `cargo test --lib` + `npx vue-tsc --noEmit` 通过；实机：便携点更新→拦截；安装版点更新→正常 NSIS。

---

## 五、临时规避（用户现在可做）
- **不要在便携目录里点"应用内更新"**；新版本请到 GitHub Releases 下载 `opx_x64-portable.zip`，解压后**只覆盖 `opx.exe`**（保留 `apps/ config/ data/ cache/`）。
- 用户机器现状：`Desktop\opx` 的 mysql 数据若已被手动删除，可用保留的 `cache` 离线重装（install 选 8.4.11，走缓存秒装）；另注意 `AppData\Local\opx` 可能已有一份 0.5.2 空安装，按需清理避免双份。

---

## 六、实施记录（2026-10-14，分支 `feat/portable-update-guard`）

### 源码级核实（tauri-plugin-updater 2.12.0 / tauri-utils 2.9.3 / 本仓 installer.nsi）
- `install_inner`(Windows)：`extract()` 识别 exe → `WindowsUpdaterType::Nsis` → `ShellExecuteW` 运行安装器 → **`process::exit(0)`**；便携 exe 不会被原位替换（坐实根因 A）。
- `get_urls`：便携 exe 的 `bundle_type()` 为 `None` → 目标键只有 `windows-x86_64` → 命中 `latest.json` 的 NSIS 条目。
- **实测本仓 `target/release/opx.exe` 的 bundle-type 标记为 `UNK`**（`grep -a -o`）——即便携 zip 内的 exe 未被打标，记忆「便携版被标 msi」在当前工具链下不成立；安装器内的 exe 才带标记。
- NSIS 默认 `InstallDir = $LOCALAPPDATA\opx`（installer.nsi:504），安装记录写 `HKCU/HKLM\Software\tauri-app\opx`（nsi:652，默认值 = INSTDIR）——第二份副本落点 + 便携检测的兜底信号。

### 改动清单
| 文件 | 改动 |
|---|---|
| `crates/opx-core/src/utils/paths.rs` | 新增 `PORTABLE_MARKER`、`is_portable()`、纯函数 `resolve_portable()` + 2 个单测 |
| `src-tauri/src/utils/update.rs` | `UpdateCheckResult.portable` 字段；`install_update` 便携模式**下载前** `bail!("i18n:portableUpdateBlocked")` |
| `src-tauri/src/commands/update.rs` | 新增 `is_portable_build` 命令（离线可取，与网络无关） |
| `src-tauri/src/lib.rs` / `http_registry.rs` | 注册命令；HTTP 侧按 `desktop_only` 约定登记 |
| `src/composables/useUpdater.ts` | `portable` ref + `loadPortable()`；`install()` 便携前置拦截（双保险） |
| `src/modules/settings/pages/AboutPage.vue` | 便携徽标；便携常驻提示；有新版时改为「下载便携版」按钮 + 引导文案 |
| `src/App.vue` | 启动 toast 便携版换文案（`updateAvailablePortable`） |
| `src/locales/{zh-CN,en-US}.ts` | 5 个新键（中英同步，已过孤儿键/一致性检查） |
| `.github/workflows/release.yml` | 便携 zip 同包打入 `portable.mode`；release notes 补「覆盖 opx.exe 保留数据」 |
| `README.md` / `README_zh.md` | 便携版与自动更新行为说明（双份同步） |

### 检测设计（与初版计划的差异）
- **主信号 = `portable.mode` 标记**（CI 便携 zip 内置），覆盖今后所有便携下载。
- **兜底 = Windows 安装痕迹**：`HKCU/HKLM\Software\tauri-app\opx` 注册表键、同目录 `uninstall.exe`。覆盖「历史便携版」与用户按临时规避方案**只覆盖 opx.exe** 的场景（该目录无标记）。
- **`bundle_type()` 未用作信号**：dev 构建也为 `None`，会误伤开发态更新流程验证；且 CI 打标行为有不确定性。
- 仅 Windows 有意义（CI 只对 windows 打便携 zip）；`debug_assertions` 构建恒非便携，保证 dev 可验证。
- 未引入方案 2/3（in-place 更新 / 数据根解耦），按计划留作后续。

### 验证
- `cargo check --workspace` 通过；`cargo test --lib` **连跑 3 次全绿**（338 + 24 + 10 passed，0 failed），含 2 个新增 `resolve_portable` 单测。
- `npx vue-tsc --noEmit` 零错误。
- 后端 `i18n:` key 与两套词条表一致性检查通过（15 键齐全）；新用图标均在离线子集内（无新增图标）。
- `release.yml` YAML 结构核对无误（改动均在既有 block scalar 内）。

### 待办（下次）
- 实机回归：打便携 zip → 解压 → 关于页应显「便携版」徽标 + 点更新被拦并给下载引导；NSIS 安装版点更新仍走原 updater 流程。
- 方案 2（便携 in-place 更新：`latest.json` 增 `windows-x86_64-portable` 条目 + 原子替换 exe）与方案 3（数据根解耦到 `%APPDATA%`）按需排期；届时再落地「覆盖 exe 目录前禁删 `apps/config/data/cache`」白名单护栏。
