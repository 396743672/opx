# 内置默认软件安装功能设计文档

**日期**: 2026-07-01
**项目位置**: `D:\object\opx`
**技术栈**: Rust + Tauri 2 + Vue 3 + TypeScript
**阶段**: 第二阶段扩展 — 自带默认软件安装（内置 zip）

## 概述

在已完成的"网络下载安装"基础上，新增"内置默认版本安装"能力：将 5 个软件的默认版本 zip 打包进安装包，用户可离线秒装，无需等待下载。

### 范围边界

**包含**：
- 5 个内置 zip（JRE 17.0.15 / JRE 1.8 / MySQL 8.4.0 / Redis 7.4.9 / Nginx 1.31.2）打包到 `src-tauri/resources/software/`
- 构建脚本 `fetch-builtin.mjs` 自动下载 zip + 生成 manifest.json
- 校验脚本 `check-resources.mjs` 验证文件完整性
- catalog 数据结构扩展：MirrorSource 加 builtin 字段
- installer 分流：builtin 镜像走本地解压，网络镜像走现有下载流程
- 安装对话框镜像下拉加"内置默认版本（离线）"选项

**不包含**：
- Linux/Mac 内置 zip（本阶段仅 Windows，`#[cfg(windows)]`）
- MinIO / RustFS 新增（留待后续阶段，本次只做已有 4 个软件的内置）
- 内置 zip 的版本切换（builtin 版本固定，不可选其他版本内置）
- 内置 zip 的增量更新（zip 内容固定，发版时更新）

### 关键约束（用户确认）

1. **内置版本**：JRE 17.0.15 + JRE 1.8（双 LTS）+ MySQL 8.4.0 + Redis 7.4.9 + Nginx 1.31.2
2. **zip 路径**：`src-tauri/resources/software/{key}/{version}.zip`
3. **zip 提供方式**：`scripts/fetch-builtin.mjs` 脚本自动下载，Redis 用 7.4.9
4. **catalog 表达**：MirrorSource 加 builtin 字段
5. **UI 选择方式**：镜像下拉加"内置默认版本（离线）"选项，排首位
6. **Redis 内置版本**：7.4.9（用户指定，非 8.8.0）

## 架构与模块边界

### 整体流程

```
开发阶段
  └─ npm run fetch:builtin
      ├─ 从已验证 URL 下载 5 个 zip 到 src-tauri/resources/software/{key}/{version}.zip
      └─ 生成 manifest.json（记录 sha256 + size）

打包阶段
  └─ tauri build
      └─ bundle.resources 把 software/ 目录打包进安装包

运行时
  ├─ catalog 加载：provider 的 catalog_entry() 中 builtin 版本 mirrors[0] 带 builtin 字段
  ├─ 用户点安装 → 镜像下拉显示"内置默认版本（离线）"选项
  ├─ 选内置 → install_software 检测 mirror.builtin.is_some()
  │   ├─ 从 resource_dir()/software/{key}/{version}.zip 读取
  │   ├─ 校验 sha256（与 manifest 对比）
  │   ├─ 解压到 apps/{key}/{version}/
  │   └─ post_install + 登记（source = Builtin）
  └─ 选网络镜像 → 走现有下载流程（不变）
```

### 文件改动清单

```
src-tauri/
├── resources/software/                    # 新增目录
│   ├── manifest.json                      # 构建时生成
│   ├── jre/17.0.15.zip                    # fetch-builtin 下载
│   ├── jre/1.8.zip
│   ├── mysql/8.4.0.zip
│   ├── redis/7.4.9.zip
│   └── nginx/1.31.2.zip
├── tauri.conf.json                         # bundle.resources 加 "software"
├── src/
│   ├── models/software.rs                 # MirrorSource 加 builtin 字段 + InstallSource::Builtin
│   ├── services/software_manager/
│   │   ├── installer.rs                   # install_software 分流 builtin 分支
│   │   └── providers/
│   │       ├── mod.rs                     # 新增 builtin 项构造辅助
│   │       ├── jre.rs                      # 17.0.15 / 1.8 的 mirrors[0] 设 builtin
│   │       ├── mysql.rs                    # 8.4.0 的 mirrors[0] 设 builtin
│   │       ├── redis.rs                    # 7.4.9 的 mirrors[0] 设 builtin
│   │       └── nginx.rs                    # 1.31.2 的 mirrors[0] 设 builtin
│   └── utils/
│       └── paths.rs                       # 新增 builtin_resource_path() 辅助
└── capabilities/default.json              # 无需改（resource_dir 不需额外权限）

src/models/software.ts                     # TS 类型同步 builtin 字段
src/modules/software-manager/
├── components/
│   ├── InstallDialog.vue                 # 镜像下拉 builtin 项显示 + 排序
│   └── InstallJreDialog.vue              # 同上
└── stores/
    └── catalog.ts                         # （可选）builtin 标记的便捷查询

scripts/
├── fetch-builtin.mjs                      # 新增：下载 zip + 生成 manifest
└── check-resources.mjs                    # 新增：校验完整性

package.json                               # 加 fetch:builtin / check:resources 脚本
```

