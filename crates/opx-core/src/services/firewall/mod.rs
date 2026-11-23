//! 防火墙端口规则维护（跨平台：ufw / firewalld / Windows 防火墙）。
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
use std::sync::OnceLock;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PortRule {
    pub port: u16,
    pub proto: Proto,
}

impl PortRule {
    pub fn tcp(port: u16) -> Self {
        Self { port, proto: Proto::Tcp }
    }

    /// ufw / firewalld 通用的 `8080/tcp` 形态。
    pub fn spec(&self) -> String {
        format!("{}/{}", self.port, self.proto.as_str())
    }
}

/// 实际可用的防火墙后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// 给前端展示的后端状态。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendInfo {
    /// "ufw" | "firewalld" | "windows" | "none"
    pub kind: String,
    /// 是否可自动维护规则（false 时前端只显示指引，不显示放行开关）
    pub available: bool,
    /// 面向用户的说明（前端有 i18n 时优先用前端词条，本字段作兜底）
    pub note: String,
}

/// 探测受支持的防火墙后端。
///
/// 探测只看「命令是否存在」，**不看防火墙是否已启用** —— 见模块文档第 1 条。
/// Linux 上 ufw 优先于 firewalld（同机并存时 ufw 通常是实际生效的那层；
/// 两者同时 active 属用户配置异常，按其首选处理）。
pub fn detect() -> BackendKind {
    if cfg!(windows) {
        return BackendKind::Windows;
    }
    if cfg!(target_os = "macos") {
        // macOS 的 Application Firewall 没有端口放行的命令行接口，不纳入支持
        return BackendKind::None;
    }
    // `ufw --version` 不需要 root（`ufw show added` 需要），用它判存在性
    if command_ok("ufw", &["--version"]) {
        return BackendKind::Ufw;
    }
    if command_ok("firewall-cmd", &["--version"]) {
        return BackendKind::Firewalld;
    }
    // Debian 13 / 精简系统：只有 nftables。见模块文档第 2 条的安全条件。
    if command_ok("nft", &["--version"]) {
        match run("nft", &["list", "ruleset"]) {
            // 规则集为空或只有 accept：独立表是纯加法，安全接管
            Ok(text) if !has_blocking_rules(&text) => return BackendKind::Nftables,
            // 已有 drop/reject（或读不到，无法确认安全）→ 交给人工
            _ => return BackendKind::None,
        }
    }
    BackendKind::None
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
pub fn backend() -> BackendKind {
    static CACHE: OnceLock<BackendKind> = OnceLock::new();
    *CACHE.get_or_init(detect)
}

/// 后端信息（供 UI 展示）。
pub fn info(kind: BackendKind) -> BackendInfo {
    let (available, note) = match kind {
        BackendKind::Ufw => (
            true,
            "已检测到 ufw。opx 只维护端口规则，不会启用或停用 ufw。",
        ),
        BackendKind::Firewalld => (
            true,
            "已检测到 firewalld。opx 只维护端口规则，不会启动或停止 firewalld。",
        ),
        BackendKind::Nftables => (
            true,
            "已检测到 nftables（无自定义拦截规则）。opx 在独立表 inet opx 中维护放行端口，不修改系统现有规则；规则随 opx 启动自动重建。",
        ),
        BackendKind::Windows => (
            true,
            "已检测到 Windows 防火墙。放行端口需要以管理员身份运行 opx。",
        ),
        BackendKind::None => (
            false,
            "未检测到 ufw 或 firewalld。请在系统防火墙中手动放行端口；opx 不会改动 iptables/nftables 裸规则。",
        ),
    };
    BackendInfo {
        kind: kind.as_str().to_string(),
        available,
        note: note.to_string(),
    }
}

/// 列出当前已放行的端口。
///
/// 口径差异说明：ufw/firewalld 读的是**持久配置层**（`ufw show added` /
/// `firewall-cmd --permanent --list-ports`），与「是否已 enable/start」无关，
/// 与 [`allow`] 写入的层一致 —— 这样查询结果与实际维护的规则永远同源。
pub fn list_allowed(kind: BackendKind) -> Result<Vec<PortRule>> {
    match kind {
        BackendKind::Ufw => {
            let text = run("ufw", &["show", "added"])?;
            Ok(parse_ufw_added(&text))
        }
        BackendKind::Firewalld => {
            let text = run("firewall-cmd", &["--permanent", "--list-ports"])?;
            Ok(parse_port_list(&text))
        }
        BackendKind::Windows => list_windows_rules(),
        BackendKind::Nftables => nft_list(),
        BackendKind::None => Err(anyhow!("未检测到受支持的防火墙后端")),
    }
}

/// 放行端口。已放行时返回 `Ok(false)`（幂等）。
pub fn allow(kind: BackendKind, rule: PortRule) -> Result<bool> {
    validate(rule)?;
    // 先查：避免重复写入（ufw/firewalld 的重复添加多数是幂等的，但 Windows
    // 会累积同名规则，必须先判）
    if list_allowed(kind)?.contains(&rule) {
        return Ok(false);
    }
    match kind {
        BackendKind::Ufw => {
            run("ufw", &["allow", &rule.spec()])?;
            Ok(true)
        }
        BackendKind::Firewalld => {
            run("firewall-cmd", &["--permanent", &format!("--add-port={}", rule.spec())])?;
            reload_firewalld_best_effort();
            Ok(true)
        }
        BackendKind::Windows => {
            add_windows_rule(rule)?;
            Ok(true)
        }
        BackendKind::Nftables => {
            nft_add(rule)?;
            Ok(true)
        }
        BackendKind::None => Err(anyhow!("未检测到受支持的防火墙后端，无法自动放行端口")),
    }
}

/// 撤销放行。本就没有该规则时返回 `Ok(false)`（幂等）。
pub fn revoke(kind: BackendKind, rule: PortRule) -> Result<bool> {
    validate(rule)?;
    if !list_allowed(kind)?.contains(&rule) {
        return Ok(false);
    }
    match kind {
        BackendKind::Ufw => {
            run("ufw", &["delete", "allow", &rule.spec()])?;
            Ok(true)
        }
        BackendKind::Firewalld => {
            run("firewall-cmd", &["--permanent", &format!("--remove-port={}", rule.spec())])?;
            reload_firewalld_best_effort();
            Ok(true)
        }
        BackendKind::Windows => {
            delete_windows_rule(rule)?;
            Ok(true)
        }
        BackendKind::Nftables => {
            nft_remove(rule)?;
            Ok(true)
        }
        BackendKind::None => Err(anyhow!("未检测到受支持的防火墙后端，无法撤销端口放行")),
    }
}

/// 批量放行（用于安装时按 provider 声明的全部端口一次放行）。
///
/// 返回成功放行的端口列表；**单个端口失败不影响其余**，失败项进 `errors`。
/// 调用方只记日志，不阻断安装。
pub fn allow_many(kind: BackendKind, rules: &[PortRule]) -> (Vec<PortRule>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for rule in rules {
        match allow(kind, *rule) {
            Ok(_) => ok.push(*rule),
            Err(e) => errors.push(format!("{}: {e}", rule.spec())),
        }
    }
    (ok, errors)
}

fn validate(rule: PortRule) -> Result<()> {
    if rule.port == 0 {
        return Err(anyhow!("端口不能为 0"));
    }
    Ok(())
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
        .filter_map(|p| revoke(kind, PortRule::tcp(*p)).err().map(|e| format!("{p}/tcp: {e}")))
        .collect()
}

// ===================== 后端实现 =====================

fn reload_firewalld_best_effort() {
    // firewalld 未运行时 `--reload` 会失败，但 `--permanent` 已写入配置，
    // 服务下次启动会自动带上 —— 所以这里只尝试，不因失败而回滚。
    let _ = run("firewall-cmd", &["--reload"]);
}

fn windows_rule_name(rule: PortRule) -> String {
    format!("opx-{}-{}", rule.port, rule.proto.as_str())
}

fn list_windows_rules() -> Result<Vec<PortRule>> {
    // 用 `show rule name=opx-*` 不支持的缘故，只能全量列举再过滤；
    // 加 `verbose` 太慢，这里用默认输出（含 Rule Name 行）。
    let text = run("netsh", &["advfirewall", "firewall", "show", "rule", "dir=in"])?;
    Ok(parse_netsh_rules(&text))
}

fn add_windows_rule(rule: PortRule) -> Result<()> {
    let name = windows_rule_name(rule);
    let proto = if rule.proto == Proto::Tcp { "TCP" } else { "UDP" };
    run(
        "netsh",
        &[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={name}"),
            "dir=in",
            "action=allow",
            "profile=any",
            &format!("protocol={proto}"),
            &format!("localport={}", rule.port),
        ],
    )
    .map(|_| ())
    .map_err(|e| {
        // 最常见原因：非管理员运行。把 netsh 的原始报错补一句可操作提示。
        anyhow!("{e}（Windows 下放行端口需要以管理员身份运行 opx）")
    })
}

fn delete_windows_rule(rule: PortRule) -> Result<()> {
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
    }

    // 历史版本的并发 bug 会留下重复跳转规则：发现重复就整链重建。
    // 链里只有 opx 自己的两条规则，flush 是安全的；集合元素不受 flush 影响。
    let chain = nft_chain_text();
    let tcp_n = chain.matches("@tcp_ports").count();
    let udp_n = chain.matches("@udp_ports").count();
    if tcp_n > 1 || udp_n > 1 {
        tracing::warn!(
            tcp_rules = tcp_n,
            udp_rules = udp_n,
            "nftables 存在重复跳转规则，重建 inet opx 链"
        );
        run("nft", &["flush", "chain", NFT_FAMILY, NFT_TABLE, "input"])?;
    }

    // 补齐缺失的跳转规则（首次建表后 / 刚 flush 后 会走到这里）
    let chain = nft_chain_text();
    for proto in [Proto::Tcp, Proto::Udp] {
        let set = nft_set_name(proto);
        let needle = format!("@{set}");
        // 用「列表 @集合」而不是「tcp dport」判断：前者只出现在 opx 自己的规则里
        if !chain.contains(&needle) {
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
                    needle.as_str(),
                    "accept",
                ],
            )?;
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
        let set = nft_set_name(proto);
        // 表或集合还没建 → 视为「没有放行」，不是错误（首次打开端口页即此状态）
        let Ok(text) = run("nft", &["list", "set", NFT_FAMILY, NFT_TABLE, set]) else {
            continue;
        };
        for port in parse_nft_elements(&text) {
            out.push(PortRule { port, proto });
        }
    }
    Ok(out)
}

