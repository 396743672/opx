# opx 安装引导与首启策略（方案 B 落地设计）

> 状态：草案 v1 · 日期：2026-10-12 · 关联：`2026-10-09-opx-web-architecture.md`（形态 A）、`2026-09-28-cross-platform-readiness.md`
>
> 结论先行：**安装器只负责「装 + 告知入口」，配置一律推迟到首次运行引导**；
> headless（Linux 服务器）首启默认开局域网（否则无入口）、默认端口被占自动降级扫描（否则 setup 页自身不可达）。

---

## 1. 前提修正

「有些设置需要安装前设置」的担忧大部分不成立：

- 代码事实：`web_server::sync()` 中 `enabled: headless || settings.web_enabled` —— headless 下 web 入口**无条件启动**，token 首启自动生成（`token::load_or_generate`）。
- 因此装完后入口天然可达，真正缺的是两件事：
  1. **把 URL + token 告知用户**（安装器收尾 / journal banner）；
  2. **一块首次运行引导页（setup）**。

## 2. 设置分级

| 级别 | 设置 | 时机 |
|---|---|---|
| 入口相关 | `web_port`（默认 17580）、`web_lan_access` | headless 首启种子 + setup 页可改；仅「装完即用且要局域网」才值得进安装器 |
| 安全 | `web_token` | **必须自动生成**，绝不让用户设置；安装器只展示 |
| 其余 | language / theme / mirror_url / alerts / smtp / ddns… | 装后随时在设置页改，默认值即可用 |

## 3. 决策 1：headless 首启默认开局域网（修订原草案 §7 决策 1）

**问题**：Linux 服务器没有浏览器，`127.0.0.1` 绑定对运维工作站毫无意义——不开局域网就没有任何入口。

**决策**：`--headless` 且 `settings.json` **不存在**（真首启）→ 种子写入 `web_lan_access: true`；此后由 setup 页 / 设置页自由改（改回 localhost 时给出 SSH 隧道命令模板）。

- 备选 B2（更保守）：默认 localhost，靠 `ssh -L 17580:127.0.0.1:17580` 隧道进 setup 再开——零暴露增量，但每个用户都要配隧道，与「装完即用」矛盾，仅作为 setup 页里的说明项。
- 备选 B3：安装器交互问一句（debconf / NSIS 页），非交互默认 = 本决策；支持 preseed，自动化部署友好。本期不做，列为开放项。
- **风险面论证**：与桌面用户手动开启局域网是同一风险面（HTTP 明文 + 强随机 token），只是默认值不同。缓解：32 字节 CSPRNG token（已具备）+ setup 页显著风险提示 + TLS 列为后续开放项（原草案 §3.4）。

**实现要点**：种子逻辑放 `setup_app` 启动装配处——仅当 `settings.json` **不存在**时创建；**不得**放在 `read_settings()` 的解析失败分支（那里会覆盖损坏文件）。token 写盘沿用 `paths::write_file_restricted`。

## 4. 决策 2：端口冲突分治（修订 D6 在 headless 的适用性）

**现状（D6）**：绑定失败 → 明确报错、不自动换端口（`web_server.rs:157`），`applied` 保留目标，supervisor 不重试，等用户改设置触发重启。桌面版维持此行为（有 GUI 可自救，静默换端口反而困惑）。

**问题**：方案 B 的入口本身就绑在端口上，headless 报错 = setup 页永远进不去。

**决策（分治）**：

| 模式 | 行为 |
|---|---|
| 桌面 | 维持 D6：报错 + `web-server-error` 事件，用户改设置 |
| headless，端口 = 默认 17580 或上次自动挑选值（`web_port_auto=true`） | **绑定失败后逐个向上重试到 17599**；实际端口回写 `settings.json`（`web_port_auto=true` 标记非用户显式配置，URL 跨重启稳定），banner 打印「17580 被占用，自动改用 17581」 |
| headless，端口 = 用户显式配置 | 维持 D6：明确报错不漂移（避免静默改写用户配置）；设置页手动改端口时前端清除 `web_port_auto` |
| 全区间（20 个口）都被占 | 报错 + `applied` 置空 → supervisor 2s 后自动重试（**自愈**，冲突解除即恢复；实现期裁定优于进程退出——退出需级联停止被管软件且 systemd 反复重启） |