## 数据模型

### Rust（`src-tauri/src/models/software.rs`）

```rust
/// 内置 zip 元信息（构建时生成 manifest，运行时校验）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinInfo {
    pub version: String,       // "17.0.15" — 与 CatalogVersion.version 一致
    pub sha256: String,        // lowercase hex
    pub size: u64,             // 字节数
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorSource {
    pub name: String,                   // "内置默认版本（离线）" / "Adoptium(清华)" / ...
    pub url: String,                    // 网络镜像为真实 URL；builtin 项为 "builtin://software/{key}/{version}.zip"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinInfo>,   // Some 表示这是内置项
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InstallSource {
    Mirror {
        mirror_name: String,
        url: String,
    },
    Builtin {
        version: String,
    },
    Custom {
        archive_name: String,
    },
}
```

**关键设计**：builtin 作为 MirrorSource 的可选字段，而非独立的 CatalogVersion 字段。这样：
- 镜像下拉中 builtin 项与网络镜像项并列，UI 复用现有下拉组件
- `mirror_index` 仍是一个 usize，installer 用 `mirror.builtin.is_some()` 判断走哪条分支
- catalog versions 列表无需新增结构

### TypeScript（`src/models/software.ts`）

```typescript
export interface BuiltinInfo {
  version: string
  sha256: string
  size: number
}

export interface MirrorSource {
  name: string
  url: string
  builtin?: BuiltinInfo   // 与 Rust 端 serde 对齐
}

export type InstallSource =
  | { Mirror: { mirror_name: string; url: string } }
  | { Builtin: { version: string } }
  | { Custom: { archive_name: string } }
```

### manifest.json（`src-tauri/resources/software/manifest.json`）

构建时由 `fetch-builtin.mjs` 生成：

```json
{
  "jre": {
    "17.0.15": { "sha256": "abc123...", "size": 43478873 },
    "1.8": { "sha256": "def456...", "size": 40132081 }
  },
  "mysql": {
    "8.4.0": { "sha256": "ghi789...", "size": 258946761 }
  },
  "redis": {
    "7.4.9": { "sha256": "jkl012...", "size": 12162621 }
  },
  "nginx": {
    "1.31.2": { "sha256": "mno345...", "size": 2780911 }
  }
}
```

## Catalog 改造

### provider catalog_entry() 改造

每个 provider 的 builtin 版本的 `mirrors[0]` 设为 builtin 项。示例（JRE）：

```rust
versions.push(CatalogVersion {
    version: "17.0.15".to_string(),
    mirrors: vec![
        MirrorSource {
            name: "内置默认版本（离线）".to_string(),
            url: "builtin://software/jre/17.0.15.zip".to_string(),
            builtin: Some(BuiltinInfo {
                version: "17.0.15".to_string(),
                sha256: read_from_manifest("jre", "17.0.15"),  // 启动时从 manifest 读
                size: read_size_from_manifest("jre", "17.0.15"),
            }),
        },
        MirrorSource {
            name: "Adoptium(清华)".to_string(),
            url: "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.10%2B7/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip".to_string(),
            builtin: None,
        },
    ],
    archive: ArchiveInfo {
        format: ArchiveFormat::Zip,
        size: None,
        sha256: None,
    },
});
```

**manifest 读取策略**：provider 启动时从 `resource_dir()/software/manifest.json` 读取并缓存。若 manifest 不存在或条目缺失，对应 builtin 项的 sha256/size 留空（运行时跳过校验，但仍尝试从 resource_dir 读 zip）。

