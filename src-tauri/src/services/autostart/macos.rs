//! macOS：LaunchAgent（`~/Library/LaunchAgents/com.opx.app.plist`）。
//!
//! 用户级代理（`gui/<uid>` 域），**无需 root**；`RunAtLoad=true` 使登录后自动启动。
//! 不设 `KeepAlive`：自启只需启动一次，崩溃后自动重启会与 opx 自身的看门狗重复。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::Autostart;

/// LaunchAgent 标签。
pub const LABEL: &str = "com.opx.app";
/// 代理目录（相对 `$HOME`）。
pub const AGENTS_REL_DIR: &str = "Library/LaunchAgents";

/// 纯函数：plist 路径。
pub fn plist_path(home: &Path) -> PathBuf {
    home.join(AGENTS_REL_DIR).join(format!("{LABEL}.plist"))
}

/// 纯函数：XML 文本转义。
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 纯函数：生成 LaunchAgent plist 文本（可跨平台单测）。
pub fn plist_text(exe: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20   <key>Label</key>\n\
         \x20   <string>{}</string>\n\
         \x20   <key>ProgramArguments</key>\n\
         \x20   <array>\n\
         \x20       <string>{}</string>\n\
         \x20   </array>\n\
         \x20   <key>RunAtLoad</key>\n\
         \x20   <true />\n\
         </dict>\n\
         </plist>\n",
        xml_escape(LABEL),
        xml_escape(&exe.to_string_lossy())
    )
}

/// macOS LaunchAgent 自启后端。
pub struct MacosAutostart;

impl MacosAutostart {
    fn path() -> Result<PathBuf> {
        let home = super::home_dir().ok_or_else(|| anyhow!("无法确定用户主目录（$HOME）"))?;
        Ok(plist_path(&home))
    }

    /// 当前用户 uid（`gui/<uid>` 域）。
    fn current_uid() -> Result<String> {
        let out = opx_core::utils::process::hidden("id")
            .arg("-u")
            .output()
            .context("执行 id -u 失败")?;
        if !out.status.success() {
            anyhow::bail!("id -u 失败");
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Autostart for MacosAutostart {
    fn platform(&self) -> &'static str {
        "launchd"
    }
    fn is_supported(&self) -> bool {
        true
    }
    fn is_enabled(&self) -> bool {
        Self::path().map(|p| p.exists()).unwrap_or(false)
    }
    fn enable(&self, exe: &Path) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建 LaunchAgents 目录失败: {}", parent.display()))?;
        }
        std::fs::write(&path, plist_text(exe))
            .with_context(|| format!("写入 plist 失败: {}", path.display()))?;
        // 立即加载（已加载时会失败，忽略——下次登录仍会按 plist 自动启动）
        if let Ok(uid) = Self::current_uid() {
            let _ = opx_core::utils::process::hidden("launchctl")
                .args(["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()])
                .output();
        }
        Ok(())
    }
    fn disable(&self) -> Result<()> {
        let path = Self::path()?;
        if let Ok(uid) = Self::current_uid() {
            let _ = opx_core::utils::process::hidden("launchctl")
                .args(["bootout", &format!("gui/{uid}/{LABEL}")])
                .output();
        }
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("删除 plist 失败: {}", path.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_text_has_label_and_runatload() {
        let text = plist_text(Path::new("/Applications/opx.app/Contents/MacOS/opx"));
        assert!(text.starts_with("<?xml version=\"1.0\""));
        assert!(text.contains("<string>com.opx.app</string>"));
        assert!(text.contains("<string>/Applications/opx.app/Contents/MacOS/opx</string>"));
        assert!(text.contains("<key>RunAtLoad</key>"));
        assert!(text.contains("<true />"));
        assert!(text.ends_with("</plist>\n"));
    }

    #[test]
    fn plist_text_escapes_xml() {
        let text = plist_text(Path::new("/opt/a&b/<x>"));
        assert!(text.contains("/opt/a&amp;b/&lt;x&gt;"), "{text}");
    }

    #[test]
    fn plist_path_layout() {
        let p = plist_path(Path::new("/Users/u"));
        assert!(p.ends_with("Library/LaunchAgents/com.opx.app.plist"), "{}", p.display());
    }
}