- **🚨 降级必须发生在绑定失败之后（真绑即真知），禁止预扫描**：supervisor 每 2s 轮询一次 sync，预扫描会把「自己上一轮已成功绑定的端口」误判为被占，导致每次轮询换一个端口、无限爬升到 17599（P1 实测踩坑，已修）。失败后重试天然只看「真绑定是否成功」，自占不可能发生。
- **「仅默认/自动挑选端口才漂移」**：保证行为可预期——用户显式配置永远不被静默改写。
- 回写已解析端口走 `write_settings_file`（原子写、无 save_settings 的代理副作用）；`web_server::sync()` 的 supervisor 2s 轮询天然热生效，无需新机制。

## 5. 首启引导页（setup）

- 触发：app 级 `first_run_done` 标志（区别于软件实例的 `config.initialized`）为 false 时，桌面窗口 / web 首页进入引导。
- 步骤：语言 → 端口确认（**保存前 TCP bind 试连探测占用**）→ 局域网开关（headless 默认开，附明文风险提示；关闭时展示 SSH 隧道模板）→ 完成（置位 `first_run_done`）。
- 保存热生效：`web_server::sync()` supervisor 已支持配置变更重启，无需重启进程。
- **改端口 / 开局域网后的 origin 变化**：localStorage 按 origin 隔离，旧页面的 token 在新 origin 下不可用 → 保存成功后前端展示**新完整 URL（含 `#/?token=…`）**，由 TokenGate 从 fragment 收录新 origin 的 localStorage（该机制已存在，草案 §3.4）。

## 6. 安装器职责（只装 + 告知，不配置）

| 平台 | 交付物 | 安装器行为 |
|---|---|---|
| Windows | NSIS（交互）/ MSI（静默） | 完成页保持 Tauri 默认（含「运行程序」勾选）；**URL/token 不在安装器展示**（见下方偏差说明） |
| Linux | deb / rpm | 已实现：systemd 单元随包（`/lib/systemd/system/opx.service`，`Environment=OPX_APP_ROOT=/var/lib/opx`）+ 维护脚本（postinst：mkdir + daemon-reload + enable + restart + 打印「首次访问地址见 journalctl -u opx」；prerm：stop/disable；postrm：daemon-reload）。数据根经 `OPX_APP_ROOT` 环境变量重定向，避免写入 /usr/bin |
| Linux 便携 | AppImage / 容器 | 首启 banner 直出终端 / `docker logs`；可选 `web_open_browser_on_start` |

**P3 已落地并经 WSL2 Ubuntu 26.04 全链路实测**（`bundle.linux.{deb,rpm}` + `src-tauri/systemd/opx.service` + `src-tauri/deb/{postinst,prerm,postrm}` + `src-tauri/linux/opx-svc.sh`）：

| 实测项 | 结果 |
|---|---|
| `npx tauri build --bundles deb,rpm` 出包 | ✅ deb + rpm 均产出 |
| `dpkg -i` 安装 / 升级（同版本覆盖重装） | ✅ postinst 装配 + restart 分支均走通 |
| `systemctl is-active / is-enabled opx` | ✅ active / enabled |
| journalctl banner（含 token、LAN IP、`/var/lib/opx` 数据根） | ✅ |
| SPA `/` 200、未带 token 的 API 401（鉴权生效） | ✅ |

**实测修正的三个关键点**（都是源码级核实/真跑出来的）：
1. **deb.files 语义**：`{ "目标绝对路径": "源路径" }`，源相对 **src-tauri/（conf 目录）** 解析（v2 与 v1 方向相反的直觉陷阱，连续两次实证确认）。
2. **无显示环境 `--headless` 必挂**：tao 事件循环 GTK 初始化 panic（exit 101），systemd 按 Restart 循环。兜底落地：`linux/opx-svc.sh` 包装（有 DISPLAY/WAYLAND 直连，无显示 `xvfb-run -a`），deb/rpm `recommends` 携带 xvfb，postinst 补执行位。
3. **SPA dist 不随 deb 分发**（adjacent 方案只覆盖 NSIS/MSI）：deb.files 增配 `/usr/lib/opx/dist` + unit 加 `Environment=OPX_WEB_DIST=/usr/lib/opx/dist`。