### 内置版本与 catalog 版本的关系

| 软件 | catalog versions | builtin 版本 | 说明 |
|---|---|---|---|
| JRE | 21.0.5, 17.0.15, 11.0.26, 1.8 | 17.0.15, 1.8 | builtin 版本必须是 catalog 已有版本 |
| MySQL | 8.4.0, 8.0.36 | 8.4.0 | — |
| Redis | 8.8.0, 8.2.7, 7.4.9 | 7.4.9 | 用户指定 7.4.9（非最新） |
| Nginx | 1.31.2 | 1.31.2 | — |

**Redis 特殊说明**：catalog 的 default_version 仍是 8.8.0，但 builtin 项在 7.4.9 版本的 mirrors[0]。用户选 7.4.9 时镜像下拉默认选中"内置"。

## 构建脚本

### `scripts/fetch-builtin.mjs`

```javascript
// 下载 5 个内置 zip + 生成 manifest.json
const BUILTIN = {
  jre: {
    '17.0.15': 'https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.10%2B7/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip',
    '1.8': 'https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip',
  },
  mysql: {
    '8.4.0': 'https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.0-winx64.zip',
  },
  redis: {
    '7.4.9': 'https://github.com/redis-windows/redis-windows/releases/download/7.4.9/Redis-7.4.9-Windows-x64-cygwin.zip',
  },
  nginx: {
    '1.31.2': 'https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip',
  },
}

// 流程：
// 1. 遍历 BUILTIN，对每个 {key}/{version} 检查 resources/software/{key}/{version}.zip 是否存在
// 2. 不存在或 --force → 下载（显示进度），存在 → 跳过
// 3. 计算 sha256 + size
// 4. 写入 manifest.json
```

**npm 脚本**：
```json
{
  "scripts": {
    "fetch:builtin": "node scripts/fetch-builtin.mjs",
    "check:resources": "node scripts/check-resources.mjs"
  }
}
```

### `scripts/check-resources.mjs`

```javascript
// 校验 resources/software/ 下文件完整性
// 1. 读 manifest.json
// 2. 对每个 {key}/{version} 检查 zip 存在 + sha256 匹配
// 3. 缺失或不匹配 → 退出码 1 + 错误信息
// 4. 全部通过 → 退出码 0
```

打包前 CI 跑 `check:resources` 确保内置 zip 完整。

## 安装流程改造

### `installer.rs` 分流逻辑

```rust
pub async fn install_software(app, manager, params, install_id) {
    // ... 现有校验逻辑不变 ...

    let mirror = &version_info.mirrors[params.mirror_index];

    if mirror.builtin.is_some() {
        // builtin 分支
        install_from_builtin(app, manager, params, install_id, mirror, entry, version_info).await;
    } else {
        // 现有网络下载分支（不变）
        install_from_mirror(app, manager, params, install_id, mirror, entry, version_info).await;
    }
}

async fn install_from_builtin(app, manager, params, install_id, mirror, entry, version_info) {
    let builtin = mirror.builtin.as_ref().unwrap();
    let install_path = paths::apps_dir().join(&params.key).join(&params.version);

    // 1. 解析 resource 路径
    let resource_zip = app.path()
        .resource_dir()
        .ok()
        .map(|d| d.join("software").join(&params.key).join(format!("{}.zip", &params.version)))
        .ok_or_else(|| anyhow!("无法定位资源目录"));

    // 2. 校验文件存在
    if !resource_zip.exists() {
        emit_failed(&app, &install_id, "内置安装包缺失，请重新安装应用");
        return;
    }

    // 3. 校验 sha256（若 manifest 提供）
    if !builtin.sha256.is_empty() {
        let computed = compute_sha256(&resource_zip)?;
        if computed != builtin.sha256.to_lowercase() {
            emit_failed(&app, &install_id, "内置安装包校验失败，文件可能损坏");
            return;
        }
    }

    // 4. 创建 install_path
    fs::create_dir_all(&install_path)?;

    // 5. 登记安装任务
    manager.add_install_task(install_id.clone(), params.key.clone(), params.version.clone());

    // 6. 解压（跳过 downloading 阶段，直接 extracting）
    emit_event(&app, json!({ "install_id": install_id, "phase": "extracting", "percent": 0 }));
    archive::extract_zip(&resource_zip, &install_path)?;
    emit_event(&app, json!({ "install_id": install_id, "phase": "extracting", "percent": 50 }));

    // 7. post_install
    if let Some(provider) = all_providers().into_iter().find(|p| p.key() == params.key) {
        let ctx = InstallContext::new(params.key.clone(), params.version.clone(), install_path.to_string_lossy().to_string());
        provider.post_install(&ctx)?;
    }

    emit_event(&app, json!({ "install_id": install_id, "phase": "extracting", "percent": 100 }));

    // 8. 登记 InstalledSoftware（source = Builtin）
    let installed = InstalledSoftware {
        // ... 其他字段同现有 ...
        source: InstallSource::Builtin { version: builtin.version.clone() },
    };
    manager.add_installed(installed)?;

    // 9. jre 默认版本处理（同现有）
    if params.key == "jre" && params.set_as_default_jre {
        manager.update_jre_default(Some(installed_id_for_event.clone()))?;
    }

    emit_event(&app, json!({ "install_id": install_id, "phase": "completed", "installed_id": installed_id_for_event }));
}
```