fn nft_add(rule: PortRule) -> Result<()> {
    let _g = nft_lock();
    nft_ensure_locked()?;
    let set = nft_set_name(rule.proto);
    let elem = format!("{{ {} }}", rule.port);
    run("nft", &["add", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()])?;
    Ok(())
}

fn nft_remove(rule: PortRule) -> Result<()> {
    let _g = nft_lock();
    let set = nft_set_name(rule.proto);
    let elem = format!("{{ {} }}", rule.port);
    run(
        "nft",
        &["delete", "element", NFT_FAMILY, NFT_TABLE, set, elem.as_str()],
    )?;
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
/// 只收 `ufw allow <port>/<proto>` 这一种形态：
/// - `deny` 不处理（opx 从不添加 deny 规则）；
/// - 带来源的 `from ... to any port N proto tcp` 不收 —— 它们不是 opx 加的，
///   收进来会导致 [`revoke`] 用错误语法删除而失败；
/// - 无协议的 `ufw allow 3306`（同时开 tcp+udp）不收，同理避免误删。
pub fn parse_ufw_added(text: &str) -> Vec<PortRule> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("ufw allow ") else { continue };
        // 第一个 token 必须是 `<port>/<proto>`，否则不是我们维护的形态
        let token = rest.split_whitespace().next().unwrap_or("");
        if let Some(rule) = parse_port_token(token) {
            if !out.contains(&rule) {
                out.push(rule);
            }
        }
    }
    out
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

