//! 设置（`settings.json`）的读取。
//!
//! 原在壳层 `opx::commands::config::read_settings`。该函数是**纯文件读取 + JSON
//! 反序列化**，不依赖 Tauri / `AppHandle` / `State`，故下沉到 core 供两个壳共用，
//! 也让`services` 层（即将搬入 core）能直接读取设置而不必反向依赖壳层 commands。
//!
//! ⚠️ **读取在此，写入也在此，但只做「纯落盘」**：壳层 `save_settings` 除了落盘，
//! 还带 `set_global_proxy` + `init_download_config` 两处**进程级副作用**（刷新全局
//! HTTP 代理与下载器配置），属宿主运行时状态而非纯落盘逻辑，故带副作用的保存整体
//! 留在壳层；本模块提供无副作用的原子写（`write_settings_file*`）与首启种子
//! （`seed_settings_if_absent*`），供 supervisor 热修单字段与首启建档使用。

use crate::models::settings::AppSettings;
use crate::utils::paths;

/// 读取 settings.json；文件缺失或内容损坏时返回默认设置（并记录告警）。
///
/// 失败**不返回 Err**：设置损坏属于可降级场景（用默认值继续跑），故本函数实际
/// 只有 `Ok` 分支。保留 `Result` 是为了与既有 5 处调用方的 `.unwrap_or_default()`
/// / `?` 写法兼容，避免搬迁时改动调用点。
///
/// 路径相对 exe 所在目录（便携布局），见 [`paths::settings_path`]。
pub fn read_settings() -> Result<AppSettings, String> {
    let path = paths::settings_path();
    let Ok(s) = std::fs::read_to_string(&path) else {
        return Ok(Default::default());
    };
    match serde_json::from_str(&s) {
        Ok(v) => Ok(v),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "settings.json 解析失败，使用默认设置");
            Ok(Default::default())
        }
    }
}

/// 原子写 settings.json 到指定路径（tmp + rename，与壳层 `save_settings` 同一
/// 落盘形状），**不带**壳层 save 的进程级副作用（全局代理 / 下载器配置）——
/// 供 supervisor 热修单字段、首启种子等纯落盘场景使用。
pub fn write_settings_file_at(path: &std::path::Path, settings: &AppSettings) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

/// 原子写 settings.json（exe 同级便携布局路径）。
pub fn write_settings_file(settings: &AppSettings) -> std::io::Result<()> {
    write_settings_file_at(&paths::settings_path(), settings)
}

/// 仅当 settings.json **不存在**时创建种子文件（返回 true）；已存在一律不动
/// （返回 false）——升级 / 已有安装的配置（mirror、token、语言等）绝不被覆盖。
///
/// 供壳层首启建档使用：headless 首启写 `web_lan_access=true`（安装引导设计 §3）、
/// 新装写 `first_run_done=Some(false)` 进入引导。⚠️ 调用方必须用「文件不存在」
/// 判定，而不是「解析失败用默认值」——后者会覆盖损坏文件。
pub fn seed_settings_if_absent_at(
    path: &std::path::Path,
    seed: impl FnOnce(&mut AppSettings),
) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    let mut s = AppSettings::default();
    seed(&mut s);
    write_settings_file_at(path, &s)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> std::path::PathBuf {
        // 进程内唯一名，避免并行测试互踩（不引 tempfile 依赖）
        let d = std::env::temp_dir();
        let p = d.join(format!(
            "opx-test-settings-{}-{tag}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn seed_writes_only_when_missing() {
        let p = tmp_path("seed-missing");
        // 首次：不存在 → 写入并应用 seed
        let written = seed_settings_if_absent_at(&p, |s| {
            s.web_lan_access = true;
            s.first_run_done = Some(false);
        })
        .expect("seed should succeed");
        assert!(written);
        let s: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(s.web_lan_access, "种子应写 web_lan_access=true");
        assert_eq!(s.first_run_done, Some(false));
        assert_eq!(s.web_port, crate::models::settings::default_web_port());
        // 二次：已存在 → 不动
        let written2 = seed_settings_if_absent_at(&p, |s| s.web_lan_access = false)
            .expect("second seed should succeed");
        assert!(!written2, "已存在时绝不覆盖");
        let s2: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(s2.web_lan_access, "既有内容保持不变");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn write_roundtrip_preserves_fields() {
        let p = tmp_path("write-roundtrip");
        let mut s = AppSettings::default();
        s.web_port = 17581;
        s.web_port_auto = true;
        s.first_run_done = Some(true);
        write_settings_file_at(&p, &s).expect("write should succeed");
        let back: AppSettings =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(back.web_port, 17581);
        assert!(back.web_port_auto);
        assert_eq!(back.first_run_done, Some(true));
        let _ = std::fs::remove_file(&p);
    }
}