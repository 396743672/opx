//! 访问令牌：生成（CSPRNG）+ 常量时间校验 + 受限写盘持久化。
//!
//! 设计依据：`docs/2026-10-10-opx-http-design.md` §3（D3，落实 ADR B2）：
//! - 32 字节 CSPRNG → hex（64 字符），`OsRng` 直取（与 opx-core 同源的
//!   `rand_core`+`getrandom`，零新增传递依赖）
//! - `subtle` 常量时间比较，防时序侧信道
//! - 落盘走 `opx_core::utils::paths::write_file_restricted` —— 项目铁律：
//!   **敏感文件统一受限写盘，禁止先 write 再单独 restrict**（窗口期内任意
//!   本机进程可读）。4.2 批次 token 将迁入 settings.json 的 `web_token`
//!   字段（同走受限写盘），届时本模块的文件持久化由 settings 取代。

use opx_core::utils::paths;
use rand_core::RngCore;
use subtle::ConstantTimeEq;

/// token 字节数（hex 后 64 字符）。ADR B2 已定 32 字节 CSPRNG。
const TOKEN_BYTES: usize = 32;

/// token 持久化文件名（config_dir 下）。⚠️ 4.2 起 token 迁入 settings.json，
/// 本文件仅供 4.0 骨架期与测试使用；迁移时删除文件并保留 settings 字段。
const TOKEN_FILE: &str = "web-token";

/// 生成一个新 token（32 字节 CSPRNG → 64 字符小写 hex）。
pub fn generate() -> String {
    let mut buf = [0u8; TOKEN_BYTES];
    // OsRng 是操作系统级 CSPRNG（getrandom），非密码学场景不得用它 —— 本处是。
    rand_core::OsRng.fill_bytes(&mut buf);
    hex_encode(&buf)
}

/// 常量时间比较：`presented`（请求携带）是否等于 `expected`（服务端保存）。
///
/// 长度不等同样返回 false，且比较耗时与内容无关（`subtle::ConstantTimeEq`
/// 对 `[u8]` 的实现对长度不等也走常量时间路径）。
pub fn verify(presented: &str, expected: &str) -> bool {
    bool::from(presented.as_bytes().ct_eq(expected.as_bytes()))
}

/// token 持久化路径（config_dir/web-token）。
fn token_file() -> std::path::PathBuf {
    paths::config_dir().join(TOKEN_FILE)
}

/// 受限写盘保存 token（铁律：统一走 `write_file_restricted`）。
///
/// 返回 `Err` 的场景：config_dir 不可创建（磁盘/权限）。调用方（设置页 /
/// headless 横幅）应把错误显式暴露给用户，而不是静默降级为无鉴权。
pub fn store(token: &str) -> std::io::Result<()> {
    let path = token_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    paths::write_file_restricted(&path, token)
}

/// 读取已持久化的 token；文件不存在（首次启动）返回 `None`。
pub fn load() -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(token_file()) {
        Ok(s) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                Ok(Some(s.to_string()))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 读取或首次生成：文件里有就用（保持 token 稳定，ADR「手动重置才轮换」），
/// 没有就生成并落盘。
pub fn load_or_generate() -> std::io::Result<String> {
    if let Some(t) = load()? {
        return Ok(t);
    }
    let t = generate();
    store(&t)?;
    Ok(t)
}

/// 手写 hex 编码（2 行核心逻辑，不值得为此引入 hex crate 依赖）。
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_64_hex_chars_and_unique() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), 64);
        assert_eq!(b.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, b, "CSPRNG 两次生成不应相同");
    }

    #[test]
    fn verify_matches_and_rejects() {
        let t = generate();
        assert!(verify(&t, &t));
        assert!(!verify(&generate(), &t), "不同 token 必须拒绝");
        // 长度不等（截断/拼接攻击面）也要拒绝
        assert!(!verify(&t[..63], &t));
        assert!(!verify(&(t.clone() + "0"), &t));
        assert!(!verify("", &t));
    }
}