**连带修复的跨平台编译 bug**（Linux 首次编译暴露，Windows 侧一直无感）：
- `winreg` / `windows-sys` 在两个 Cargo.toml 无条件声明 → 移入 `[target.'cfg(windows)'.dependencies]`（⚠️ Windows-only 依赖必须放 target 段，且该段必须置于文件末尾——插在 [dependencies] 表中间会把后续键全部吞进 Windows 表）。
- `minio.rs` 的 `SILO_MAX_REMOTE_VERSIONS` / `silo_remote_versions` 残留 `#[cfg(windows)]`（P2-1 运行时选包重构的尾巴，函数本体已按 os 参数化）→ 摘除门控。

**遗留（WSL 环境特性，非 opx 问题）**：Windows → WSL IP 直连不通（NAT/防火墙），WSL 内部已完整验证；rpm 的 xvfb 包名（xorg-x11-server-Xvfb）未在真实 Fedora 验证；updater 签名（TAURI_SIGNING_PRIVATE_KEY）未配，deb/rpm 暂无更新签名产物。

**偏差说明（P2 实现期裁定）**：原设想的「NSIS 完成页显示 URL/token」不可行也不必要——
1. Tauri v2 的 NSIS `installerHooks` 只在安装 section 内执行，插不了自定义 MUI 完成页（换自定义模板 = 整份 installer.nsi 的长期维护负担）；
2. token 首启才生成，安装期不存在（NSIS 里预生成 = 脆弱且与 `token::load_or_generate` 双写竞争）。
等价 UX：桌面首启引导页开启「浏览器管理入口」→ 完成时经 `get_web_access_urls`（桌面 IPC）展示含 token 的完整 URL 列表（此时端口/令牌才是最终值），服务器由 supervisor ~2s 内热启动。「装完即见入口」由 Tauri 默认的「完成后运行程序」+ 首启引导页承接。

## 7. 升级幂等与安全红线

1. 安装器与首启种子对 `settings.json` 只允许「**不存在才写**」——升级绝不覆盖 mirror / alerts / token / 语言。
2. 任何含 token 的落盘文件走 `paths::write_file_restricted`（清只读 → 写 → 收紧），禁止 write 后单独 restrict。
3. `first_run_done` 等新增字段遵守「三处齐改」：Rust model + TS model + 页面 ref/save + **watch 依赖数组**。
4. 局域网开启时 setup 页必须展示明文风险提示（原草案 §3.4 要求）。

## 8. 实施阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| P1 ✅ | headless 首启种子（LAN 默认开）+ 端口降级扫描 + setup 页（web 端） | headless 模拟占用 17580 后仍可从工作站进入 setup；改端口后新 URL 可登录 |
| P2 ✅ | 桌面首启引导（复用 setup 组件，新增「浏览器管理入口」开关 + `get_web_access_urls` URL/token 面板） | 桌面新装首启强制进入引导；开启入口后面板展示含 token 的完整 URL，服务器 2s 内热启动（真机实测通过）。升级不覆盖 settings.json 由「不存在才写」保证（P1 md5 实测） |
| P3 ✅ | Linux 打包资产 + WSL2 全链路实测：`OPX_APP_ROOT` 数据根覆盖（含纯函数单测）、systemd 单元、deb/rpm 维护脚本 + conf 挂接、xvfb 包装、dist 分发 | WSL2 Ubuntu 26.04 实测：deb/rpm 出包、dpkg 安装/升级、systemd active+enabled、journalctl banner、SPA 200 / 未授权 API 401（详见 §6 实测表）。连带修复 winreg/windows-sys 无条件声明与 minio.rs cfg 残留两个跨平台编译 bug |

## 9. 开放项

- 自签 TLS / IP 白名单（原草案 §3.4 后续演进）。
- debconf / NSIS 交互配置页（B3）与 preseed 支持。
- `web_bind` 高级覆盖（原草案 §3.6 已列字段，随 setup 页一并暴露）。
