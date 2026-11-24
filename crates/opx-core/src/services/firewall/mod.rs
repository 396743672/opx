//! 防火墙端口规则维护（跨平台：ufw / firewalld / nftables / Windows 防火墙）。
//!
//! ## 后端自动判定（Linux）
//!
//! 不看「装了哪个命令」就算了，而是按**谁真正在生效**排序（见 [`decide`]）：
//! ufw 生效 → ufw；firewalld 运行 → firewalld；都没生效才退回「装了谁」，
//! 最后才是裸 nftables。原因很实际：Debian 上常见 `ufw` 已装但 `Status: inactive`，
//! 而真正过滤流量的是 firewalld/Docker 的规则 —— 这时往 ufw 写规则等于什么也没做，
//! 用户看到「已放行」却连不上，比不显示更糟。
//!
//! ## 设计边界（三条，改动前先读）
//!
//! 1. **只维护端口规则，绝不启停防火墙服务**。
//!    本模块不执行 `ufw enable|disable`、`systemctl start|stop firewalld`、
//!    `netsh advfirewall set allprofiles state off`。用户的机器该不该开防火墙是
//!    用户的决定，opx 只负责「需要放行的端口确实是放行状态」。因此
//!    `ufw allow` 在 ufw 处于 inactive 时也会写入规则文件，用户日后 enable
//!    时规则自动生效 —— 这正是我们要的行为。
//!
//! 2. **只接管能安全表达「端口已放行」的后端**。
//!    - ufw / firewalld：原生端口语义，覆盖 Debian/Ubuntu 与 RHEL/Fedora/SUSE/openEuler；
//!    - Windows 防火墙：`opx-<port>-<proto>` 命名规则，可精确撤销；
//!    - **nftables：仅在系统规则集为空、或不存在任何 `drop`/`reject` 时接管**。
//!      Debian 13 起默认只装 nftables（无 ufw），实测目标机规则集就是空的 ——
//!      此时 opx 新建**独立表 `inet opx`**（`policy accept` + 纯 `accept` 规则）是
//!      纯加法，删掉它也不会影响系统任何行为。
//!      一旦系统已有拦截规则，优先级/顺序判断极易出错（nftables 的 `accept`
//!      会继续执行后续链，插在 priority 更小的位置「看着放行了实际仍被 drop」；
//!      插错顺序还可能把 SSH 一起挡掉），故此时返回 [`BackendKind::None`]，交给人工。
//!    - **不碰裸 iptables 规则**（无可撤销的命名语义，同上）。
//!
//! 3. **失败不阻断主流程**。安装/启动调用方拿到 `Err` 只记日志 + 审计，
//!    绝不让「防火墙放行失败」导致「软件装不上 / 起不来」。
//!
//! 4. **nftables 规则不持久化到磁盘**。opx 启动时会做一次全量校对
//!    （见 `sync_all`），所以重启后规则由 opx 自行重建 —— 用「启动校对」换
//!    「不写系统级 nftables.conf」，避免改坏用户的防火墙配置文件。
//!
//! ## 幂等
//!
//! `allow` 先查当前规则，已存在返回 `Ok(false)`（未改动）；
//! `revoke` 不存在同样返回 `Ok(false)`。调用方可以无条件重复调用。

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::utils::process::hidden;

/// 传输层协议。仅这两种有「端口」概念，其它（ICMP 等）不在本模块范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    pub fn as_str(self) -> &'static str {
        match self {
            Proto::Tcp => "tcp",
            Proto::Udp => "udp",
        }
    }

    /// 解析 `"tcp"` / `"TCP"` / `"udp"`，其它返回 None。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tcp" => Some(Proto::Tcp),
            "udp" => Some(Proto::Udp),
            _ => None,
        }
    }
}

/// 一条「放行某端口」的规则。
///
/// `source` 为 `None` 表示对任意来源放行；`Some(ip_or_cidr)` 表示**仅该来源**
/// 放行（单 IP 或 CIDR，如 `192.168.200.1` / `10.0.0.0/8`）。来源是规则身份的
/// 一部分：同一个端口可以同时存在「全网放行」与「仅限某 IP」两条独立规则。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortRule {
    pub port: u16,
    pub proto: Proto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl PortRule {
    pub fn tcp(port: u16) -> Self {
        Self { port, proto: Proto::Tcp, source: None }
    }

    pub fn udp(port: u16) -> Self {
        Self { port, proto: Proto::Udp, source: None }
    }

    /// 带来源限定的规则。
    pub fn sourced(port: u16, proto: Proto, source: impl Into<String>) -> Self {
        Self { port, proto, source: Some(source.into()) }
    }

    /// ufw / firewalld 通用的 `8080/tcp` 形态。
    pub fn spec(&self) -> String {
        format!("{}/{}", self.port, self.proto.as_str())
    }

    /// 人读的完整描述（含来源），用于日志与审计。
    pub fn spec_full(&self) -> String {
        match &self.source {
            Some(s) => format!("{} from {}", self.spec(), s),
            None => self.spec(),
        }
    }
}

/// 实际可用的防火墙后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendKind {
    /// Debian / Ubuntu 系
    Ufw,
    /// RHEL / Fedora / SUSE / openEuler 系
    Firewalld,
    /// 裸 nftables（Debian 13+ 默认只装这个）：独立表 `inet opx`，仅纯加法场景接管
    Nftables,
    /// Windows 高级防火墙（netsh advfirewall）
    Windows,
    /// 未检测到可安全接管的后端（含 macOS / Alpine / 容器 / 已有自定义拦截规则）
    None,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::Ufw => "ufw",
            BackendKind::Firewalld => "firewalld",
            BackendKind::Nftables => "nftables",
            BackendKind::Windows => "windows",
            BackendKind::None => "none",
        }
    }
}

/// 判定依据：为什么落到这个后端。前端按 code 取 i18n 词条。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Windows 高级防火墙
    Windows,
    /// 非 Linux/Windows 平台（macOS 没有端口级命令行接口）
    UnsupportedOs,
    /// ufw 已安装且正在生效
    UfwActive,
    /// ufw 已安装但没启用：规则写进配置，待用户启用后生效
    UfwInactive,
    /// firewalld 已安装且正在运行
    FirewalldRunning,
    /// firewalld 已安装但没有运行
    FirewalldStopped,
    /// 没有托管防火墙，接管裸 nftables（独立表，纯加法）
    Nftables,
    /// 系统已有 nftables 拦截规则，不接管
    NftBlocking,
    /// nftables 规则集读不到（通常是非 root），无法确认安全，不接管
    NftUnreadable,
    /// 没有任何可用的防火墙工具
    NoTool,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Windows => "windows",
            Reason::UnsupportedOs => "unsupported_os",
            Reason::UfwActive => "ufw_active",
            Reason::UfwInactive => "ufw_inactive",
            Reason::FirewalldRunning => "firewalld_running",
            Reason::FirewalldStopped => "firewalld_stopped",
            Reason::Nftables => "nftables",
            Reason::NftBlocking => "nft_blocking",
            Reason::NftUnreadable => "nft_unreadable",
            Reason::NoTool => "no_tool",
        }
    }

    /// 该后端是否**正在生效**。false 表示「已安装未启用」：规则能写、但现在不拦也不放。
    pub fn active(self) -> bool {
        matches!(
            self,
            Reason::Windows | Reason::UfwActive | Reason::FirewalldRunning | Reason::Nftables
        )
    }

    fn note(self) -> &'static str {
        match self {
            Reason::Windows => "已检测到 Windows 防火墙。放行端口需要以管理员身份运行 opx。",
            Reason::UnsupportedOs => "当前系统没有可自动维护的端口级防火墙。",
            Reason::UfwActive => "已检测到 ufw 且正在生效。opx 只维护端口规则，不会启用或停用 ufw。",
            Reason::UfwInactive => {
                "已检测到 ufw，但当前未启用（Status: inactive）。放行规则会写入 ufw 配置，待你启用 ufw 后生效；opx 不会替你启用。"
            }
            Reason::FirewalldRunning => {
                "已检测到 firewalld 且正在运行。opx 只维护端口规则，不会启动或停止 firewalld。"
            }
            Reason::FirewalldStopped => {
                "已检测到 firewalld，但当前没有运行。放行规则会写入永久配置，待 firewalld 启动后生效；opx 不会替你启动。"
            }
            Reason::Nftables => {
                "已检测到 nftables（系统无自定义拦截规则）。opx 在独立表 inet opx 中维护放行端口，不修改系统现有规则；规则随 opx 启动自动重建。"
            }
            Reason::NftBlocking => {
                "系统已存在 nftables 拦截规则（drop/reject，常来自 Docker 或自定义配置），opx 不会自动插入规则以免影响现有链路。请手动放行端口。"
            }
            Reason::NftUnreadable => {
                "无法读取 nftables 规则集（通常需要 root 权限），无法确认自动接管是否安全，因此不改动系统规则。"
            }
            Reason::NoTool => {
                "未检测到 ufw、firewalld 或 nftables。请在系统防火墙中手动放行端口；opx 不会改动 iptables 裸规则。"
            }
        }
    }
}

/// 探测结果：后端 + 判定依据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub kind: BackendKind,
    pub reason: Reason,
}