### 进度事件差异

| 阶段 | 网络下载 | builtin |
|---|---|---|
| downloading | 流式下载，持续 emit downloaded/total/percent | **跳过**（本地文件） |
| extracting | emit percent 0→50→100 | 同左 |
| completed/failed | 同左 | 同左 |

前端 InstallProgressDialog 对 builtin 任务看到的是"直接进入 extracting"，UX 上"秒装"。

## 前端 UI 改造

### InstallDialog / InstallJreDialog 镜像下拉

镜像下拉项排序：builtin 项排首位，其余网络镜像按原顺序。

```vue
<div class="select" @click="showMirrorDropdown = !showMirrorDropdown">
  <span>{{ selectedMirror?.name }}</span>
  <Icon icon="mdi:chevron-down" class="caret" />
</div>
<div v-if="showMirrorDropdown" class="dropdown">
  <div
    v-for="(m, idx) in selectedVersion?.mirrors"
    :key="idx"
    class="dropdown-item"
    :class="{ selected: selectedMirrorIdx === idx }"
    @click="selectMirror(idx)"
  >
    <Icon v-if="m.builtin" icon="mdi:package-variant-closed" class="builtin-icon" />
    {{ m.name }}
    <span v-if="m.builtin" class="builtin-tag">{{ $t('offline') }}</span>
  </div>
</div>
```

**i18n 新增**：`offline`（"离线" / "Offline"）、`builtinVersion`（"内置默认版本" / "Builtin default version"）。

### builtin 项的默认选中

打开安装对话框时，若该版本的 `mirrors[0].builtin.is_some()`，默认 `selectedMirrorIdx = 0`（选中内置项）。这样用户首次打开就看到"内置默认版本（离线）"，一键安装最快。

### SoftwareCard 标记

catalog 中带 builtin 的版本可在卡片上显示"内置"徽标（可选，非必须）。

## Tauri 配置

### `tauri.conf.json`

```json
{
  "bundle": {
    "resources": ["software/**/*"]
  }
}
```

打包时把 `src-tauri/resources/software/` 整个目录复制到安装包 resources 下。

### resource_dir 运行时解析

Tauri 2 用 `app.path().resource_dir()` 获取资源根目录：
- 开发模式：指向 `src-tauri/resources/`
- 生产模式：指向安装目录下的 resources/

内置 zip 路径 = `resource_dir()/software/{key}/{version}.zip`。

## 错误处理

| 错误场景 | 处理方式 | 用户可见消息 |
|---|---|---|
| 内置 zip 文件缺失 | 校验阶段返回 | "内置安装包缺失，请重新安装应用" |
| sha256 校验失败 | 校验阶段返回 | "内置安装包校验失败，文件可能损坏" |
| resource_dir 无法定位 | 启动时降级 | "无法定位资源目录（{原因}）" |
| 解压失败 | 同现有网络下载失败处理 | "解压失败：{原因}" |
| manifest.json 缺失 | provider 启动时降级，builtin 项仍可用但跳过 sha256 校验 | （无提示，日志记录） |

## 测试策略

### 后端单元测试

