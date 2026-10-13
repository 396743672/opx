//! 访问令牌：生成（CSPRNG）+ 常量时间校验 + 受限写盘持久化。
//!
//! 设计依据：`docs/2026-10-10-opx-http-design.md` §3（D3，落实 ADR B2）+ 批次 4.6：
//! - **新 token 格式：10 位 Crockford Base32**（批次 4.6 用户裁定）。字符集
//!   `0-9A-HJKMNP-TV-Z`（去 I/L/O/U 防混淆），≈50 bit 熵。
//!   **安全权衡（已裁定，不做失败限速）**：在线爆破每次猜测消耗一个完整 HTTP
//!   请求，2^50 次即使 1000 req/s 也需数万年；场景为内网 + token 唯一门槛，
//!   50 bit 足够。
//! - **旧 token 兼容**：校验与格式无关（常量时间比较前仅做大小写归一），
//!   存量 64 hex token 继续有效，下次「重置令牌」自然换新格式；**不做迁移
//!   强制重置**。
//! - 落盘走 `opx_core::utils::paths::write_file_restricted` —— 项目铁律：
//!   **敏感文件统一受限写盘，禁止先 write 再单独 restrict**（窗口期内任意
//!   本机进程可读）。

use opx_core::utils::paths;
use rand_core::RngCore;
use subtle::ConstantTimeEq;

/// token 持久化文件名（config_dir 下）。⚠️ 4.2 起 token 迁入 settings.json，
/// 本文件仅供 4.0 骨架期与测试使用；迁移时删除文件并保留 settings 字段。
const TOKEN_FILE: &str = "web-token";

/// Crockford Base32 字符集（32 字符）：去 I/L/O/U 防手读混淆；
/// 10 位 ≈ 50 bit 熵（批次 4.6 裁定）。
const CROCKFORD32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 新 token 长度（10 × 5 bit = 50 bit）。
const TOKEN_LEN: usize = 10;

/// 生成一个新 token：10 位 Crockford Base32（`OsRng` CSPRNG）。
///
/// 无偏取样：256 = 32 × 8，`byte % 32` 天然均匀，**无需拒绝采样**。
pub fn generate() -> String {
    let mut buf = [0u8; TOKEN_LEN];
    // OsRng 是操作系统级 CSPRNG（getrandom），非密码学场景不得用它 —— 本处是。
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter()
        .map(|&b| CROCKFORD32[(b % 32) as usize] as char)
        .collect()
}

/// 常量时间比较：`presented`（请求携带）是否等于 `expected`（服务端保存）。
///
/// **与格式无关**（批次 4.6 兼容策略）：比较前仅做 trim + ASCII 大写归一
///（Crockford 与 hex 均大小写不敏感地接受），新旧两种形态（64 hex / 10 位）
/// 统一走这里。长度不等同样返回 false，且比较耗时与内容无关
///（`subtle::ConstantTimeEq` 对 `[u8]` 的实现对长度不等也走常量时间路径）。
pub fn verify(presented: &str, expected: &str) -> bool {
    let p = normalize(presented);
    let e = normalize(expected);
    bool::from(p.as_bytes().ct_eq(e.as_bytes()))
}

/// 校验前归一：trim 空白 + ASCII 大写（只影响比较结果，不改存储值）。
fn normalize(s: &str) -> String {
    s.trim().to_ascii_uppercase()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 批次 4.6：新 token 格式钉死——10 位 Crockford Base32（字符集去 I/L/O/U）。
    #[test]
    fn generated_tokens_are_10_crockford_chars_and_unique() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), TOKEN_LEN);
        for c in a.chars() {
            assert!(
                CROCKFORD32.contains(&(c as u8)),
                "字符 {c} 不在 Crockford 字符集内"
            );
            assert!(!"ILOU".contains(c), "混淆字符 {c} 不应出现");
        }
        assert_ne!(a, b, "CSPRNG 两次生成不应相同");
    }

    #[test]
    fn verify_matches_and_rejects() {
        let t = generate();
        assert!(verify(&t, &t));
        // 大小写不敏感（用户手输小写也能过）
        assert!(verify(&t.to_ascii_lowercase(), &t));
        assert!(!verify(&generate(), &t), "不同 token 必须拒绝");
        // 长度不等（截断/拼接攻击面）也要拒绝
        assert!(!verify(&t[..9], &t));
        assert!(!verify(&(t.clone() + "0"), &t));
        assert!(!verify("", &t));
    }

    /// 批次 4.6 兼容策略钉死：存量 64 hex token 继续有效（不做迁移强制重置）。
    #[test]
    fn legacy_64_hex_token_still_verifies() {
        let legacy = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(legacy.len(), 64);
        assert!(verify(legacy, legacy), "旧 64 hex token 必须继续可用");
        assert!(verify(legacy, &legacy.to_ascii_uppercase()), "hex 大小写归一");
        let new_fmt = generate();
        assert!(!verify(legacy, &new_fmt), "新旧 token 不得互通");
    }
}