/// 「规则到底有没有在拦」——和「装没装 / 启没启」是两个不同的问题。
///
/// 这个区分是页面的核心：当系统没有任何拦截规则时（典型的 Debian 13 空规则集），
/// 所有端口本来就能从外部访问，页面上的「放行/撤销」只是登记，**不改变可达性**。
/// 不把这件事说清楚，用户就会以为开关失效、或以为端口已经被保护。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enforcement {
    /// 防火墙在拦：只有显式放行的端口可以从外部访问
    Enforced,
    /// 只有 opx 建的登记表（policy accept + 纯 accept），系统不拦任何入站
    BookkeepingOnly,
    /// 系统有自己的拦截规则，但不是 opx 管的那套
    SystemBlocking,
    /// 防火墙装了但没启用
    Disabled,
    /// 规则读不到 / 平台不支持，无法判定
    Unknown,
}

impl Enforcement {
    pub fn as_str(self) -> &'static str {
        match self {
            Enforcement::Enforced => "enforced",
            Enforcement::BookkeepingOnly => "bookkeeping_only",
            Enforcement::SystemBlocking => "system_blocking",
            Enforcement::Disabled => "disabled",
            Enforcement::Unknown => "unknown",
        }
    }
}

/// 由判定依据推出「是否在拦截」。Windows 需要额外看配置文件开关，故传入。
fn enforcement_of(reason: Reason, windows_on: bool) -> Enforcement {
    match reason {
        Reason::Windows => {
            if windows_on {
                Enforcement::Enforced
            } else {
                Enforcement::Disabled
            }
        }
        Reason::UfwActive | Reason::FirewalldRunning => Enforcement::Enforced,
        // opx 的独立表是 `policy accept` + 纯 accept 规则：只登记，不拦任何东西
        Reason::Nftables => Enforcement::BookkeepingOnly,
        Reason::NftBlocking => Enforcement::SystemBlocking,
        Reason::NftUnreadable | Reason::UnsupportedOs => Enforcement::Unknown,
        Reason::UfwInactive | Reason::FirewalldStopped | Reason::NoTool => Enforcement::Disabled,
    }
}

/// 给前端展示的后端状态。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendInfo {
    /// "ufw" | "firewalld" | "nftables" | "windows" | "none"
    pub kind: String,
    /// 是否可自动维护规则（false 时前端只显示指引，不显示放行开关）
    pub available: bool,
    /// 防火墙当前是否正在生效（已安装未启用 = false，规则要等启用后才起作用）
    pub active: bool,
    /// 规则是否真的在拦截入站流量（见 [`Enforcement`]）
    pub enforcement: String,
    /// 判定依据 code（`ufw_active` / `firewalld_stopped` / `nft_blocking` …），前端据此取词条
    pub reason: String,
    /// 面向用户的说明（前端有 i18n 时优先用前端词条，本字段作兜底）
    pub note: String,
}

/// 系统里各防火墙工具的「装了没 / 在用没」。抽成纯数据是为了让 [`decide`] 可单测。
#[derive(Debug, Clone, Copy, Default)]
struct Caps {
    ufw_installed: bool,
    ufw_active: bool,
    firewalld_installed: bool,
    firewalld_active: bool,
    nft_installed: bool,
}

/// 裸 nftables 的可接管性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NftState {
    /// 没装 nft
    Missing,
    /// 装 ufw/firewalld 时不必探测（两种情况都不会选 nft）
    Skipped,
    /// 规则集为空或只有 accept → 纯加法，安全
    Clean,
    /// 已有 drop/reject → 不接管
    Blocking,
    /// 读不到（权限不足）→ 无法确认安全，不接管
    Unreadable,
}

/// 纯判定：给定工具状态，选出该用哪个后端。
///
/// 排序原则是「**谁真正在生效**」优先，其次才是「装了谁」：
/// 1. ufw 正在生效 → ufw；
/// 2. firewalld 正在运行 → firewalld（**ufw 装了但没启用时不能抢**：那样规则写进
///    ufw 配置却拦不住任何东西，用户会以为已放行，而真正过滤流量的是 firewalld）；
/// 3. 都没生效：ufw 装了 → ufw（规则先落盘，启用即生效）；再轮到 firewalld；
/// 4. 都没有 → 裸 nftables（仅在无拦截规则时）；
/// 5. 都不行 → None，交给人工。
fn decide(c: Caps, nft: NftState) -> Detection {
    let det = |kind, reason| Detection { kind, reason };
    if c.ufw_active {
        return det(BackendKind::Ufw, Reason::UfwActive);
    }
    if c.firewalld_active {
        return det(BackendKind::Firewalld, Reason::FirewalldRunning);
    }
    if c.ufw_installed {
        return det(BackendKind::Ufw, Reason::UfwInactive);
    }
    if c.firewalld_installed {
        return det(BackendKind::Firewalld, Reason::FirewalldStopped);
    }
    match nft {
        NftState::Clean => det(BackendKind::Nftables, Reason::Nftables),
        NftState::Blocking => det(BackendKind::None, Reason::NftBlocking),
        NftState::Unreadable => det(BackendKind::None, Reason::NftUnreadable),
        NftState::Missing | NftState::Skipped => det(BackendKind::None, Reason::NoTool),
    }
}

/// 探测当前系统实际可用的防火墙后端。
pub fn detect() -> Detection {
    if cfg!(windows) {
        return Detection { kind: BackendKind::Windows, reason: Reason::Windows };
    }
    if cfg!(target_os = "macos") {
        // macOS 的 Application Firewall 没有端口放行的命令行接口，不纳入支持
        return Detection { kind: BackendKind::None, reason: Reason::UnsupportedOs };
    }
    // `ufw --version` / `firewall-cmd --version` / `nft --version` 都不需要 root
    let ufw_installed = command_ok("ufw", &["--version"]);
    let firewalld_installed = command_ok("firewall-cmd", &["--version"]);
    let caps = Caps {
        ufw_installed,
        // 装了才问状态：没装时问一次就是白 spawn 一个进程
        ufw_active: ufw_installed && ufw_enabled(),
        firewalld_installed,
        firewalld_active: firewalld_installed && firewalld_running(),
        nft_installed: command_ok("nft", &["--version"]),
    };
    let nft = if ufw_installed || firewalld_installed {
        NftState::Skipped
    } else {
        probe_nft(caps.nft_installed)
    };
    decide(caps, nft)
}

/// ufw 是否正在生效。
///
/// 两条信号：`ufw status`（root 下的权威答案）与 `/etc/ufw/ufw.conf` 的 `ENABLED=yes`
///（非 root 也能读）。只认 active —— `Status: inactive` 表示装了没开，此时写规则
///不会生效，UI 必须说清楚，否则用户会以为端口已放行。
fn ufw_enabled() -> bool {
    if let Some(out) = run_raw("ufw", &["status"]) {
        if out.contains("Status:") {
            return parse_ufw_status(&out);
        }
        // 非 root 会打成「ERROR: You need to be root」，没有 Status 行 → 落到配置文件
    }
    std::fs::read_to_string("/etc/ufw/ufw.conf")
        .map(|s| parse_ufw_conf(&s))
        .unwrap_or(false)
}

/// firewalld 是否正在运行。
///
/// 先问 `firewall-cmd --state`（输出 `running` / `not running`），失败再用
/// `systemctl is-active` 兜底 —— 容器里没有 D-Bus 时前者会报授权失败，
/// 但 systemctl 在多数容器里也查不到服务，此时按「没运行」处理是安全侧。
fn firewalld_running() -> bool {
    if let Some(out) = run_raw("firewall-cmd", &["--state"]) {
        if parse_firewalld_state(&out) {
            return true;
        }
        if out.to_ascii_lowercase().contains("not running") {
            return false;
        }
    }
    run_raw("systemctl", &["is-active", "firewalld"])
        .map(|o| o.trim().eq_ignore_ascii_case("active"))
        .unwrap_or(false)
}

fn probe_nft(installed: bool) -> NftState {
    if !installed {
        return NftState::Missing;
    }
    match run("nft", &["list", "ruleset"]) {
        // 规则集为空或只有 accept：独立表是纯加法，安全接管
        Ok(text) if !has_blocking_rules(&text) => NftState::Clean,
        Ok(_) => NftState::Blocking,
        Err(_) => NftState::Unreadable,
    }
}

/// `ufw status` 是否为 active（`Status: active`，容忍缩进与大小写）。
fn parse_ufw_status(text: &str) -> bool {
    text.lines().any(|line| {
        line.trim()
            .strip_prefix("Status:")
            .map(|rest| rest.trim().eq_ignore_ascii_case("active"))
            .unwrap_or(false)
    })
}

/// `/etc/ufw/ufw.conf` 里的 `ENABLED=yes`。
fn parse_ufw_conf(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#')
            && line
                .split_once('=')
                .map(|(k, v)| k.trim().eq_ignore_ascii_case("ENABLED") && v.trim().eq_ignore_ascii_case("yes"))
                .unwrap_or(false)
    })
}

/// `firewall-cmd --state` 是否输出 `running`（输出可能带 stderr 里的告警行）。
fn parse_firewalld_state(text: &str) -> bool {
    text.lines().any(|l| l.trim().eq_ignore_ascii_case("running"))
}

/// 规则集里是否存在会让包「被拒绝/丢弃」的规则。
///
/// 有这类规则时，「在独立表里 accept」并不能真正放行（nftables 的 accept 只结束
/// 当前链，后续链的 drop 依然生效），而且自动插入有把 SSH 挡掉的风险 —— 故不接管。
///
/// 判定按**词**而不是按行尾：`policy drop;`、`ip saddr ... drop`、独立的 `reject`
/// 行都要能识别。行注释已先排除；行尾注释里出现 drop 会误判为「有拦截」，
/// 这是安全侧偏移（宁可不接管，也不误伤）。
fn has_blocking_rules(ruleset: &str) -> bool {
    ruleset.lines().any(|line| {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            return false;
        }
        t.split(|c: char| c.is_whitespace() || c == ';' || c == '{' || c == '}')
            .any(|tok| tok == "drop" || tok == "reject")
    })
}

