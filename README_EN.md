# OPX — Local Dev Environment Management Desktop Tool

> `README.md` (中文) and `README_EN.md` (English) are the canonical usage documents. When adding or changing a feature, **both must be updated together**.

OPX is a Windows desktop application for managing infrastructure software and services in a local development environment. It provides installation, startup, shutdown, configuration management, and log viewing for middleware such as MySQL, PostgreSQL, MongoDB, Redis, Nginx, MinIO, Kafka, Elasticsearch, InfluxDB, and Nacos; full-lifecycle management for Java Spring Boot and Node.js applications; static website hosting; and service-group (Stack) orchestration.

## Feature Overview

| Module | Capability | Status |
|--------|-----------|:------:|
| **Software Repository** | Browse by category, online/offline install, version selection, mirror switching, batch install, upgrade detection with one-click upgrade / rollback | ✅ |
| **Software Management** | Start / stop / restart, config editing (form / source), health check, startup settings, backup / restore, log viewing (archive continuation / search / export), uninstall | ✅ |
| **Application Management (Spring Boot)** | App registration, JVM tuning (recommended params), environment variables (global / group / app three-level), leveled logs (debug/info/warn/error + archive), log-detection startup, local IP pinning | ✅ |
| **Node App Management** | JS/TS service registration (entry file + args + env + auto-start order), Node.js runtime selection, one-click start/stop, log tail | ✅ |
| **Service Group (Stack)** | Orchestration / member management, one-click start/stop of the whole group, startup report, member port / log quick links | ✅ |
| **Website Management** | Site CRUD, nginx config form / source editing, static file upload, enable / disable, ACME certificate automation | ✅ |
| **Certificate Automation (ACME)** | Let's Encrypt DNS-01 issuance, auto-renewal, multi-DNS provider, staging switch | ✅ |
| **Dynamic DNS (DDNS)** | Cloudflare / Aliyun / DNSPod(Tencent) / Huawei, multi-source public IP detection, 5-min scheduler + manual sync | ✅ |
| **DNS Accounts** | Centralized DNS provider credential management, per-site binding, migration from legacy config, reference protection | ✅ |
| **Audit Log** | Visualized operation records with result state, CSV export, 7-day retention | ✅ |
| **System Monitoring & Alerting** | CPU/memory/disk/network usage, persisted history, threshold alerts, webhook/SMTP notification channels | ✅ |
| **Crash Self-Heal (Watchdog)** | Auto-restart on unexpected exit (3-retry cap), no false restart on manual stop | ✅ |
| **System Monitor** | CPU/memory/disk/network usage, real-time trend charts (Chart.js), system info | ✅ |
| **Custom Software** | Custom start command, config file path, health-check rule | ✅ |

## System Architecture

```
┌─────────────────────────────────────────────┐
│            Frontend (Vue 3 + TypeScript)      │
│  Dashboard │ Software │ NodeApps │ Stack     │
│  Websites  │ SpringBoot │ Audit │ Monitor    │
├─────────────────────────────────────────────┤
│              Tauri IPC (invoke)              │
├─────────────────────────────────────────────┤
│                 Backend (Rust)               │
│  ├─ commands/      — Tauri command layer     │
│  ├─ services/      — business logic          │
│  │  ├─ software_manager/  — software lifecycle│
│  │  ├─ springboot_manager/ — SpringBoot mgmt │
│  │  ├─ node_app_manager/   — Node app mgmt   │
│  │  ├─ stack_manager/     — Stack orchestr.  │
│  │  ├─ website_manager/   — website mgmt     │
│  │  ├─ dns_account/       — DNS credential   │
│  │  ├─ acme/              — cert automation  │
│  │  ├─ ddns/              — dynamic DNS      │
│  │  ├─ watchdog/          — crash self-heal  │
│  │  └─ system_monitor/    — metrics/alert    │
│  ├─ models/        — data models            │
│  └─ utils/         — utilities              │
├─────────────────────────────────────────────┤
│          Local Storage (JSON files)          │
│  apps.json │ node-apps.json │ installed.json │
│  stacks*.json │ settings │ dns-accounts.json │
└─────────────────────────────────────────────┘
```

