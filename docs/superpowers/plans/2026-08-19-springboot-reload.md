# R4 SpringBoot 快速重启（替换 Jar 并重启）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 SpringBoot 应用新增一键「替换并重启」能力——停（若运行中）→ 换包 → 启动，运行中和停止态均可操作，同时保留原「替换 Jar」（运行中禁止、只换包不启动）语义。

**Architecture:** 后端在 `commands/springboot.rs` 提取纯文件函数 `replace_jar_file`（可单测），重构现有 `replace_springboot_jar` 复用之；新增命令 `replace_springboot_jar_and_restart` 组合 lifecycle 的 stop_app/start_app。前端 AppCard 新增「替换并重启」按钮，SpringBootPage 新增 handler，store 新增 action，结果用全局 toast 呈现。

**Tech Stack:** Rust (Tauri v2 command + lifecycle)、Vue 3 `<script setup>` + Pinia store + vue-i18n、`@tauri-apps/plugin-dialog`。

## Global Constraints

- 分支：开发必须在 `feat/roadmap2-r4`（当前分支）上进行，禁止改 master。
- 提交规范：不伪造 Co-Authored-By trailers；用本机 git 用户名邮箱。
- 原「替换 Jar」语义保留：Running 时禁止换包，停止时只换包不启动。
- 操作结果反馈统一用全局 toast（`@/composables/useToast`），不用页面 banner。
- `replace_jar_file` 为纯文件函数，依赖 SpringBootManager，可直接用临时目录单测。
- 新增 i18n key 必须中/英双语（`zh-CN.ts` / `en-US.ts`）。
- 验证命令：`cargo test --lib`（Rust）+ `npx vue-tsc --noEmit`（前端类型）。

---

### Task 1: 后端 —— 提取 `replace_jar_file` 纯函数并写单测

**Files:**
- Modify: `src-tauri/src/commands/springboot.rs`（新增函数 + 导入）
- Test: 同文件底部 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `fn replace_jar_file(old_jar: &Path, new_jar: &Path) -> anyhow::Result<(PathBuf, String)>` —— 校验存在 → 备份到 `{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak` → 复制新 jar 覆盖旧 → 读新版本（失败回退 `"unknown"`）→ 返回 `(backup_path, new_version)`。

- [ ] **Step 1: 在 `commands/springboot.rs` 添加函数**

在 `replace_springboot_jar` 命令函数之后（紧邻，line ~152 之后）、`get_springboot_jvm_metrics` 之前加入：

```rust
/// 纯文件操作：校验新旧 jar → 备份旧 jar → 复制新 jar 覆盖 → 读新版本。
/// 不依赖 SpringBootManager，可直接单测。返回 (backup_path, new_version)。
fn replace_jar_file(app_name: &str, old_jar: &Path, new_jar: &Path) -> anyhow::Result<(std::path::PathBuf, String)> {
    use chrono::Local;

    if !new_jar.exists() {
        anyhow::bail!("新 JAR 文件不存在");
    }
    if !old_jar.exists() {
        anyhow::bail!("原 JAR 文件不存在");
    }

    // 备份：{data_dir}/backups/{app_name}/{jar}.{timestamp}.bak
    let backup_dir = crate::utils::paths::data_dir().join("backups").join(app_name);
    std::fs::create_dir_all(&backup_dir).map_err(|e| anyhow::anyhow!("创建备份目录失败: {}", e))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S");
    let fname = old_jar.file_name().unwrap_or_default();
    let backup_path = backup_dir.join(format!("{}.{}.bak", fname.to_string_lossy(), timestamp));

    std::fs::copy(old_jar, &backup_path).map_err(|e| anyhow::anyhow!("备份失败: {}", e))?;
    std::fs::copy(new_jar, old_jar).map_err(|e| anyhow::anyhow!("替换 JAR 失败: {}", e))?;

    let new_version = crate::services::springboot_manager::read_jar_version(new_jar.to_str().unwrap_or(""))
        .unwrap_or_else(|| "unknown".to_string());
    Ok((backup_path, new_version))
}
```

