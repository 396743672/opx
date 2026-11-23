//! 可执行文件定位：把「安装根目录 + 程序名」解析成真实路径。
//!
//! 为什么需要单独一层：同一个发行包在不同平台/不同版本的目录层级并不一致，
//! 而各模块各自 `root.join("bin").join("java")` 这种硬编码必然在某一侧失效：
//!
//! - **Windows** Node 官方 zip 解出 `node.exe` 直接在根，JDK 是 `<root>/bin/java.exe`；
//! - **Linux** Node 官方 tar.gz 解出 `bin/node`，JDK 是 `<root>/bin/java`；
//! - **历史安装** 官方包自带一层顶层目录（`jdk-25.0.5.1+2/`、`node-v22-linux-x64/`），
//!   早期 tar 解压没有剥层，装出来就是 `<root>/jdk-25.0.5.1+2/bin/java`。
//!
//! 统一按「根 → 根/bin → 一层子目录 → 一层子目录/bin」顺序找，命中即返回；
//! 找不到返回 `None`，由调用方给用户可读的报错（而不是启动阶段 `os error 2`）。

use std::path::{Path, PathBuf};

/// 按平台补 `.exe` 后缀（`name` 传入不带后缀的名字，如 `java` / `node`）。
fn with_platform_ext(dir: &Path, name: &str) -> PathBuf {
    let plain = dir.join(name);
    if cfg!(windows) {
        let exe = plain.with_extension("exe");
        if exe.exists() {
            return exe;
        }
    }
    plain
}

/// 在 `root` 下定位名为 `name` 的可执行文件。
///
/// 查找顺序（命中即返回）：
/// 1. `<root>/<name>` —— Windows 的 node.exe、单文件二进制（minio 等）
/// 2. `<root>/bin/<name>` —— Linux/macOS 的 node、JDK 的 java
/// 3. `<root>/<sub>/<name>`、`<root>/<sub>/bin/<name>` —— 历史安装多剥一层的存量目录
///
/// 只往下扫**一层**子目录：再深就是误判（例如 JDK 里 `lib/` 下也有同名文件），
/// 且一层已覆盖官方包「顶层目录 + 内容」的全部形状。
pub fn resolve_bin(root: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    for direct in [with_platform_ext(root, name), with_platform_ext(&root.join("bin"), name)] {
        if direct.is_file() {
            return Some(direct);
        }
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        // 用 file_type() 不跟随软链，避免 JDK 里指向自身的链接把扫描带偏
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let sub = entry.path();
        for candidate in [with_platform_ext(&sub, name), with_platform_ext(&sub.join("bin"), name)] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 定位到可执行文件后返回其「home」（即 `JAVA_HOME` 那层目录）。
///
/// `<root>/bin/java` → `<root>`（剥掉 `bin`）；`<root>/java`（Windows 直放）→ `<root>`。
/// 找不到返回 `None`。
pub fn resolve_home(root: &Path, name: &str) -> Option<PathBuf> {
    let bin = resolve_bin(root, name)?;
    let parent = bin.parent()?;
    // `bin/` 目录本身不是 home：JAVA_HOME 必须是「含 bin 的那一层」
    if parent.file_name().map(|n| n == "bin").unwrap_or(false) {
        return parent.parent().map(|p| p.to_path_buf());
    }
    Some(parent.to_path_buf())
}

/// 确保可执行位存在（Unix 生效；Windows 只判存在性）。
///
/// 为什么需要：历史上解压 tar 时用 `File::create` 落盘，权限一律 0644，
/// 装出来的 `bin/java`、`bin/node` 没有执行位，启动报 `os error 13（权限不够）`。
/// 安装侧已修（解包恢复归档权限），这里兜住**存量装坏的实例**——就地补 x 位
/// 比让用户「卸载重装一遍」代价小得多。
///
/// 返回最终是否具备执行条件（文件不存在返回 false）。
pub fn ensure_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        if mode & 0o111 == 0 {
            // 只补 x 位，不动读写位（0644 → 0755），避免把文件提权成可写
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o111));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{resolve_bin, resolve_home};

    /// 建一个临时目录树，返回根路径（调用方负责清理）。
    fn fixture(tag: &str, layout: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("opx-exec-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for rel in layout {
            let p = root.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, b"#!/bin/sh\n").unwrap();
        }
        root
    }

    fn exec_name() -> &'static str {
        if cfg!(windows) {
            "java.exe"
        } else {
            "java"
        }
    }

    #[test]
    fn finds_bin_layout_and_root_layout() {
        // Linux/macOS 风格：<root>/bin/<name>
        let root = fixture("bin", &[&format!("bin/{}", exec_name())]);
        assert_eq!(
            resolve_bin(&root, exec_name()).unwrap(),
            root.join("bin").join(exec_name())
        );
        let _ = std::fs::remove_dir_all(&root);

        // Windows 风格：<root>/<name>
        let root = fixture("root", &[exec_name()]);
        assert_eq!(resolve_bin(&root, exec_name()).unwrap(), root.join(exec_name()));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 存量安装多剥一层（`<root>/jdk-25/bin/java`）也要能找到——
    /// 这正是应用卡片报「JDK 的 java 不存在」的那种目录。
    #[test]
    fn finds_nested_single_root_layout() {
        let nested = format!("jdk-25/bin/{}", exec_name());
        let root = fixture("nested", &[&nested]);
        assert_eq!(
            resolve_bin(&root, exec_name()).unwrap(),
            root.join("jdk-25").join("bin").join(exec_name())
        );
        // home 应是那个可执行文件所在目录的上一级
        assert_eq!(
            resolve_home(&root, exec_name()).unwrap(),
            root.join("jdk-25")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_binary_returns_none() {
        let root = fixture("missing", &["README.txt"]);
        assert!(resolve_bin(&root, exec_name()).is_none());
        assert!(resolve_bin(&root, "").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 目录名恰好等于程序名时必须忽略：`is_file()` 才算命中，
    /// 否则会把 `<root>/node/` 这种目录误报成可执行文件。
    #[test]
    fn directory_named_like_binary_is_not_a_hit() {
        let root = fixture("dirhit", &[&format!("{}/keep.txt", exec_name())]);
        assert!(resolve_bin(&root, exec_name()).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 存量安装里 0644 的 `bin/java` 要能就地修好执行位（`os error 13` 的根因）。
    #[cfg(unix)]
    #[test]
    fn ensure_executable_repairs_missing_exec_bit() {
        use std::os::unix::fs::PermissionsExt;

        let root = fixture("chmod", &[&format!("bin/{}", exec_name())]);
        let bin = root.join("bin").join(exec_name());
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(super::ensure_executable(&bin), "文件存在即应返回 true");
        let mode = std::fs::metadata(&bin).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o111, 0o111, "应补上 x 位");
        assert_eq!(mode, 0o755, "读写位保持不变（0644 | 0111）");
        // 不存在的路径不该 panic
        assert!(!super::ensure_executable(&root.join("nope")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
