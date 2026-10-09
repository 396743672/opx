//! 设置（`settings.json`）的读取。
//!
//! 原在壳层 `opx::commands::config::read_settings`。该函数是**纯文件读取 + JSON
//! 反序列化**，不依赖 Tauri / `AppHandle` / `State`，故下沉到 core 供两个壳共用，
//! 也让`services` 层（即将搬入 core）能直接读取设置而不必反向依赖壳层 commands。
//!
//! ⚠️ **只搬读取，不搬写入**：壳层 `save_settings` 除了落盘，还带
//! `set_global_proxy` + `init_download_config` 两处**进程级副作用**（刷新全局
//! HTTP 代理与下载器配置），属宿主运行时状态而非纯落盘逻辑，故整体留在壳层。

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