/// 解析 `netsh advfirewall firewall show rule dir=in` 的输出，挑出 opx 加的规则。
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
        let name = value.trim();
        // 严格匹配 opx-<port>-<proto>，多一字少一字都不认
        let Some(rest) = name.strip_prefix("opx-") else { continue };
        let Some((port_s, proto_s)) = rest.rsplit_once('-') else { continue };
        let (Ok(port), Some(proto)) = (port_s.parse::<u16>(), Proto::parse(proto_s)) else {
            continue;
        };
        let rule = PortRule { port, proto };
        if !out.contains(&rule) {
            out.push(rule);
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
    Some(PortRule { port, proto })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ufw_added_only_plain_allow_forms() {
        let sample = "Added user rules (see 'ufw status' for running firewall):\n\
ufw allow 22/tcp\n\
ufw allow 3306/tcp\n\
ufw allow 53/udp\n\
ufw allow 8080/tcp comment 'opx-web'\n\
ufw deny 23/tcp\n\
ufw allow from 10.0.0.0/8 to any port 5432 proto tcp\n\
ufw allow 3307\n\
ufw allow 8080/tcp\n";
        let got = parse_ufw_added(sample);
        assert_eq!(
            got,
            vec![
                PortRule::tcp(22),
                PortRule::tcp(3306),
                PortRule { port: 53, proto: Proto::Udp },
                PortRule::tcp(8080),
            ],
            "带 comment 的要收（opx 自己加的会带 comment）；deny/带来源/无协议/重复都不收"
        );
    }

    #[test]
    fn parses_firewalld_port_list() {
        assert_eq!(
            parse_port_list("3306/tcp 8080/tcp 53/udp\n"),
            vec![
                PortRule::tcp(3306),
                PortRule::tcp(8080),
                PortRule { port: 53, proto: Proto::Udp },
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
                PortRule { port: 53, proto: Proto::Udp },
            ],
            "只认 opx-<port>-<proto>，用户自建规则与畸形名都不纳入"
        );
    }

    #[test]
    fn parse_netsh_rules_handles_english_output() {
        let sample = "Rule Name:                            opx-17580-tcp\nEnabled:                              Yes\n";
        assert_eq!(parse_netsh_rules(sample), vec![PortRule::tcp(17580)]);
    }

    #[test]
    fn spec_and_proto_roundtrip() {
        assert_eq!(PortRule::tcp(8080).spec(), "8080/tcp");
        assert_eq!(Proto::parse("UDP"), Some(Proto::Udp));
        assert_eq!(Proto::parse("sctp"), None);
    }

    #[test]
    fn rejects_port_zero() {
        assert!(allow(BackendKind::None, PortRule::tcp(0)).is_err());
        assert!(revoke(BackendKind::None, PortRule::tcp(0)).is_err());
    }

    /// 后端不支持时必须明确报错，不能静默「成功」——
    /// 否则前端会把未放行的端口显示成已放行。
    #[test]
    fn none_backend_errors_instead_of_silent_success() {
        assert!(list_allowed(BackendKind::None).is_err());
        assert!(allow(BackendKind::None, PortRule::tcp(3306)).is_err());
        assert!(revoke(BackendKind::None, PortRule::tcp(3306)).is_err());
    }

    #[test]
    fn info_marks_none_unavailable() {
        assert!(!info(BackendKind::None).available);
        assert!(info(BackendKind::Ufw).available);
        assert_eq!(info(BackendKind::Firewalld).kind, "firewalld");
        assert_eq!(info(BackendKind::Nftables).kind, "nftables");
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