## Supported Software

Listed by category (the repository page shows tabs per category):

| Category | Software | Version Mgmt | Config Edit |
|----------|----------|:------------:|:-----------:|
| Database | MySQL, PostgreSQL, MongoDB | ✅ online | ✅ form + source |
| Cache | Redis | ✅ online | ✅ form + source |
| Web Server | Nginx | ✅ built-in | ✅ form + source |
| Object Storage | MinIO, RustFS | ✅ online | ✅ form + source |
| Runtime | JDK, JRE, Node.js | ✅ online | ❌ |
| Registry | Nacos | ✅ online | ✅ form + source |
| Message Queue | Kafka (KRaft mode) | ✅ online | ✅ form + source |
| Search | Elasticsearch | ✅ online | ✅ form + source |
| Time Series | InfluxDB, InfluxDB 3 | ✅ online | ✅ form + source |
| Custom | User-uploaded | ✅ user upload | ✅ custom |

The repository supports batch install, upgrade detection, and one-click upgrade (replace flow: stop old → backup → install new → migrate data → merge records), with one-click rollback after upgrade.

## Tech Stack

- **Frontend**: Vue 3 + TypeScript + Vite + Pinia + Chart.js + Tailwind CSS
- **Backend**: Rust + Tauri v2 + reqwest + tokio + serde + sysinfo
- **Storage**: Local JSON file storage (`installed.json`, `settings.json`, …)
- **Logging**: `tracing` + `tracing-appender` (daily rotation, 7-day retention; operation audit log records all key actions)
- **Proxy**: GitHub acceleration proxy (`ghfast.top`) + global HTTP proxy
- **Themes**: Three themes (light / dark / warm) + custom window title bar

## Quick Start

```bash
# Install dependencies
npm install

# Run in development mode (hot reload)
npm run tauri:dev

# Production build
npm run tauri:build
```

## Feature Details

### Software Management
- Built-in software repository with online version lists, mirror switching, and batch install.
- Upgrade detection and one-click upgrade (replace flow: stop old → backup `.bak` → install new → migrate data → merge records), with one-click rollback afterward.
- Backup / restore: data snapshot backup, historical snapshot restore, one-click reset; upgrade backups are zip archives.
- Start / stop / restart, with first-run initialization (e.g. MySQL `--initialize-insecure`).
- Config editing (form mode + source mode); changes take effect and reload automatically after saving.
- Health check (TCP port probe / HTTP status code / process liveness).
- Log viewer: live refresh, auto-scroll, keyword / regex / level filters, time-range filter, seamless archive continuation (auto-cross file/gzip), merged archive export, download, global cross-software log search.
- Runtime-only software (JDK / JRE / Node.js) offers uninstall only.

### Website Management
- Static and reverse-proxy site management based on nginx.
- Form-based route editing (static directory and proxy modes).
- Source mode directly edits the `.conf` file.
- Enable / disable a site triggers nginx reload.

### Certificate Automation (ACME, DNS-01)
- One-click Let's Encrypt issuance: the site dialog adds a "certificate source" selector (self-signed / Let's Encrypt) using DNS-01 validation; saving the site completes issuance, with a popup showing ACME stage progress (10/35/65/90/100%).
- Auto-renewal: checked hourly; renewed automatically when expiry is within 30 days (result is recorded by actual outcome, not blindly marked success).
- Multi-DNS providers (Cloudflare in v1; trait is extensible); issuance and renewal read credentials from the DNS account bound to the site.
- Staging switch (in Settings → Domain tab) for debugging without hitting production rate limits; results are clearly marked as untrusted.
- Certificates are stored at relative paths (`sites-data/certs/<domain>.{crt,key}`) so they survive nginx reinstall.