- [ ] **Step 2: 在文件底部添加 `#[cfg(test)]` 测试模块**

在 `copy_dir_all` 函数结束后（文件末尾）追加：

```rust
#[cfg(test)]
mod tests {
    use super::replace_jar_file;
    use std::io::Write;

    fn fake_jar(path: &std::path::Path, manifest: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let opts = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn jar_has_version(path: &std::path::Path) -> Option<String> {
        crate::services::springboot_manager::read_jar_version(path.to_str().unwrap())
    }

    #[test]
    fn replace_jar_file_backs_up_and_overwrites() {
        let dir = std::env::temp_dir().join(format!("opx_repl_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let old = dir.join("app.jar");
        let new = dir.join("new.jar");
        fake_jar(&old, "Implementation-Version: 1.0.0\r\n");
        fake_jar(&new, "Implementation-Version: 2.0.0\r\n");

        let (backup, version) = replace_jar_file("test-app", &old, &new).unwrap();

        assert_eq!(version, "2.0.0");
        assert_eq!(jar_has_version(&old), Some("2.0.0".to_string()));
        // 备份是一份旧的 1.0.0 且文件名以 .bak 结尾
        assert!(backup.extension().map(|e| e == "bak").unwrap_or(false));
        assert!(backup.exists());
        assert_eq!(jar_has_version(&backup), Some("1.0.0".to_string()));
        // 旧 jar 已不是原文件（内容被覆盖）
        assert_ne!(std::fs::read(&old).unwrap(), std::fs::read(&backup).unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replace_jar_file_rejects_missing_files() {
        let dir = std::env::temp_dir().join(format!("opx_repl_err_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let old = dir.join("app.jar");
        let new = dir.join("new.jar");
        fake_jar(&old, "Implementation-Version: 1.0.0\r\n");
        // new 不存在
        assert!(replace_jar_file("t", &old, &new).is_err());

        // old 不存在
        let old2 = dir.join("missing.jar");
        assert!(replace_jar_file("t", &old2, &new).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```

- [ ] **Step 3: 运行测试验证失败/通过**

Run: `cargo test --lib replace_jar_file` （从 `src-tauri/`）
Expected: 新测试通过（提取的函数已就位）。若失败，检查 `anyhow` 是否已引入依赖（`ctxt` 见下）。

> 注：若 `cargo test` 报 `anyhow` 未找到宏，说明该 crate 测试目标未引 anyhow —— 但 `commands/springboot.rs` 上层已用 `anyhow::anyhow!`（见 lifecycle import），正常可用。测试通过即视为成功。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/springboot.rs
git commit -m "feat(springboot): 提取 replace_jar_file 纯函数并添加单测"
```

---

### Task 2: 后端 —— 重构 `replace_springboot_jar` + 新增 `replace_springboot_jar_and_restart` 命令

**Files:**
- Modify: `src-tauri/src/commands/springboot.rs:102-152`（重构 replace_springboot_jar，新增命令）
- Modify: `src-tauri/src/lib.rs:245`（注册新命令）

**Interfaces:**
- Consumes: `replace_jar_file(app_name, old_jar, new_jar)`（Task 1）、`lifecycle::stop_app` / `lifecycle::start_app`、`manager.find_app` / `manager.update_version`。
- Produces: `#[tauri::command] pub async fn replace_springboot_jar_and_restart(manager, software_mgr, app_handle, id: String, new_jar_path: String) -> Result<ReplaceResult, String>`。

- [ ] **Step 1: 重构 `replace_springboot_jar` 使用共享函数**

将现有 `replace_springboot_jar`（line 102-152）主体替换为：

```rust
#[tauri::command]
pub async fn replace_springboot_jar(
    manager: State<'_, Arc<SpringBootManager>>,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    oplog!("springboot_replace_jar", &format!("{} ({})", app.name, id));
    if app.status == crate::models::springboot::AppStatus::Running {
        return Err("运行中的应用不可换包".to_string());
    }

    let old_jar = std::path::PathBuf::from(&app.jar_path);
    let new_jar = std::path::Path::new(&new_jar_path);
    let (backup_path, new_version) =
        replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;

    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;
    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap_or("").to_string(),
        old_version: app.version,
        new_version,
    })
}
```

