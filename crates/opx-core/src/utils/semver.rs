//! 版本号比较（语义化分段比较）。
//!
//! 原在壳层 `opx::commands::software::compare_versions`，是该文件里唯一一个
//! 「纯逻辑、零宿主依赖」的函数，故下沉到 core 供两个壳与 `providers` 共用——
//! 升级检测与各 provider 的远程版本排序**必须用同一份语义**，否则同一个版本号
//! 在「检测可升级」与「列表排序」两处会得出相反结论。

use std::cmp::Ordering;

/// 数字分段版本比较：`5.7.44 < 8.0.36`；`7.4.9 < 7.10.0`；
/// 任一段含非数字时退化为字符串比较（`v1 < v2`）。
///
/// 规则：
/// - 逐段解析为 `u64` 比较，**跳过相等段**（故 `7.4.9 < 7.10.0` 正确：第 2 段
///   `4< 10`，不会被 `9 < 0` 的字符串比较误导）；
/// - 任一段解析失败 → 整体退化为字符串字典序（`v1 < v2`）；
/// - 所有公共段相等 → 段数少的更小（`1.0 < 1.0.1`）。
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let ap: Vec<&str> = a.split('.').collect();
    let bp: Vec<&str> = b.split('.').collect();
    for i in 0..ap.len().min(bp.len()) {
        match (ap[i].parse::<u64>(), bp[i].parse::<u64>()) {
            (Ok(x), Ok(y)) if x != y => return x.cmp(&y),
            (Ok(_), Ok(_)) => {}
            _ => return a.cmp(b),
        }
    }
    ap.len().cmp(&bp.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 原函数注释里给出的三个基准 case——它们是本函数存在的理由，
    /// 故钉死为回归测试。
    #[test]
    fn compares_numeric_segments() {
        // 5.7.44 < 8.0.36：首位即分出大小
        assert_eq!(compare_versions("5.7.44", "8.0.36"), Ordering::Less);
        // 反向也成立（Ordering 必须对称）
        assert_eq!(compare_versions("8.0.36", "5.7.44"), Ordering::Greater);
        // 7.4.9 < 7.10.0：第 2 段 4 < 10。若错误地按字符串比 '9' > '1' 会得出相反结论，
        // 这是本函数最容易被改坏的一条。
        assert_eq!(compare_versions("7.4.9", "7.10.0"), Ordering::Less);
        assert_eq!(compare_versions("7.10.0", "7.4.9"), Ordering::Greater);
    }

    /// 含非数字段（`v1` / `pre`）时退化为字符串字典序。
    #[test]
    fn falls_back_to_string_compare_on_non_numeric() {
        assert_eq!(compare_versions("v1", "v2"), Ordering::Less);
        assert_eq!(compare_versions("v2", "v1"), Ordering::Greater);
        // 前段可解析、后段不可解析 → 同样退化（而非只比已解析的前缀）
        assert_eq!(compare_versions("1.0.beta", "1.0.alpha"), Ordering::Greater);
    }

    #[test]
    fn equal_and_prefix_versions() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
        // 公共段全等 → 段数少者更小
        assert_eq!(compare_versions("1.0", "1.0.1"), Ordering::Less);
        assert_eq!(compare_versions("1.0.1", "1.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Ordering::Equal);
    }
}