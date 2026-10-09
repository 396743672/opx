//! Linux：XDG autostart（`~/.config/autostart/opx.desktop`）。
//!
//! 选 XDG 而非 `systemd --user`：opx 是 **GUI 桌面应用**，桌面会话登录时由桌面环境
//! 读取 autostart 目录启动，天然继承 `DISPLAY`/`WAYLAND_DISPLAY`/`DBUS_SESSION_BUS_ADDRESS`
//! 等会话环境；`systemd --user` 启动 GUI 需手工注入这些变量，且与桌面会话绑定更弱。

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::Autostart;

/// `.desktop` 文件名。
pub const DESKTOP_FILE: &str = "opx.desktop";
/// autostart 目录（相对 `$HOME`）。
pub const AUTOSTART_REL_DIR: &str = ".config/autostart";

/// 纯函数：`.desktop` 路径。
pub fn desktop_path(home: &Path) -> PathBuf {
    home.join(AUTOSTART_REL_DIR).join(DESKTOP_FILE)
}

/// 纯函数：`Exec` 值的转义（Desktop Entry 规范：引号包裹 + 转义 `\` `"`，并转义
/// 有特殊含义的 `$` 与反引号，避免被当 field code / 命令替换）。
pub fn exec_escape(p: &Path) -> String {
    let esc = p
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{esc}\"")
}

/// 纯函数：生成 `.desktop` 文本（可跨平台单测）。
pub fn desktop_text(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=OPX\n\
         Comment=Software manager for local services\n\
         Exec={}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n",
        exec_escape(exe)
    )
}

/// Linux XDG 自启后端。
pub struct LinuxAutostart;

impl LinuxAutostart {
    fn path() -> Result<PathBuf> {
        let home = super::home_dir().ok_or_else(|| anyhow!("无法确定用户主目录（$HOME）"))?;
        Ok(desktop_path(&home))
    }
}

impl Autostart for LinuxAutostart {
    fn platform(&self) -> &'static str {
        "xdg"
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
                .with_context(|| format!("创建 autostart 目录失败: {}", parent.display()))?;
        }
        std::fs::write(&path, desktop_text(exe))
            .with_context(|| format!("写入 .desktop 失败: {}", path.display()))?;
        Ok(())
    }
    fn disable(&self) -> Result<()> {
        let path = Self::path()?;
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("删除 .desktop 失败: {}", path.display()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_text_is_valid_entry() {
        let text = desktop_text(Path::new("/opt/opx/opx"));
        assert!(text.starts_with("[Desktop Entry]\n"));
        assert!(text.contains("Type=Application"));
        assert!(text.contains("Exec=\"/opt/opx/opx\""));
        assert!(text.contains("X-GNOME-Autostart-enabled=true"));
        // 除节头（[Desktop Entry]）外，每行必须是 键=值，否则桌面环境会忽略该文件
        for line in text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('['))
        {
            assert!(line.contains('='), "非法行: {line}");
        }
    }

    #[test]
    fn exec_escape_quotes_and_escapes_specials() {
        assert_eq!(exec_escape(Path::new("/opt/a b/opx")), "\"/opt/a b/opx\"");
        assert_eq!(
            exec_escape(Path::new("/opt/we\"ird")),
            "\"/opt/we\\\"ird\""
        );
        assert_eq!(exec_escape(Path::new("/opt/$HOME/x")), "\"/opt/\\$HOME/x\"");
    }

    #[test]
    fn desktop_path_layout() {
        let p = desktop_path(Path::new("/home/u"));
        assert!(p.ends_with(".config/autostart/opx.desktop"), "{}", p.display());
    }
}