/// 带缓存的探测结果（后端在进程生命周期内不会变）。
///
/// 缓存是有意为之：`get_firewall_overview` 每次刷新都会调，探测要跑 3-5 个子进程。
/// 代价是用户在 opx 运行期间启停防火墙后需重启 opx 才能换后端 —— 这与
/// 「opx 不启停防火墙服务」的边界一致，用户改防火墙本来就该重启一次。
pub fn detection() -> Detection {
    static CACHE: OnceLock<Detection> = OnceLock::new();
    *CACHE.get_or_init(detect)
}

/// 只需要后端类型时的便捷入口。
pub fn backend() -> BackendKind {
    detection().kind
}

/// 后端信息（供 UI 展示）。
pub fn info(det: Detection) -> BackendInfo {
    // 只在真有 Windows 后端时才去问 netsh：其它情况白 spawn 一个进程没意义。
    // 走缓存版：get_firewall_overview 每次刷新都调 info()，而这条命令实测 ~195ms。
    let windows_on = det.reason != Reason::Windows || windows_firewall_on_cached();
    BackendInfo {
        kind: det.kind.as_str().to_string(),
        available: det.kind != BackendKind::None,
        active: det.reason.active(),
        enforcement: enforcement_of(det.reason, windows_on).as_str().to_string(),
        reason: det.reason.as_str().to_string(),
        note: det.reason.note().to_string(),
    }
}

/// Windows 防火墙是否在拦（三个配置文件任一开启即算）。
///
/// 解析失败按「已开启」处理：宁可少报一次「未保护」，也不要误报「没开防火墙」
/// 让用户白排查一遍。非 Windows 平台不会走到这里。
fn windows_firewall_on() -> bool {
    if !cfg!(windows) {
        return true;
    }
    run_raw("netsh", &["advfirewall", "show", "allprofiles", "state"])
        .map(|t| parse_netsh_state_on(&t))
        .unwrap_or(true)
}

/// 进程级缓存的开关判定：`get_firewall_overview` 每次刷新都经 `info()` 走到这里，
/// 而 `show allprofiles state` 要 spawn 一个 netsh（实测 ~195ms）。与 `detection()`
/// 同理，防火墙开关在 opx 运行期不变（opx 不启停防火墙服务），缓存安全。
fn windows_firewall_on_cached() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(windows_firewall_on)
}

/// 解析 `netsh advfirewall show allprofiles state` 的输出。
///
/// 中英文输出都要认：`State ON` / `状态 启用`。只要有一个配置文件是 ON 就算在拦。
fn parse_netsh_state_on(text: &str) -> bool {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let is_state_line = lower.contains("state") || line.contains("状态");
        if !is_state_line {
            continue;
        }
        let on = lower.split_whitespace().any(|t| t == "on")
            || line.split_whitespace().any(|t| t == "启用");
        let off = lower.split_whitespace().any(|t| t == "off")
            || line.split_whitespace().any(|t| t == "禁用");
        if on && !off {
            return true;
        }
    }
    false
}

/// 进程级规则表缓存。
///
/// 为什么要缓存：Windows 上 `netsh show rule name=all` 要枚举约 700 条规则、
/// 解析 6000+ 行输出，实测约 700ms；opx 是本机这些 `opx-*` 规则的唯一写入方，
/// 读回只用于展示与幂等判断。首次读一次，之后命中缓存，写入时增量维护。
///
/// 取舍（已与用户确认）：外部手动改系统防火墙，本进程内不会立即感知，需重启
/// opx。opx 自己经 [`allow`]/[`revoke`] 的改动则即时反映。
#[derive(Default)]
struct RulesCache {
    by_kind: RwLock<HashMap<BackendKind, Vec<PortRule>>>,
}

impl RulesCache {
    fn get(&self, kind: BackendKind) -> Option<Vec<PortRule>> {
        self.by_kind.read().ok()?.get(&kind).cloned()
    }

    fn put(&self, kind: BackendKind, rules: Vec<PortRule>) {
        if let Ok(mut map) = self.by_kind.write() {
            map.insert(kind, rules);
        }
    }

    fn apply_add(&self, kind: BackendKind, rule: &PortRule) {
        if let Ok(mut map) = self.by_kind.write() {
            if let Some(rules) = map.get_mut(&kind) {
                if !rules.contains(rule) {
                    rules.push(rule.clone());
                }
            }
        }
    }

    fn apply_remove(&self, kind: BackendKind, rule: &PortRule) {
        if let Ok(mut map) = self.by_kind.write() {
            if let Some(rules) = map.get_mut(&kind) {
                rules.retain(|r| r != rule);
            }
        }
    }
}

fn rules_cache() -> &'static RulesCache {
    static CACHE: OnceLock<RulesCache> = OnceLock::new();
    CACHE.get_or_init(RulesCache::default)
}

/// 列出当前已放行的端口（走进程缓存；写入方需同步维护，见 [`RulesCache`]）。
pub fn list_allowed(kind: BackendKind) -> Result<Vec<PortRule>> {
    list_allowed_with(kind, rules_cache(), load_allowed_fresh)
}

/// 可注入 loader 的缓存读取，便于单测验证「只加载一次」。
fn list_allowed_with<F>(kind: BackendKind, cache: &RulesCache, load: F) -> Result<Vec<PortRule>>
where
    F: Fn(BackendKind) -> Result<Vec<PortRule>>,
{
    if let Some(cached) = cache.get(kind) {
        return Ok(cached);
    }
    let rules = load(kind)?;
    cache.put(kind, rules.clone());
    Ok(rules)
}

/// 真正跑系统命令读取规则（不缓存）。
///
/// 口径差异说明：ufw/firewalld 读的是**持久配置层**（`ufw show added` /
/// `firewall-cmd --permanent --list-ports`），与「是否已 enable/start」无关，
/// 与 [`allow`] 写入的层一致 —— 这样查询结果与实际维护的规则永远同源。
fn load_allowed_fresh(kind: BackendKind) -> Result<Vec<PortRule>> {
    match kind {
        BackendKind::Ufw => {
            let text = run("ufw", &["show", "added"])?;
            Ok(parse_ufw_added(&text))
        }
        BackendKind::Firewalld => {
            let text = run("firewall-cmd", &["--permanent", "--list-ports"])?;
            let mut rules = parse_port_list(&text);
            // 来源限定规则存为富规则，端口列表里看不到，需另查
            let rich = run("firewall-cmd", &["--permanent", "--list-rich-rules"]).unwrap_or_default();
            rules.extend(parse_firewalld_rich_rules(&rich));
            Ok(rules)
        }
        BackendKind::Windows => list_windows_rules(),
        BackendKind::Nftables => nft_list(),
        BackendKind::None => Err(anyhow!("未检测到受支持的防火墙后端")),
    }
}

/// 放行端口。已放行时返回 `Ok(false)`（幂等）。
pub fn allow(kind: BackendKind, rule: &PortRule) -> Result<bool> {
    validate(rule)?;
    // 先查：避免重复写入（ufw/firewalld 的重复添加多数是幂等的，但 Windows
    // 会累积同名规则，必须先判）
    if list_allowed(kind)?.contains(rule) {
        return Ok(false);
    }
    let changed = match kind {
        BackendKind::Ufw => {
            let args = ufw_args("allow", rule);
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run("ufw", &refs)?;
            true
        }
        BackendKind::Firewalld => {
            match &rule.source {
                Some(src) => run(
                    "firewall-cmd",
                    &["--permanent", &format!("--add-rich-rule={}", firewalld_rich_rule(rule, src))],
                )?,
                None => run(
                    "firewall-cmd",
                    &["--permanent", &format!("--add-port={}", rule.spec())],
                )?,
            };
            reload_firewalld_best_effort();
            true
        }
        BackendKind::Windows => {
            add_windows_rule(rule)?;
            true
        }
        BackendKind::Nftables => {
            nft_add(rule)?;
            true
        }
        BackendKind::None => return Err(anyhow!("未检测到受支持的防火墙后端，无法自动放行端口")),
    };
    if changed {
        rules_cache().apply_add(kind, rule);
    }
    Ok(changed)
}

/// 撤销放行。本就没有该规则时返回 `Ok(false)`（幂等）。
pub fn revoke(kind: BackendKind, rule: &PortRule) -> Result<bool> {
    validate(rule)?;
    if !list_allowed(kind)?.contains(rule) {
        return Ok(false);
    }
    let changed = match kind {
        BackendKind::Ufw => {
            let args = ufw_args("delete", rule);
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run("ufw", &refs)?;
            true
        }
        BackendKind::Firewalld => {
            match &rule.source {
                Some(src) => run(
                    "firewall-cmd",
                    &[
                        "--permanent",
                        &format!("--remove-rich-rule={}", firewalld_rich_rule(rule, src)),
                    ],
                )?,
                None => run(
                    "firewall-cmd",
                    &["--permanent", &format!("--remove-port={}", rule.spec())],
                )?,
            };
            reload_firewalld_best_effort();
            true
        }
        BackendKind::Windows => {
            delete_windows_rule(rule)?;
            true
        }
        BackendKind::Nftables => {
            nft_remove(rule)?;
            true
        }
        BackendKind::None => return Err(anyhow!("未检测到受支持的防火墙后端，无法撤销端口放行")),
    };
    if changed {
        rules_cache().apply_remove(kind, rule);
    }
    Ok(changed)
}