### Dynamic DNS (DDNS)
- Four providers: Cloudflare, Aliyun, DNSPod (Tencent Cloud), Huawei Cloud. Provider selection and credentials are fully decoupled from website certificates.
- Multi-source public IP detection: IPv4 / IPv6 family-specific endpoints, tried in sequence, aggregating all failure reasons.
- 5-minute scheduler + a "Sync now" button in Settings; `sync_once` is serialized to avoid duplicate records from concurrent scheduler/manual sync.

### DNS Account Management
- New "DNS Accounts" page: centrally manage each provider's credentials (name / provider / credentials / zone cache / test time), with a "Test connectivity" action that really fetches the zone list.
- Site binding: SSL config becomes a dropdown selecting a DNS account; different sites may use different providers; the site domain can be auto-filled from the account's cached zones.
- Legacy migration: on startup, detects the old global Cloudflare Token, auto-creates a "Default Cloudflare" account and binds ACME-enabled sites (idempotent).
- Reference protection: deleting an account referenced by a site is blocked with the site name shown.

### Application Management (Spring Boot)
- Spring Boot JAR app registration and full-lifecycle management.
- Recommended JVM params (G1 tuning auto-computed from Xmx).
- Three-level environment variable override (global → group → app).
- UTF-8 charset option, global / group env var configuration.
- Log-detection startup (every 5s checks the console log for the "Started " marker instead of a fixed timeout).
- Leveled log viewing: debug/info/warn/error multi-source tabs, with `logs/<date>/<type>.date.seq.log.gz` archive history, keyword / regex / level / error-only filters, live + auto-scroll, time-range filter.
- **Local IP pinning**: a "Local IP" field lets you pin the advertised IP. When left blank, OPX auto-detects the real NIC IP and excludes virtual adapters (VMware / VirtualBox / Hyper-V / Docker / VPN) — fixing the common case where Nacos / Spring Boot wrongly pick a virtual adapter IP (e.g. `192.168.200.1`).

### Node App Management
- Register JS / TS services: entry file + start args + env vars + auto-start order.
- Select an installed Node.js runtime as the engine (or auto-select).
- One-click start/stop, run status, log tail.
- Auto-start checked apps in order on launch.
- Running apps are locked from edit/delete, with busy-state debounce.

### Service Group (Stack)
- Orchestration / member management; one-click start / stop of the whole group.
- Startup report: the last startup shows total group time and each member's start time / result (running / failed with reason), persisted.
- Member cards offer one-click "open port in browser" and a log-view quick link.
- Unified startup orchestration: dependencies are started only after they are ready (`wait_dependency_ready`); software-level dependency topology visualization and a process/port map (netstat-collected listening ports + conflict diagnosis).

### System Monitoring & Alerting
- Metrics history persistence: a 30s resident sampler (whole machine + running software / Spring Boot) writes to disk with a configurable retention (1–90 days, default 7). Trend charts read the persisted series.
- Alert rules: four thresholds (default 90, configurable); trigger / recovery records an audit entry + a global toast. (Fixed: alerts used to only work when the monitor page was open.)
- Alert notification channels: webhook (json / DingTalk sign / WeCom / Feishu formats) + SMTP email; Settings provides a real "Test" send.
- Monitor Center page (`/monitor`): KPI×3 + process resource list + search; shares `useRunningSoftware()` with the dashboard.
- Process resource monitoring: static `System` cache across calls for CPU%; corrected fork-type services (e.g. mysqld child process accumulation); disk sampling uses an independent instance for 30s-average CPU.

### Crash Self-Heal (Watchdog)
- Software / Spring Boot / Node entities auto-restart 2s after an unexpected exit; gives up after 3 consecutive failures (Error + toast + audit).
- Failure counter: +1 on observed unexpected exit, cleared on healthy.
- Manual stop is no longer wrongly restarted; an externally-killed process with auto-restart off is uniformly set to "Error + prompt", with status listening promoted to a global `App.vue` registration so events aren't lost on page switch.

