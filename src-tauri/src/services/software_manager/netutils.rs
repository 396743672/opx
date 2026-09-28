//! Windows 端口采集：解析 `netstat -ano` 的 LISTENING 行 → (pid, port, addr)。
//!
//! 仅采集监听态；命令失败/权限不足时降级为空集，不阻塞上层。

use crate::utils::process::hidden;

/// 一条监听记录。pid 缺失（权限不足等情况）记 None。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ListenEntry {
    pub pid: Option<u32>,
    pub port: u16,
    pub addr: String,
}

/// 运行 `netstat -ano` 并解析监听记录。失败返回空集（降级不崩）。
pub fn listen_entries() -> Vec<ListenEntry> {
    // 隐藏控制台窗口：GUI 进程起 netstat 会闪黑窗（本函数在运行态被周期调用）
    let Ok(out) = hidden("netstat").arg("-ano").output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    // 中文 Windows 的 netstat 输出为 OEM 代码页，但 LISTENING/地址/pid 均为 ASCII，lossy 足够
    parse_netstat_listening(&String::from_utf8_lossy(&out.stdout))
}

/// 解析 netstat 输出中的 LISTENING 行。
/// 形如 `  TCP    0.0.0.0:80      0.0.0.0:0       LISTENING       1234`。
/// 畸形行/非 TCP/非监听态跳过。
pub fn parse_netstat_listening(output: &str) -> Vec<ListenEntry> {
    let mut result = Vec::new();
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 期望：TCP <local> <foreign> LISTENING <pid>
        if cols.len() < 5 || cols[0] != "TCP" || cols[3] != "LISTENING" {
            continue;
        }
        let Some(port) = port_of(cols[1]) else { continue };
        result.push(ListenEntry {
            pid: cols[4].parse().ok(),
            port,
            addr: cols[1].to_string(),
        });
    }
    result
}

/// 从本地地址取端口：`0.0.0.0:80` / `[::]:80` / `127.0.0.1:5432` → 端口号。
fn port_of(addr: &str) -> Option<u16> {
    addr.rsplit(':').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listening_skips_malformed() {
        let sample = "\
活动连接

  协议  本地地址          外部地址        状态           PID
  TCP    0.0.0.0:80             0.0.0.0:0              LISTENING       1234
  TCP    [::]:443               [::]:0                 LISTENING       1234
  TCP    127.0.0.1:5432         0.0.0.0:0              LISTENING       5678
  TCP    192.168.1.5:51000      93.184.216.34:443      ESTABLISHED     9999
  UDP    0.0.0.0:53             *:*                                    4321
  TCP    0.0.0.0:8080           0.0.0.0:0              LISTENING       notapid
  垃圾行
";
        let got = parse_netstat_listening(sample);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], ListenEntry { pid: Some(1234), port: 80, addr: "0.0.0.0:80".into() });
        assert_eq!(got[1], ListenEntry { pid: Some(1234), port: 443, addr: "[::]:443".into() });
        assert_eq!(got[2], ListenEntry { pid: Some(5678), port: 5432, addr: "127.0.0.1:5432".into() });
        // pid 解析失败降级为 None，端口仍保留
        assert_eq!(got[3], ListenEntry { pid: None, port: 8080, addr: "0.0.0.0:8080".into() });
    }

    #[test]
    fn empty_or_garbage_returns_empty() {
        assert!(parse_netstat_listening("").is_empty());
        assert!(parse_netstat_listening("hello world").is_empty());
    }
}