/// 批量放行（用于安装时按 provider 声明的全部端口一次放行）。
///
/// 返回成功放行的端口列表；**单个端口失败不影响其余**，失败项进 `errors`。
/// 调用方只记日志，不阻断安装。
pub fn allow_many(kind: BackendKind, rules: &[PortRule]) -> (Vec<PortRule>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for rule in rules {
        match allow(kind, rule) {
            Ok(_) => ok.push(rule.clone()),
            Err(e) => errors.push(format!("{}: {e}", rule.spec())),
        }
    }
    (ok, errors)
}

/// 来源字段校验：`None` 合法；`Some` 必须是可解析的 IP 或 CIDR。
pub fn validate_source(source: &str) -> Result<()> {
    let s = source.trim();
    let fail = || anyhow!("来源地址不合法：「{s}」（应为 IP 或 CIDR，如 192.168.200.1 / 10.0.0.0/8）");
    let (ip_part, prefix) = match s.split_once('/') {
        Some((ip, p)) => (ip, p.parse::<u8>().map_err(|_| fail())?),
        None => {
            s.parse::<std::net::IpAddr>().map_err(|_| fail())?;
            return Ok(());
        }
    };
    let ip = ip_part.parse::<std::net::IpAddr>().map_err(|_| fail())?;
    let max = match ip {
        std::net::IpAddr::V4(_) => 32,
        std::net::IpAddr::V6(_) => 128,
    };
    if prefix > max {
        return Err(fail());
    }
    Ok(())
}

fn validate(rule: &PortRule) -> Result<()> {
    if rule.port == 0 {
        return Err(anyhow!("端口不能为 0"));
    }
    if let Some(src) = &rule.source {
        validate_source(src)?;
    }
    Ok(())
}

/// ufw 的规则参数。语法差异：`ufw allow <目标>` 只有一个 allow；`delete` 则是
/// `ufw delete allow <目标>`。目标两种形态 —— 全网规则 `3306/tcp`；来源规则
/// `from <src> to any port 3306 proto tcp`（allow/delete 同形，须拆成多参数 ——
/// ufw 把带空格的单参数当规则文本原样处理，实测会报 "bad rule"）。
fn ufw_args(action: &str, rule: &PortRule) -> Vec<String> {
    let mut args = vec![action.to_string()];
    if action == "delete" {
        args.push("allow".to_string());
    }
    match &rule.source {
        Some(src) => {
            for tok in format!(
                "from {} to any port {} proto {}",
                src.trim(),
                rule.port,
                rule.proto.as_str()
            )
            .split_whitespace()
            {
                args.push(tok.to_string());
            }
        }
        None => args.push(rule.spec()),
    }
    args
}

/// firewalld 富规则的规范形态（add/remove 同形）。
fn firewalld_rich_rule(rule: &PortRule, source: &str) -> String {
    // 按 CIDR 的地址部分判族：IpAddr::parse 不接受 "/"，IPv6 CIDR 会被误判为 ipv4
    let family = if source_is_v6(source) { "ipv6" } else { "ipv4" };
    format!(
        "rule family={family} source address={src} port port={port} protocol={proto} accept",
        src = source.trim(),
        port = rule.port,
        proto = rule.proto.as_str()
    )
}

/// 业务层最常用的入口：按端口号批量放行 TCP。
///
/// 各 manager（内置软件 / JAR / Node 应用 / 站点）只知道自己要监听哪个端口号，
/// 协议信息在 `ConfigFieldType::Port` 里并不存在 —— 这些被管服务（HTTP、MySQL、
/// Redis、Kafka……）实际全部走 TCP。需要 UDP 的场景（如 Consul 的 8301/8302
/// gossip）在端口页手动添加，不在这里猜。
///
/// 无可用后端时**静默返回空**：容器 / macOS / 未装 ufw 的机器每次启动都报错
/// 只会刷屏日志，无任何可操作性；该状态由端口页的后端徽章统一呈现。
///
/// 返回 `(已放行端口, 错误描述)`；错误由调用方记日志，绝不向上抛。
pub fn allow_tcp(ports: &[u16]) -> (Vec<u16>, Vec<String>) {
    let kind = backend();
    if kind == BackendKind::None {
        return (Vec::new(), Vec::new());
    }
    // nftables 专项：顺带做一次结构自检。
    //
    // 为什么放在这里而不是只在 `allow` 里：重复跳转规则是「表结构」问题，
    // 而端口已放行时 `allow` 会提前返回、根本走不到建链逻辑 —— 实测升级后
    // 存量机器上的重复规则一直残留。业务入口（安装/启动/启动校对）都会经过
    // 本函数，在这里修最省事；自检本身只有 1 次 `list chain`，重复时才重建。
    if kind == BackendKind::Nftables {
        if let Err(e) = nft_ensure() {
            return (Vec::new(), vec![format!("nftables 表结构自检失败: {e:#}")]);
        }
    }
    let rules: Vec<PortRule> = ports.iter().map(|p| PortRule::tcp(*p)).collect();
    let (ok, errors) = allow_many(kind, &rules);
    (ok.iter().map(|r| r.port).collect(), errors)
}

/// 撤销若干 TCP 端口的放行（卸载软件时用）。
pub fn revoke_tcp(ports: &[u16]) -> Vec<String> {
    let kind = backend();
    if kind == BackendKind::None {
        return Vec::new();
    }
    ports
        .iter()
        .filter_map(|p| revoke(kind, &PortRule::tcp(*p)).err().map(|e| format!("{p}/tcp: {e}")))
        .collect()
}

// ===================== 后端实现 =====================

fn reload_firewalld_best_effort() {
    // firewalld 未运行时 `--reload` 会失败，但 `--permanent` 已写入配置，
    // 服务下次启动会自动带上 —— 所以这里只尝试，不因失败而回滚。
    let _ = run("firewall-cmd", &["--reload"]);
}

/// netsh 规则名。来源编码进名字（`/` 在规则名里不可靠 → 换成 `_`；IP/CIDR 里
/// 本来就没有 `_`，可无损还原），否则 `show rule` 默认输出看不到 RemoteIP，
/// 来源规则会「消失」。
fn windows_rule_name(rule: &PortRule) -> String {
    match &rule.source {
        Some(src) => format!(
            "opx-{}-{}-src-{}",
            rule.port,
            rule.proto.as_str(),
            src.trim().replace('/', "_")
        ),
        None => format!("opx-{}-{}", rule.port, rule.proto.as_str()),
    }
}

/// 从规则名还原规则；形态不符返回 None。
fn parse_windows_rule_name(name: &str) -> Option<PortRule> {
    let rest = name.trim().strip_prefix("opx-")?;
    let (port_s, tail) = rest.split_once('-')?;
    let port = port_s.parse::<u16>().ok()?;
    if port == 0 {
        return None;
    }
    // `tcp` / `udp` 或 `tcp-src-192.168.200.1_32`
    let (proto_s, source) = match tail.split_once("-src-") {
        Some((p, encoded)) => {
            // `_` 只可能来自 `/`（IP/CIDR 不含下划线）
            (p, Some(encoded.replace('_', "/")))
        }
        None => (tail, None),
    };
    let proto = Proto::parse(proto_s)?;
    Some(PortRule { port, proto, source })
}

/// `netsh advfirewall firewall show rule` 的 `name=` 是微软文档标注的必填参数；
/// 只传 `dir=in` 会以退出码 1 报「一个或多个重要的参数没有输入」，导致整个防火墙页读不到规则。
/// `show rule name=opx-*` 的通配不支持，只能 `name=all` 全量列举再在解析层过滤。
const NETSH_LIST_RULES_ARGS: &[&str] = &[
    "advfirewall",
    "firewall",
    "show",
    "rule",
    "name=all",
    "dir=in",
];

fn list_windows_rules() -> Result<Vec<PortRule>> {
    let text = run("netsh", NETSH_LIST_RULES_ARGS)?;
    Ok(parse_netsh_rules(&text))
}