### Custom Title Bar
- Native window decoration disabled, custom title bar.
- Drag, double-click maximize/restore, minimize/maximize/close buttons.
- Style updates automatically with theme switching.

### Nacos Cluster Mode
- A single node can join an external cluster (multi-line `ip:port` in `cluster_nodes`; OPX writes `conf/cluster.conf`; cluster mode strictly requires `storage=mysql`).
- The Nacos console link differs by version (3.x omits `/nacos`); the DB field reflects `storage` live.

### Settings Tabs
- Three tabs: General / Monitor & Alert / Domain & DNS. The selected tab is recorded in the URL (`?tab=`, preserved on refresh and deep-link; illegal values fall back to General).
- Config panels link to official docs.
- The site dialog uses a fixed header with only the form area scrolling.

## Themes

Three theme modes, cycled by clicking the theme icon in the top bar:

- **Light** — neutral gray base + sapphire accent
- **Dark** — neutral warm-gray base + amber accent
- **Warm** — warm white paper base + amber accent

Theme preference is saved in Settings and supports following the system (`auto`).

## Download Proxy

Settings supports two proxy modes:

1. **GitHub acceleration proxy** — prefixes download URLs with a proxy (default `https://ghfast.top`).
2. **Global HTTP proxy** — all download requests go through the specified proxy (VPN-class preferred; direct for GitHub).

Proxy changes take effect immediately, no restart needed.

## Build & Package

OPX is packaged with Tauri v2. `package.json` defines the `tauri` script; `tauri.conf.json` sets `build.beforeBuildCommand = "npm run build"`, which runs the frontend build (vue-tsc type-check + vite build) before packaging, and `bundle.targets = ["nsis"]` produces a `.exe` installer on Windows.

### Prerequisites (Windows)
- **Rust toolchain**: install via `rustup` (https://rustup.rs/); `cargo --version` available.
- **Visual Studio Build Tools**: "Desktop development with C++" workload (MSVC + Windows SDK). First build compiles all Rust deps and is slow.
- **WebView2 Runtime**: preinstalled on Windows 11; required on Windows 10 (the installer bootstraps it).
- **Node.js + npm**: `npm install` for frontend deps.

### Build commands

```bash
npm install                 # install frontend deps

npm run tauri:build         # full release: frontend build + Rust release compile + MSI + NSIS installer + portable exe
npm run tauri:build:nsis    # NSIS .exe only (skip MSI, no WiX) — recommended in CN
npm run tauri:build:msi     # MSI only (requires WiX)
npm run tauri:build:debug   # debug build, fast verification

npm run tauri:dev           # dev mode, hot reload, no packaging
npm run build               # frontend only (vue-tsc + vite → dist/)
npm run tauri:info          # environment info
```

> **Important**: the portable exe (`target/release/opx.exe`) produced by `tauri build` has frontend assets **embedded** and runs standalone. Do **not** use `cargo build --release` to produce the portable exe — it skips Tauri's asset-embedding step and yields a blank page.

### Artifacts
Under `src-tauri/target/release/bundle/`:
- `msi/opx_0.5.0_x64_zh-CN.msi` — MSI installer (enterprise / GPO-friendly)
- `nsis/opx_0.5.0_x64-setup.exe` — NSIS installer (recommended for end users)
- `src-tauri/target/release/opx.exe` — portable exe (requires WebView2 on the target machine)

MSI/NSIS installers embed the WebView2 Bootstrapper; the portable exe does not.

### Versioning
The package version comes from `tauri.conf.json`'s `version` field (currently `0.5.0`). On a new release, keep `tauri.conf.json`, `package.json`, and `Cargo.toml` versions in sync.

## Contributing

This is a personal local dev-environment management tool. Issues and PRs are welcome.

## License

MIT