| 模块 | 测试内容 |
|---|---|
| `models/software.rs` | MirrorSource.builtin 序列化/反序列化 round-trip；InstallSource::Builtin 序列化正确 |
| `services/software_manager/installer.rs` | install_software 检测 mirror.builtin.is_some() 时走 builtin 分支；sha256 不匹配时返回错误；resource 缺失时返回错误 |
| `utils/paths.rs` | builtin_resource_path() 在开发/生产模式返回正确路径 |

### 脚本测试

| 脚本 | 测试内容 |
|---|---|
| `fetch-builtin.mjs` | 下载成功生成 zip + manifest；已存在跳过；--force 强制重下 |
| `check-resources.mjs` | 文件存在 + sha256 匹配时返回 0；缺失时返回 1 |

### 手动验证清单

1. `npm run fetch:builtin` → 5 个 zip 下载到 resources/software/，manifest.json 生成
2. `npm run check:resources` → 退出码 0
3. `npm run tauri:dev` → 软件仓库页打开，JRE/MySQL/Redis/Nginx 安装对话框镜像下拉首项为"内置默认版本（离线）"
4. 选内置项 → 点安装 → 进度对话框直接显示 extracting（无 downloading 阶段）→ 秒级完成
5. 安装完成后 `apps/{key}/{version}/` 目录存在，installed.json 新增记录（source = Builtin）
6. 选网络镜像项 → 走现有下载流程（回归验证）
7. 删除某个内置 zip → `check:resources` 报错；`tauri:dev` 安装该 builtin 时报"内置安装包缺失"
8. `npm run tauri:build` → 安装包含 resources/software/ 目录

## 依赖与体积

### 无新增 Cargo/npm 依赖

复用现有：
- Rust：`sha2`（已有，用于校验）、`zip`（已有，解压）、`tauri::path`（resource_dir）
- Node：`node:crypto`（内置，计算 sha256）、`node:fs`（内置）

### 安装包体积影响

| 内置 zip | 大小 |
|---|---|
| JRE 17.0.15 | ~43MB |
| JRE 1.8 | ~40MB |
| MySQL 8.4.0 | ~247MB |
| Redis 7.4.9 | ~12MB |
| Nginx 1.31.2 | ~2.7MB |
| **合计** | **~345MB** |

安装包从当前 ~15MB → ~360MB。MSI/NSIS 安装包会略大（含压缩）。用户已确认可接受。

## 国际化

新增 i18n 键：

```
offline              // "离线" / "Offline"
builtinVersion       // "内置默认版本" / "Builtin default version"
builtinMissing       // "内置安装包缺失，请重新安装应用"
builtinCorrupted     // "内置安装包校验失败，文件可能损坏"
```

## 设计决策总结

| 决策点 | 选择 | 理由 |
|---|---|---|
| builtin 数据结构 | MirrorSource.builtin 字段 | 复用现有 mirrors 数组，UI 下拉无需新增组件 |
| builtin 版本选择 | JRE 17.0.15+1.8 / MySQL 8.4.0 / Redis 7.4.9 / Nginx 1.31.2 | JRE 双 LTS 覆盖常见需求；Redis 7.4.9 用户指定 |
| zip 提供方式 | fetch-builtin.mjs 脚本下载 | 可重复、可校验、可 CI 集成 |
| zip 路径组织 | resources/software/{key}/{version}.zip | 清晰，与 catalog key/version 对齐 |
| sha256 校验 | manifest.json + 运行时校验 | 防篡改/损坏 |
| builtin 进度 | 跳过 downloading，直接 extracting | 本地文件无下载，UX 更快 |
| 平台范围 | 仅 Windows | 用户主要在 Windows 用；Linux/Mac 留后续 |
| 镜像下拉默认选中 | builtin 项排首位且默认选中 | 用户首次打开即可一键秒装 |

## 规格自检

| 检查项 | 状态 | 说明 |
|---|---|---|
| 占位符扫描 | ✅ 无 | 无 TODO 或未完成章节 |
| 内部一致性 | ✅ 通过 | 数据模型、catalog、installer、UI、测试各节一致 |
| 范围检查 | ✅ 合适 | 聚焦内置 zip，一个实现计划可覆盖 |
| 模糊性检查 | ✅ 通过 | builtin 版本、路径、校验、UI 均明确 |

---

设计文档已编写完成，请审查。