fn add_windows_rule(rule: &PortRule) -> Result<()> {
    let name = windows_rule_name(rule);
    let proto = if rule.proto == Proto::Tcp { "TCP" } else { "UDP" };
    let mut args: Vec<String> = vec![
        format!("name={name}"),
        "dir=in".to_string(),
        "action=allow".to_string(),
        "profile=any".to_string(),
        format!("protocol={proto}"),
        format!("localport={}", rule.port),
    ];
    if let Some(src) = &rule.source {
        args.push(format!("remoteip={}", src.trim()));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut full: Vec<&str> = vec!["advfirewall", "firewall", "add", "rule"];
    full.extend(arg_refs);
    run("netsh", &full)
        .map(|_| ())
        .map_err(|e| {
            // 最常见原因：非管理员运行。把 netsh 的原始报错补一句可操作提示。
            anyhow!("{e}（Windows 下放行端口需要以管理员身份运行 opx）")
        })
}

fn delete_windows_rule(rule: &PortRule) -> Result<()> {
    let name = windows_rule_name(rule);
    run(
        "netsh",
        &["advfirewall", "firewall", "delete", "rule", &format!("name={name}")],
    )
    .map(|_| ())
}

// ===================== nftables 后端 =====================
//
// 独立表结构（与系统现有规则集完全隔离）：
//
//   table inet opx {
//     set tcp_ports { type inet_service; }
//     set udp_ports { type inet_service; }
//     chain input {
//       type filter hook input priority 0; policy accept;
//       tcp dport @tcp_ports accept
//       udp dport @udp_ports accept
//     }
//   }
//
// 用**集合 + 单条规则**而不是逐条 `add rule`：
// - 增删集合元素是幂等的，`revoke` 只需删元素；
// - nft 会重排/格式化规则文本，按文本匹配删除极易失手，按元素删除不会。
//
// 表的 `policy accept` 且不含任何 drop/reject —— 即使这个表被整个删掉，
// 系统行为也不变（只是端口不再「显式放行」）。这就是「纯加法」的含义，
// 也是敢在裸 nftables 环境接管的依据（另见 `detect` 的拦截规则检查）。

const NFT_FAMILY: &str = "inet";
const NFT_TABLE: &str = "opx";

/// 串行化全部 nft 操作。
///
/// 为什么必须串行：`nft add set` 对**已存在**的集合也返回 0（实测），所以
/// 「先 list 判断、再 add」的写法在并发下会双双通过判断，随后各自 `add rule`
/// 追加一条跳转规则 —— 实测同一台机器上 tcp/udp 各出现两条重复规则。
/// 同一进程内的并发源头有三处：web_server 启动放行、启动全量校对、软件启动放行。
fn nft_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn nft_set_name(proto: Proto) -> &'static str {
    match proto {
        Proto::Tcp => "tcp_ports",
        Proto::Udp => "udp_ports",
    }
}

/// 来源限定规则的集合名：`<协议>_src` / `<协议>6_src`（IPv4 / IPv6 各一套）。
/// 类型是 `<addr>_addr . inet_service` 拼接集合，元素形如 `192.168.200.1 . 3306`。
fn nft_addr_set_name(proto: Proto, v6: bool) -> &'static str {
    match (proto, v6) {
        (Proto::Tcp, false) => "tcp_src",
        (Proto::Tcp, true) => "tcp6_src",
        (Proto::Udp, false) => "udp_src",
        (Proto::Udp, true) => "udp6_src",
    }
}

/// 来源地址属于哪个协议族（按 CIDR 的地址部分判定）。
fn source_is_v6(source: &str) -> bool {
    let ip_part = source.trim().split('/').next().unwrap_or("");
    matches!(
        ip_part.trim().parse::<std::net::IpAddr>(),
        Ok(std::net::IpAddr::V6(_))
    )
}

/// 读链内容（用于判断跳转规则是否已存在）。失败当空串处理。
fn nft_chain_text() -> String {
    run("nft", &["list", "chain", NFT_FAMILY, NFT_TABLE, "input"]).unwrap_or_default()
}

/// 确保表 / 链 / 集合 / 跳转规则齐备，并修复历史重复规则。**调用方须已持有 [`nft_lock`]**。
fn nft_ensure_locked() -> Result<()> {
    if run("nft", &["list", "table", NFT_FAMILY, NFT_TABLE]).is_err() {
        // add 对并发竞争者可能失败（已存在），用 `let _` 容忍：真正的成功判据是下面的 list
        let _ = run("nft", &["add", "table", NFT_FAMILY, NFT_TABLE]);
        let _ = run(
            "nft",
            &[
                "add",
                "chain",
                NFT_FAMILY,
                NFT_TABLE,
                "input",
                "{ type filter hook input priority 0; policy accept; }",
            ],
        );
        if run("nft", &["list", "table", NFT_FAMILY, NFT_TABLE]).is_err() {
            return Err(anyhow!("创建 nftables 表 inet {NFT_TABLE} 失败"));
        }
    }

    for proto in [Proto::Tcp, Proto::Udp] {
        let set = nft_set_name(proto);
        if run("nft", &["list", "set", NFT_FAMILY, NFT_TABLE, set]).is_err() {
            let _ = run(
                "nft",
                &["add", "set", NFT_FAMILY, NFT_TABLE, set, "{ type inet_service; }"],
            );
        }
        for v6 in [false, true] {
            let set = nft_addr_set_name(proto, v6);
            if run("nft", &["list", "set", NFT_FAMILY, NFT_TABLE, set]).is_err() {
                let _ = run(
                    "nft",
                    &[
                        "add",
                        "set",
                        NFT_FAMILY,
                        NFT_TABLE,
                        set,
                        // interval：元素可能带 CIDR 前缀（10.0.0.0/8），无此标志 add element 会拒
                        if v6 {
                            "{ type ipv6_addr . inet_service; flags interval; }"
                        } else {
                            "{ type ipv4_addr . inet_service; flags interval; }"
                        },
                    ],
                );
            }
        }
    }

    // 历史版本的并发 bug 会留下重复跳转规则：发现重复就整链重建。
    // 链里只有 opx 自己的规则，flush 是安全的；集合元素不受 flush 影响。
    let needles: Vec<String> = [Proto::Tcp, Proto::Udp]
        .iter()
        .flat_map(|p| {
            let port = nft_set_name(*p);
            [
                format!("@{port}"),
                format!("@{}", nft_addr_set_name(*p, false)),
                format!("@{}", nft_addr_set_name(*p, true)),
            ]
        })
        .collect();
    let chain = nft_chain_text();
    let dup = |n: &str| chain.matches(n).count() > 1;
    if needles.iter().any(|n| dup(n)) {
        tracing::warn!("nftables 存在重复跳转规则，重建 inet opx 链");
        run("nft", &["flush", "chain", NFT_FAMILY, NFT_TABLE, "input"])?;
    }

    // 补齐缺失的跳转规则（首次建表后 / 刚 flush 后 会走到这里）
    let chain = nft_chain_text();
    for proto in [Proto::Tcp, Proto::Udp] {
        // 用「列表 @集合」而不是「tcp dport」判断：前者只出现在 opx 自己的规则里
        let port_set = nft_set_name(proto);
        if !chain.contains(&format!("@{port_set}")) {
            run(
                "nft",
                &[
                    "add",
                    "rule",
                    NFT_FAMILY,
                    NFT_TABLE,
                    "input",
                    proto.as_str(),
                    "dport",
                    format!("@{port_set}").as_str(),
                    "accept",
                ],
            )?;
        }
        // 来源限定规则：`ip saddr . tcp dport @tcp_src accept`（ip6 同理）
        for (v6, family) in [(false, "ip"), (true, "ip6")] {
            let set = nft_addr_set_name(proto, v6);
            if !chain.contains(&format!("@{set}")) {
                run(
                    "nft",
                    &[
                        "add",
                        "rule",
                        NFT_FAMILY,
                        NFT_TABLE,
                        "input",
                        family,
                        "saddr",
                        ".",
                        proto.as_str(),
                        "dport",
                        format!("@{set}").as_str(),
                        "accept",
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// 确保表结构齐备并修复重复规则（加锁入口）。
fn nft_ensure() -> Result<()> {
    let _g = nft_lock();
    nft_ensure_locked()
}

fn nft_list() -> Result<Vec<PortRule>> {
    let _g = nft_lock();
    let mut out = Vec::new();
    for proto in [Proto::Tcp, Proto::Udp] {
        // 表或集合还没建 → 视为「没有放行」，不是错误（首次打开端口页即此状态）
        // 全网端口集
        if let Ok(text) = run("nft", &["list", "set", NFT_FAMILY, NFT_TABLE, nft_set_name(proto)]) {
            for port in parse_nft_elements(&text) {
                out.push(PortRule { port, proto, source: None });
            }
        }
        // 来源限定集（IPv4 / IPv6）
        for v6 in [false, true] {
            let set = nft_addr_set_name(proto, v6);
            if let Ok(text) = run("nft", &["list", "set", NFT_FAMILY, NFT_TABLE, set]) {
                for (src, port) in parse_nft_addr_elements(&text) {
                    out.push(PortRule { port, proto, source: Some(src) });
                }
            }
        }
    }
    Ok(out)
}

fn nft_add(rule: &PortRule) -> Result<()> {
    let _g = nft_lock();
    nft_ensure_locked()?;
    match &rule.source {
        Some(src) => {
            let set = nft_addr_set_name(rule.proto, source_is_v6(src));
            let elem = format!("{{ {} . {} }}", src.trim(), rule.port);
            run("nft", &["add", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()])?;
        }
        None => {
            let set = nft_set_name(rule.proto);
            let elem = format!("{{ {} }}", rule.port);
            run("nft", &["add", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()])?;
        }
    }
    Ok(())
}

fn nft_remove(rule: &PortRule) -> Result<()> {
    let _g = nft_lock();
    match &rule.source {
        Some(src) => {
            let set = nft_addr_set_name(rule.proto, source_is_v6(src));
            let elem = format!("{{ {} . {} }}", src.trim(), rule.port);
            run(
                "nft",
                &["delete", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()],
            )?;
        }
        None => {
            let set = nft_set_name(rule.proto);
            let elem = format!("{{ {} }}", rule.port);
            run(
                "nft",
                &["delete", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()],
            )?;
        }
    }
    Ok(())
}

/// 从 `nft list set inet opx tcp_ports` 的输出里取端口元素。
///
/// 输出形如：
/// ```text
/// table inet opx {
/// 	set tcp_ports {
/// 		type inet_service
/// 		flags interval
/// 		elements = { 8080, 3306 }
/// 	}
/// }
/// ```
/// 无 `elements` 行（空集合）返回空；区间写法（`8000-8100`）解析失败会被跳过。
pub fn parse_nft_elements(text: &str) -> Vec<u16> {
    let Some(pos) = text.find("elements") else {
        return Vec::new();
    };
    let rest = &text[pos..];
    let Some(open) = rest.find('{') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find('}') else {
        return Vec::new();
    };
    rest[open + 1..open + close]
        .split(',')
        .filter_map(|s| s.trim().parse::<u16>().ok())
        .collect()
}

/// 解析来源拼接集合（`tcp_src` 等）的元素：`192.168.200.1 . 3306`。
///
/// 输出形如：
/// ```text
/// set tcp_src {
///     type ipv4_addr . inet_service
///     flags interval
///     elements = { 192.168.200.1 . 3306, 10.0.0.0/8 . 5432 }
/// }
/// ```
/// 地址与端口之间用 ` . ` 分隔；IPv4 地址本身含 `.`，必须从**最后一个点**切开
/// （IPv6 地址无点，IPv4-mapped IPv6 `::ffff:a.b.c.d` 的最后一个点也是分隔符）。
/// 解析失败的元素被跳过。
pub fn parse_nft_addr_elements(text: &str) -> Vec<(String, u16)> {
    let Some(pos) = text.find("elements") else {
        return Vec::new();
    };
    let rest = &text[pos..];
    let Some(open) = rest.find('{') else {
        return Vec::new();
    };
    let Some(close) = rest[open..].find('}') else {
        return Vec::new();
    };
    rest[open + 1..open + close]
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            let (addr, port) = s.rsplit_once('.')?;
            let addr = addr.trim();
            if addr.is_empty() {
                return None;
            }
            Some((addr.to_string(), port.trim().parse::<u16>().ok()?))
        })
        .collect()
}

// ===================== 解析（纯函数，可单测） =====================

/// 解析 `ufw show added` 的输出。
///
/// 形如：
/// ```text
/// Added user rules (see 'ufw status' for running firewall):
/// ufw allow 22/tcp
/// ufw allow 3306/tcp
/// ufw allow 53/udp
/// ufw allow 8080/tcp comment 'opx-web'
/// ufw deny 23/tcp
/// ufw allow from 10.0.0.0/8 to any port 5432 proto tcp
/// ```
/// 收两种形态（都是 opx 自己写入的）：
/// - `ufw allow <port>/<proto>`（全网放行）；
/// - `ufw allow from <src> to any port <port> proto <proto>`（来源限定）。
///
/// 不收：`deny`（opx 从不添加）；无协议的 `ufw allow 3306`（同时开 tcp+udp，
/// revoke 时用错误语法会删错）；`from` 行缺 `proto` 的同理跳过。
pub fn parse_ufw_added(text: &str) -> Vec<PortRule> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("ufw allow ") else { continue };
        if let Some(rule) = parse_ufw_rule(rest) {
            if !out.contains(&rule) {
                out.push(rule);
            }
        }
    }
    out
}

/// `ufw allow ` 之后的文本 → 规则。
fn parse_ufw_rule(rest: &str) -> Option<PortRule> {
    let tokens = rest.split_whitespace().collect::<Vec<_>>();
    match tokens.first().copied() {
        // `from <src> to any port <port> proto <proto>`（必须先于端口形态判断：
        // "from" 不含 '='，若先走端口臂会被 parse_port_token 拒掉）
        Some("from") => {
            if tokens.len() != 8 || tokens[2] != "to" || tokens[3] != "any" || tokens[4] != "port" {
                return None;
            }
            let source = tokens[1].trim().to_string();
            let port = tokens[5].parse::<u16>().ok()?;
            let proto = Proto::parse(tokens[7])?;
            (port != 0).then_some(PortRule { port, proto, source: Some(source) })
        }
        // `3306/tcp`（后面可能跟 comment '…'，忽略）
        Some(token) if !token.contains('=') => {
            let rule = parse_port_token(token)?;
            // 第一个 token 是 `port/proto` 形态时，如果还有 from 子句（用户手敲的
            // 混合写法），语义不明确，宁可不收
            if tokens.iter().any(|t| *t == "from") {
                return None;
            }
            Some(rule)
        }
        _ => None,
    }
}

/// 解析 `firewall-cmd --permanent --list-ports` 的输出：`"3306/tcp 8080/tcp"`。
pub fn parse_port_list(text: &str) -> Vec<PortRule> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        if let Some(rule) = parse_port_token(token) {
            if !out.contains(&rule) {
                out.push(rule);
            }
        }
    }
    out
}