- [ ] **Step 2: 新增 `replace_springboot_jar_and_restart` 命令**

紧随上面重构后的命令函数之后添加：

```rust
#[tauri::command]
pub async fn replace_springboot_jar_and_restart(
    manager: State<'_, Arc<SpringBootManager>>,
    software_mgr: State<'_, Arc<SoftwareManager>>,
    app_handle: AppHandle,
    id: String,
    new_jar_path: String,
) -> Result<ReplaceResult, String> {
    use crate::models::springboot::AppStatus;

    let app = manager.find_app(&id).map_err(|e| e.to_string())?;
    oplog!("springboot_replace_restart", &format!("{} ({})", app.name, id));

    // 运行中/错误态先停（优雅），停止态直接换包
    if matches!(app.status, AppStatus::Running | AppStatus::Error) {
        crate::services::springboot_manager::lifecycle::stop_app(
            &id, &manager, &software_mgr, &app_handle,
        ).await?;
    }

    let old_jar = std::path::PathBuf::from(&app.jar_path);
    let new_jar = std::path::Path::new(&new_jar_path);
    let (backup_path, new_version) =
        replace_jar_file(&app.name, &old_jar, &new_jar).map_err(|e| e.to_string())?;

    manager.update_version(&id, new_version.clone()).map_err(|e| e.to_string())?;

    crate::services::springboot_manager::lifecycle::start_app(
        &id, &manager, &software_mgr, &app_handle,
    ).await?;

    Ok(ReplaceResult {
        backup_path: backup_path.to_str().unwrap_or("").to_string(),
        old_version: app.version,
        new_version,
    })
}
```

> 注意：`app` 是 `find_app` 返回的克隆快照，`old_version` 在 stop/换包后取 `app.version` 是被引用的原版本字段（字段是 String，克隆后独立），正确。state 参数 `manager`/`software_mgr` 为 `State<'_, Arc<_>>`，传给 lifecycle 的 `&manager`/`&software_mgr` 自动 deref 为 `&SpringBootManager`（与现有 start/stop 命令一致）。

- [ ] **Step 3: 注册命令到 `lib.rs`**

在 `src-tauri/src/lib.rs` line 245（`commands::springboot::replace_springboot_jar,`）之后加入：

```rust
commands::springboot::replace_springboot_jar_and_restart,
```

- [ ] **Step 4: 编译验证**

Run: `cargo build` （从 `src-tauri/`，先跑通全量编译）
Expected: 编译通过，无错误。

- [ ] **Step 5: 运行全部测试**

Run: `cargo test --lib`
Expected: Task 1 单测 + 既有测试全绿。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/springboot.rs src-tauri/src/lib.rs
git commit -m "feat(springboot): 新增 replace_springboot_jar_and_restart 命令"
```

---

### Task 3: 前端 —— store action + AppCard 按钮 + Page handler + i18n

**Files:**
- Modify: `src/modules/springboot-manager/stores/springboot.ts`（新增 action）
- Modify: `src/modules/springboot-manager/components/AppCard.vue`（新增按钮 + emit）
- Modify: `src/modules/springboot-manager/pages/SpringBootPage.vue`（新增 handler + 绑定）
- Modify: `src/locales/zh-CN.ts` / `src/locales/en-US.ts`（新增 key）

**Interfaces:**
- Consumes: `invoke('replace_springboot_jar_and_restart', { id, newJarPath })`、`toast(message, kind)`、`open()` from `@tauri-apps/plugin-dialog`。
- Produces: store `replaceJarAndRestart(id, jarPath): Promise<ReplaceResult>`；AppCard `@replaceRestart` emit；Page `handleReplaceRestart(id)`。

- [ ] **Step 1: store 新增 action**

在 `src/modules/springboot-manager/stores/springboot.ts` 的 `replaceJar` 之后（line 59 后）添加：

```ts
  async function replaceJarAndRestart(id: string, newJarPath: string): Promise<ReplaceResult> {
    return await invoke<ReplaceResult>('replace_springboot_jar_and_restart', { id, newJarPath })
  }