/// 解析 `firewall-cmd --permanent --list-rich-rules` 的输出（每行一条富规则）。
///
/// 只收 opx 写入的形态：`rule family="ipv4" source address="1.2.3.4" port port="3306" protocol="tcp" accept`。
/// 解析失败 / 缺字段的行跳过。
pub fn parse_firewalld_rich_rules(text: &str) -> Vec<PortRule> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains("accept") {
            continue;
        }
        let grab = |key: &str| -> Option<String> {
            let start = line.find(key)? + key.len();
            let rest = &line[start..];
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        };
        let (Some(source), Some(port), Some(proto)) = (
            grab("source address=\""),
            grab("port port=\"").and_then(|p| p.parse::<u16>().ok()),
            grab("protocol=\"").and_then(|p| Proto::parse(&p)),
        ) else {
            continue;
        };
        if port != 0 {
            out.push(PortRule { port, proto, source: Some(source) });
        }
    }
    out
}

/// 解析 `netsh advfirewall firewall show rule name=all dir=in` 的输出，挑出 opx 加的规则。
///
/// 每条规则是一段「键: 值」块，规则名行为 `Rule Name:  opx-3306-tcp`
/// （中文系统为 `规则名:  opx-3306-tcp`）。只认 `opx-<port>-<tcp|udp>` 命名，
/// 避免把用户自己加的规则纳入 opx 的撤销范围。
pub fn parse_netsh_rules(text: &str) -> Vec<PortRule> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        if key != "Rule Name" && key != "规则名" && key != "規則名" {
            continue;
        }
        // 严格匹配 opx-<port>-<proto>[-src-<来源>]，多一字少一字都不认
        if let Some(rule) = parse_windows_rule_name(value.trim()) {
            if !out.contains(&rule) {
                out.push(rule);
            }
        }
    }
    out
}

/// `"8080/tcp"` → [`PortRule`]；形态不符返回 None。
fn parse_port_token(token: &str) -> Option<PortRule> {
    let (port_s, proto_s) = token.split_once('/')?;
    let port: u16 = port_s.trim().parse().ok()?;
    if port == 0 {
        return None;
    }
    let proto = Proto::parse(proto_s.trim())?;
    Some(PortRule { port, proto, source: None })
}

// ===================== 命令执行 =====================

/// 执行外部命令并合并 stdout+stderr 返回。
///
/// 合并的原因：不同后端把提示写在不同的流上（ufw 写 stdout、netsh 写 stderr），
/// 只看一个流会得到「退出码非零但没有任何信息」的报错，用户无法自救。
fn run(program: &str, args: &[&str]) -> Result<String> {
    let out = hidden(program)
        .args(args)
        .output()
        .map_err(|e| anyhow!("无法执行 `{program}`：{e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let merged = format!("{stdout}{stderr}");
        let tail = merged
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" / ");
        return Err(anyhow!(
            "`{program} {}` 失败（退出码 {:?}）：{}",
            args.join(" "),
            out.status.code(),
            if tail.is_empty() { "无输出".to_string() } else { tail }
        ));
    }
    Ok(format!("{stdout}{stderr}"))
}