```

并在 `return { ... }`（line 94-101）中加入 `replaceJarAndRestart`：

```ts
    startApp, stopApp, restartApp, replaceJar, replaceJarAndRestart,
```

- [ ] **Step 2: AppCard 新增「替换并重启」按钮**

在 `AppCard.vue` 的 replace 按钮之后（line 62 后）新增按钮，并添加 emit。模板：

```html
      <button
        class="btn"
        :disabled="app.status === AppStatus.Starting || app.status === AppStatus.Stopping"
        @click="$emit('replaceRestart', app.id)"
      >
        <Icon icon="mdi:package-up" /> {{ $t('replaceAndRestart') }}
      </button>
```

`defineEmits`（line 97-106）追加：

```ts
  replaceRestart: [id: string]
```

- [ ] **Step 3: Page 新增 handler 并绑定**

`SpringBootPage.vue`：
1. 模板中 AppCard 绑定处（line 90-102）加一行：

```html
        @replaceRestart="handleReplaceRestart"
```

2. script 导入 `toast`（在顶部 import 区，`@/components/...` 附近）加：

```ts
import { toast } from '@/composables/useToast'
```

3. 在 `handleReplace`（line 375-384）之后新增：

```ts
async function handleReplaceRestart(id: string) {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'JAR', extensions: ['jar'] }],
  })
  if (selected && typeof selected === 'string') {
    try {
      await store.replaceJarAndRestart(id, selected)
      toast(t('replaceRestartSuccess'), 'ok')
    } catch (e) {
      toast(String(e), 'err')
    }
    await store.fetchApps()
  }
}
```

- [ ] **Step 4: i18n 新增 key**

`src/locales/zh-CN.ts` 在 `replaceJar: '换包',`（line 249）后追加：

```ts
  replaceAndRestart: '替换并重启',
  replaceRestartSuccess: '替换并重启成功',
```

`src/locales/en-US.ts` 在 `replaceJar: 'Replace JAR',`（line 249）后追加：

```ts
  replaceAndRestart: 'Replace & Restart',
  replaceRestartSuccess: 'Replaced and restarted',
```

> 若 en-US.ts 中 `replaceJar` 相邻行有冲突，用 Grep 定位后在其后插入即可（`replaceAndRestart` / `replaceRestartSuccess` 两个新 key 保证唯一）。

- [ ] **Step 5: 前端类型检查 + 构建**

Run: `npx vue-tsc --noEmit`
Expected: 无类型错误。

- [ ] **Step 6: 全量测试确认**

Run: `cargo test --lib` （从 `src-tauri/`，确认后端仍绿）
Expected: 全绿。

- [ ] **Step 7: Commit**

```bash
git add src/modules/springboot-manager/stores/springboot.ts src/modules/springboot-manager/components/AppCard.vue src/modules/springboot-manager/pages/SpringBootPage.vue src/locales/zh-CN.ts src/locales/en-US.ts
git commit -m "feat(springboot): 前端新增替换并重启按钮与处理"
```

---

## Self-Review 记录

- **Spec 覆盖:** 后端提取函数(T1)、重构原本(T2)、新命令(T2)、lib.rs 注册(T2)、store action(T3)、AppCard 按钮(T3)、Page handler(T3)、i18n(T3)、toast 结果(T3)、单测(T1)、验收命令(T2/T3) —— 全覆盖。
- **占位符扫描:** 所有步骤含具体代码与命令，无 TBD/TODO。
- **类型一致性:** `replace_jar_file(app_name: &str, old_jar: &Path, new_jar: &Path) -> anyhow::Result<(PathBuf, String)>`；store `replaceJarAndRestart(id, jarPath)`；emit `replaceRestart`；handler `handleReplaceRestart(id)` —— 三处名字一致，与 spec 一致。
- **边界:** 运行中禁止换包仅在原 `replace_springboot_jar` 保留；新命令运行中自动 stop（走 `stop_app` 优雅停止）。启动失败时 jar 已替换、备份已生成——符合 spec 边界。