fn command_ok(program: &str, args: &[&str]) -> bool {
    hidden(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 执行命令并按「不管退出码」返回合并输出。
///
/// 探测状态必须用这个：`ufw status`、`firewall-cmd --state` 在「未启用」时退出码
/// 非零，但输出里正带着我们要的状态；用 [`run`] 会把这句话当成错误丢掉。
fn run_raw(program: &str, args: &[&str]) -> Option<String> {
    let out = hidden(program).args(args).output().ok()?;
    let mut merged = String::from_utf8_lossy(&out.stdout).into_owned();
    merged.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ufw_added_plain_and_sourced_forms() {
        let sample = "Added user rules (see 'ufw status' for running firewall):\n\
ufw allow 22/tcp\n\
ufw allow 3306/tcp\n\
ufw allow 53/udp\n\
ufw allow 8080/tcp comment 'opx-web'\n\
ufw deny 23/tcp\n\
ufw allow from 10.0.0.0/8 to any port 5432 proto tcp\n\
ufw allow from 192.168.200.1 to any port 6379 proto udp\n\
ufw allow from fe80::1/128 to any port 9100 proto tcp\n\
ufw allow 3307\n\
ufw allow 8080/tcp\n";
        let got = parse_ufw_added(sample);
        assert_eq!(
            got,
            vec![
                PortRule::tcp(22),
                PortRule::tcp(3306),
                PortRule { port: 53, proto: Proto::Udp, source: None },
                PortRule::tcp(8080),
                PortRule::sourced(5432, Proto::Tcp, "10.0.0.0/8"),
                PortRule::sourced(6379, Proto::Udp, "192.168.200.1"),
                PortRule::sourced(9100, Proto::Tcp, "fe80::1/128"),
            ],
            "comment 形态要收；deny/无协议的 from 行/裸端口/重复都不收"
        );
    }

    #[test]
    fn validate_source_accepts_ip_and_cidr_only() {
        assert!(validate_source("192.168.200.1").is_ok());
        assert!(validate_source("10.0.0.0/8").is_ok());
        assert!(validate_source("fe80::1/128").is_ok());
        assert!(validate_source("::1").is_ok());
        // 非法：域名、缺前缀数字、超范围前缀、多余字符
        assert!(validate_source("example.com").is_err());
        assert!(validate_source("10.0.0.0/33").is_err());
        assert!(validate_source("::1/129").is_err());
        assert!(validate_source("192.168.1.1a").is_err());
        assert!(validate_source("10.0.0.0/8 extra").is_err());
    }

    #[test]
    fn parses_nft_addr_set_elements() {
        let sample = "table inet opx {\n\
\tset tcp_src {\n\
\t\ttype ipv4_addr . inet_service\n\
\t\tflags interval\n\
\t\telements = { 192.168.200.1 . 3306, 10.0.0.0/8 . 5432 }\n\
\t}\n\
}";
        assert_eq!(
            parse_nft_addr_elements(sample),
            vec![("192.168.200.1".to_string(), 3306), ("10.0.0.0/8".to_string(), 5432)]
        );
        // IPv6 地址不含 '.'，可安全分离
        let v6 = "elements = { fe80::1 . 9100 }";
        assert_eq!(parse_nft_addr_elements(v6), vec![("fe80::1".to_string(), 9100)]);
        assert!(parse_nft_addr_elements("no elements here").is_empty());
    }

    #[test]
    fn ufw_args_cover_both_rule_shapes() {
        let plain = PortRule::tcp(3306);
        assert_eq!(ufw_args("allow", &plain), vec!["allow", "3306/tcp"]);
        let sourced = PortRule::sourced(5432, Proto::Tcp, "10.0.0.0/8");
        assert_eq!(
            ufw_args("delete", &sourced),
            vec!["delete", "allow", "from", "10.0.0.0/8", "to", "any", "port", "5432", "proto", "tcp"]
        );
        assert_eq!(
            ufw_args("allow", &sourced),
            vec!["allow", "from", "10.0.0.0/8", "to", "any", "port", "5432", "proto", "tcp"]
        );
    }

    #[test]
    fn windows_rule_name_roundtrips_source() {
        let sourced = PortRule::sourced(3306, Proto::Tcp, "192.168.200.1/32");
        let name = windows_rule_name(&sourced);
        assert_eq!(name, "opx-3306-tcp-src-192.168.200.1_32");
        assert_eq!(parse_windows_rule_name(&name), Some(sourced));
        // 全网规则不带 src 段
        let plain = PortRule::tcp(17580);
        assert_eq!(parse_windows_rule_name(&windows_rule_name(&plain)), Some(plain));
    }

    #[test]
    fn firewalld_rich_rule_roundtrip() {
        let sourced = PortRule::sourced(3306, Proto::Tcp, "10.0.0.0/8");
        let rich = firewalld_rich_rule(&sourced, "10.0.0.0/8");
        assert_eq!(
            rich,
            "rule family=ipv4 source address=10.0.0.0/8 port port=3306 protocol=tcp accept"
        );
        // firewalld --list-rich-rules 的真实输出：值带双引号
        let listed = r#"rule family="ipv4" source address="10.0.0.0/8" port port="3306" protocol="tcp" accept"#;
        assert_eq!(parse_firewalld_rich_rules(listed), vec![sourced]);
        let v6 = PortRule::sourced(9100, Proto::Tcp, "fe80::1");
        assert!(firewalld_rich_rule(&v6, "fe80::1").contains("family=ipv6"));
        // IPv6 CIDR 同样必须 ipv6（曾因 IpAddr::parse 不接受 "/" 而误判成 ipv4）
        let v6cidr = PortRule::sourced(9100, Proto::Tcp, "2001:db8::/32");
        assert!(firewalld_rich_rule(&v6cidr, "2001:db8::/32").contains("family=ipv6"));
    }

    #[test]
    fn parses_firewalld_port_list() {
        assert_eq!(
            parse_port_list("3306/tcp 8080/tcp 53/udp\n"),
            vec![
                PortRule::tcp(3306),
                PortRule::tcp(8080),
                PortRule { port: 53, proto: Proto::Udp, source: None },
            ]
        );
        // 空输出（没有任何放行规则）不应崩
        assert!(parse_port_list("").is_empty());
        // 畸形 token 跳过
        assert!(parse_port_list("abc 0/tcp 80/sctp").is_empty());
    }

    #[test]
    fn parses_netsh_rules_only_opx_named() {
        let sample = "\
规则名:                             opx-3306-tcp
已启用:                             是
方向:                               入
操作:                               允许
规则名:                             opx-53-udp
已启用:                             是
规则名:                             MyOwnRule
已启用:                             是
规则名:                             opx-notaport-tcp
";
        assert_eq!(
            parse_netsh_rules(sample),
            vec![
                PortRule::tcp(3306),
                PortRule { port: 53, proto: Proto::Udp, source: None },
            ],
            "只认 opx-<port>-<proto>，用户自建规则与畸形名都不纳入"
        );
    }

    #[test]
    fn parse_netsh_rules_handles_english_output() {
        let sample = "Rule Name:                            opx-17580-tcp\nEnabled:                              Yes\n";
        assert_eq!(parse_netsh_rules(sample), vec![PortRule::tcp(17580)]);
    }

    /// `netsh advfirewall firewall show rule` 的 `name=` 是必填参数（微软文档
    /// "Required"）；只传 `dir=in` 在 Windows 11 上直接报「一个或多个重要的参数没有输入」，
    /// 整个防火墙页读不到任何规则。此测试锁定参数里必须含 `name=all`。
    #[test]
    fn netsh_show_rule_includes_required_name_all() {
        let args = super::NETSH_LIST_RULES_ARGS;
        assert!(args.contains(&"name=all"), "show rule 必须传 name=all，实际: {args:?}");
        assert!(args.contains(&"dir=in"), "仍需限定入站规则，实际: {args:?}");
    }

    /// 真机验证（仅 Windows）：修复后的命令能被 netsh 接受；原命令 `dir=in` 会退出码 1。
    #[cfg(windows)]
    #[test]
    fn netsh_show_rule_command_succeeds_on_windows() {
        let rules = super::list_windows_rules();
        assert!(rules.is_ok(), "show rule name=all dir=in 应成功，实际: {rules:?}");
    }

    #[test]
    fn spec_and_proto_roundtrip() {
        assert_eq!(PortRule::tcp(8080).spec(), "8080/tcp");
        assert_eq!(Proto::parse("UDP"), Some(Proto::Udp));
        assert_eq!(Proto::parse("sctp"), None);
    }

    #[test]
    fn rejects_port_zero() {
        assert!(allow(BackendKind::None, &PortRule::tcp(0)).is_err());
        assert!(revoke(BackendKind::None, &PortRule::tcp(0)).is_err());
    }

    /// 后端不支持时必须明确报错，不能静默「成功」——
    /// 否则前端会把未放行的端口显示成已放行。
    #[test]
    fn none_backend_errors_instead_of_silent_success() {
        assert!(list_allowed(BackendKind::None).is_err());
        assert!(allow(BackendKind::None, &PortRule::tcp(3306)).is_err());
        assert!(revoke(BackendKind::None, &PortRule::tcp(3306)).is_err());
    }

    #[test]
    fn info_marks_none_unavailable() {
        let none = Detection { kind: BackendKind::None, reason: Reason::NoTool };
        assert!(!info(none).available);
        assert!(!info(none).active);
        let ufw = Detection { kind: BackendKind::Ufw, reason: Reason::UfwActive };
        assert!(info(ufw).available);
        assert!(info(ufw).active);
        assert_eq!(info(ufw).kind, "ufw");
        assert_eq!(
            info(Detection { kind: BackendKind::Firewalld, reason: Reason::FirewalldStopped }).kind,
            "firewalld"
        );
        assert_eq!(
            info(Detection { kind: BackendKind::Nftables, reason: Reason::Nftables }).kind,
            "nftables"
        );
    }

    /// 判定矩阵：这次改动的全部意义都在这几条上。
    #[test]
    fn decides_by_what_is_actually_running() {
        let only_ufw = Caps { ufw_installed: true, ufw_active: true, ..Default::default() };
        assert_eq!(
            decide(only_ufw, NftState::Skipped),
            Detection { kind: BackendKind::Ufw, reason: Reason::UfwActive }
        );

        let only_fwd = Caps {
            firewalld_installed: true,
            firewalld_active: true,
            ..Default::default()
        };
        assert_eq!(
            decide(only_fwd, NftState::Skipped),
            Detection { kind: BackendKind::Firewalld, reason: Reason::FirewalldRunning }
        );

        // 🔴 核心回归：ufw 装了但没启用，firewalld 在跑 → 必须选 firewalld。
        // 选 ufw 会把规则写进不生效的配置里，用户看到「已放行」却连不上。
        let both = Caps {
            ufw_installed: true,
            ufw_active: false,
            firewalld_installed: true,
            firewalld_active: true,
            ..Default::default()
        };
        assert_eq!(
            decide(both, NftState::Skipped),
            Detection { kind: BackendKind::Firewalld, reason: Reason::FirewalldRunning }
        );

        // 都没生效：ufw 装了就先落盘，启用即生效
        let ufw_off = Caps { ufw_installed: true, ..Default::default() };
        assert_eq!(
            decide(ufw_off, NftState::Skipped),
            Detection { kind: BackendKind::Ufw, reason: Reason::UfwInactive }
        );
        let fwd_off = Caps { firewalld_installed: true, ..Default::default() };
        assert_eq!(
            decide(fwd_off, NftState::Skipped),
            Detection { kind: BackendKind::Firewalld, reason: Reason::FirewalldStopped }
        );

        // 没有任何托管防火墙 → 裸 nftables（仅在无拦截规则时）
        let bare = Caps { nft_installed: true, ..Default::default() };
        assert_eq!(
            decide(bare, NftState::Clean),
            Detection { kind: BackendKind::Nftables, reason: Reason::Nftables }
        );
        assert_eq!(
            decide(bare, NftState::Blocking),
            Detection { kind: BackendKind::None, reason: Reason::NftBlocking }
        );
        assert_eq!(
            decide(bare, NftState::Unreadable),
            Detection { kind: BackendKind::None, reason: Reason::NftUnreadable }
        );
        assert_eq!(
            decide(Caps::default(), NftState::Missing),
            Detection { kind: BackendKind::None, reason: Reason::NoTool }
        );
    }

    /// 「未启用」不能算生效 —— 否则 UI 不会提示「规则待启用后生效」。
    #[test]
    fn inactive_reasons_are_not_active() {
        assert!(!Reason::UfwInactive.active());
        assert!(!Reason::FirewalldStopped.active());
        assert!(!Reason::NoTool.active());
        assert!(Reason::UfwActive.active());
        assert!(Reason::FirewalldRunning.active());
        assert!(Reason::Nftables.active());
    }

    /// 「装没装」和「有没有在拦」必须分开：Debian 13 这类只有 nftables 且规则集为空的
    /// 机器，端口页显示「已放行」但系统根本不拦——不说清楚用户会以为规则失效。
    #[test]
    fn enforcement_reflects_real_blocking() {
        assert_eq!(enforcement_of(Reason::UfwActive, true), Enforcement::Enforced);
        assert_eq!(enforcement_of(Reason::FirewalldRunning, true), Enforcement::Enforced);
        assert_eq!(
            enforcement_of(Reason::Nftables, true),
            Enforcement::BookkeepingOnly,
            "opx 的独立表是 policy accept + 纯 accept，不拦任何入站"
        );
        assert_eq!(enforcement_of(Reason::NftBlocking, true), Enforcement::SystemBlocking);
        assert_eq!(enforcement_of(Reason::UfwInactive, true), Enforcement::Disabled);
        assert_eq!(enforcement_of(Reason::FirewalldStopped, true), Enforcement::Disabled);
        assert_eq!(enforcement_of(Reason::NoTool, true), Enforcement::Disabled);
        assert_eq!(enforcement_of(Reason::NftUnreadable, true), Enforcement::Unknown);
        // Windows 要看三个配置文件的实际开关
        assert_eq!(enforcement_of(Reason::Windows, true), Enforcement::Enforced);
        assert_eq!(enforcement_of(Reason::Windows, false), Enforcement::Disabled);
    }

    #[test]
    fn parses_netsh_profile_state() {
        assert!(parse_netsh_state_on(
            "域配置文件 设置:\n----------------------------------------------------------------------\n状态                                 启用\n"
        ));
        assert!(parse_netsh_state_on(
            "Domain Profile Settings:\nState                                 ON\n"
        ));
        assert!(!parse_netsh_state_on(
            "Domain Profile Settings:\nState                                 OFF\n"
        ));
        assert!(!parse_netsh_state_on(""), "空输出按「没开」处理由调用方决定，解析层返回 false");
    }

    /// Windows 防火墙开关判定必须走进程级缓存：get_firewall_overview 每次
    /// 刷新都会调 info()，而 show allprofiles state 要 spawn 一个 netsh（实测 ~195ms）。
    /// 开关状态与 detection() 同理在进程生命周期内不变，重复 spawn 纯属浪费。
    /// 此测试锁定缓存函数存在且连续调用结果稳定（非 Windows 恒 true）。
    #[test]
    fn windows_firewall_on_is_process_cached() {
        let a = super::windows_firewall_on_cached();
        let b = super::windows_firewall_on_cached();
        assert_eq!(a, b, "开关判定必须进程级稳定，不能每次重新探测");
        if !cfg!(windows) {
            assert!(a, "非 Windows 平台不该探测，直接视为不拦");
        }
    }

    /// 规则表缓存：首次加载后不再重复跑系统命令（Windows 上 netsh 全量约 700ms）。
    #[test]
    fn rules_cache_loads_only_once() {
        use std::cell::Cell;
        let cache = super::RulesCache::default();
        let calls = Cell::new(0);
        let loader = |_kind| {
            calls.set(calls.get() + 1);
            Ok(vec![PortRule::tcp(1234)])
        };
        let a = super::list_allowed_with(BackendKind::Ufw, &cache, &loader).unwrap();
        let b = super::list_allowed_with(BackendKind::Ufw, &cache, &loader).unwrap();
        assert_eq!(a, vec![PortRule::tcp(1234)]);
        assert_eq!(b, a);
        assert_eq!(calls.get(), 1, "第二次必须命中缓存，不能再跑系统命令");
    }

    /// 不同后端各存一份，绝不串味（Windows 与 ufw 规则互不影响）。
    #[test]
    fn rules_cache_isolated_per_kind() {
        use std::cell::Cell;
        let cache = super::RulesCache::default();
        let calls = Cell::new(0);
        let loader = |kind: BackendKind| {
            calls.set(calls.get() + 1);
            Ok(vec![PortRule::tcp(if kind == BackendKind::Ufw { 1111 } else { 2222 })])
        };
        let ufw = super::list_allowed_with(BackendKind::Ufw, &cache, &loader).unwrap();
        let win = super::list_allowed_with(BackendKind::Windows, &cache, &loader).unwrap();
        assert_eq!(ufw, vec![PortRule::tcp(1111)]);
        assert_eq!(win, vec![PortRule::tcp(2222)]);
        assert_eq!(calls.get(), 2, "两个后端各加载一次");
    }

    /// opx 写入后增量维护缓存：allow 加一条、revoke 去一条，且不重复。
    #[test]
    fn rules_cache_tracks_opx_writes() {
        let cache = super::RulesCache::default();
        cache.put(BackendKind::Windows, vec![PortRule::tcp(3306)]);
        let added = PortRule::sourced(3306, Proto::Tcp, "10.0.0.0/8");
        cache.apply_add(BackendKind::Windows, &added);
        cache.apply_add(BackendKind::Windows, &added); // 幂等
        let rules = cache.get(BackendKind::Windows).unwrap();
        assert_eq!(rules, vec![PortRule::tcp(3306), added.clone()]);

        cache.apply_remove(BackendKind::Windows, &PortRule::tcp(3306));
        let rules = cache.get(BackendKind::Windows).unwrap();
        assert_eq!(rules, vec![added.clone()], "撤销全网规则不影响来源规则");

        cache.apply_remove(BackendKind::Windows, &added);
        assert!(cache.get(BackendKind::Windows).unwrap().is_empty());
    }

    /// 状态解析：真实输出形态（含非 root 的报错文本）都要能吃。
    #[test]
    fn parses_firewall_states() {
        assert!(parse_ufw_status("Status: active\nLogging: on (low)\n"));
        assert!(!parse_ufw_status("Status: inactive\n"));
        // 非 root 的报错里没有 Status 行
        assert!(!parse_ufw_status("ERROR: You need to be root to run this script\n"));
        assert!(parse_ufw_conf("# comment\nENABLED=yes\n"));
        assert!(!parse_ufw_conf("ENABLED=no\n"));
        assert!(!parse_ufw_conf("#ENABLED=yes\n"), "注释掉的不能算启用");
        assert!(parse_firewalld_state("running\n"));
        assert!(!parse_firewalld_state("not running\n"));
        assert!(
            parse_firewalld_state("Authorization failed.\nEnsure polkit is running.\nrunning\n"),
            "合并了 stderr 告警行时仍要能认出 running"
        );
    }

    #[test]
    fn parses_nft_elements() {
        let sample = "table inet opx {\n\tset tcp_ports {\n\t\ttype inet_service\n\t\tflags interval\n\t\telements = { 8080, 3306 }\n\t}\n}\n";
        assert_eq!(parse_nft_elements(sample), vec![8080, 3306]);
        // 空集合：没有 elements 行
        let empty = "table inet opx {\n\tset tcp_ports {\n\t\ttype inet_service\n\t}\n}\n";
        assert!(parse_nft_elements(empty).is_empty());
        // 区间写法跳过而不是崩
        assert_eq!(
            parse_nft_elements("elements = { 8080, 8000-8100 }"),
            vec![8080]
        );
        assert!(parse_nft_elements("").is_empty());
    }

    /// 接管裸 nftables 的唯一前提就是「没有拦截规则」——这条判错的后果是
    /// 在已有 drop 策略的机器上以为放行了、实际没放行，或者更糟：动了别人的链。
    #[test]
    fn detects_blocking_rules() {
        assert!(!has_blocking_rules(""));
        assert!(!has_blocking_rules(
            "table inet filter {\n  chain input {\n    type filter hook input priority 0; policy accept;\n  }\n}"
        ));
        // policy drop 是最常见的默认拒绝
        assert!(has_blocking_rules(
            "chain input {\n  type filter hook input priority 0; policy drop;\n}"
        ));
        // 行尾 drop / 独立 reject 行
        assert!(has_blocking_rules("ip saddr 10.0.0.0/8 drop"));
        assert!(has_blocking_rules("  reject with icmp type port-unreachable"));
        // 注释行不算
        assert!(!has_blocking_rules("# policy drop 是不允许的\n policy accept;"));
    }
}
