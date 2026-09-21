# 网站管理模块 设计规格

- 日期：2026-07-10
- 分支：`feature/website-management`
- 状态：设计已确认，待编写实现计划

## 1. 背景与目标

在现有"软件管理"能安装/启停 nginx 的基础上，新增一个**网站管理**模块，让用户用已安装的 nginx 快速把 **web 页面（前端静态资源）** 部署上线，并可为页面配置反向代理规则（把 `/api` 之类转发到用户自己跑的后端）。

菜单位置：侧边栏 `management` 分组内，插在"软件管理"与"SpringBoot"之间。

## 2. 范围

**In（本期）**
- 站点 = 一组 location，每条 location 要么"静态目录"、要么"反向代理"，可混合（前端 dist 挂 `/` + `/api` 反代）。
- 静态资源两种来源：指向本机已有目录 / 上传 zip 部署到受管目录。
- 站点按 `server_name`（域名，可空）+ `listen`（端口，默认 80）访问。
- 生成 nginx server 块配置、上线/下线、编辑、删除；保存并生效 = 校验 + reload。

**Out（不做 / 仅预留）**
- HTTPS/SSL：数据模型与 UI **预留占位**，本期不实现证书管理（UI 置灰）。
- **不启停/不管理任何后端服务**——反代只写转发规则，后端由 SpringBoot 模块或外部负责。
- 负载均衡（多后端 upstream）、多 nginx 实例选择：本期不做。

## 3. 术语
- **站点 Site**：一个 nginx `server {}` 块，含监听、域名、若干 location。
- **Location**：`server` 内一条 `location`，类型为静态或反代。
- **目标 nginx**：软件管理里已安装的 nginx 实例（本期取唯一/第一个）。

## 4. 架构与落位

**前端**
- 模块目录 `src/modules/website-manager/`
- 路由 `/websites`，`meta.title = 'websiteManagement'`
- 菜单项加入 `Sidebar.vue` 的 management 分组，位于 `/software` 之后、`/springboot` 之前，图标 `mdi:web-box`
- i18n：`websiteManagement` 等 key 加入 `zh-CN.ts` / `en-US.ts`

**后端**
- 命令层 `src-tauri/src/commands/website.rs`
- 服务层 `src-tauri/src/services/website_manager/`
  - `mod.rs`：`WebsiteManager`（加载/持久化 `websites.json`，与 `SoftwareManager` 同套路，用 `RwLock` + 原子写）
  - `nginx_conf.rs`：**纯函数** —— Site → server 块字符串生成、`include` 注入
  - `deploy.rs`：静态 zip 解压到受管目录、目录校验

## 5. 数据模型

存储于配置目录 `websites.json`（`WebsiteList { websites: Vec<Site> }`）。Rust 结构体 + TS 镜像（serde 默认命名，与现有模型一致）。

```
Site {
  id: String,               // uuid
  name: String,
  server_name: Option<String>,   // 域名，空 => 生成 "server_name _;"
  listen: u16,              // 默认 80
  ssl: SslConfig,           // 预留
  enabled: bool,            // 上线/下线
  locations: Vec<Location>,
}

SslConfig {                 // 本期仅存储，UI 置灰
  enabled: bool,            // 恒 false
  cert_path: Option<String>,
  key_path: Option<String>,
}

Location {
  path: String,             // 如 "/" 或 "/api"
  kind: LocationKind,       // Static | Proxy
  // Static:
  source: Option<StaticSource>,  // Dir | Upload
  root: Option<String>,     // 静态目录绝对/相对路径
  spa_fallback: bool,       // 默认 true：try_files $uri $uri/ /index.html
  // Proxy:
  target: Option<String>,   // 如 http://127.0.0.1:8080
}

enum LocationKind { Static, Proxy }
enum StaticSource { Dir, Upload }
```

约束（保存前校验）：
- `listen` 1–65535；`path` 以 `/` 开头；同一站点内 path 不重复。
- Static：`root` 非空且（Dir 时）目录存在；Upload 时受管目录已生成。
- Proxy：`target` 形如 `http(s)://host:port`。

## 6. nginx 集成

**目标解析**：通过 `SoftwareManager` 查 `key == "nginx"` 的已安装实例。无 → 命令返回错误提示"请先在软件管理中安装 nginx"。多个 → 取第一个。
> ponytail: 单 nginx 假设；多实例选择留待后续（升级路径：Site 增加 `nginx_id` 字段 + UI 选择器）。

**配置产物**
- 每个站点生成 `<nginx>/conf/sites/<id>.conf`（一个 server 块）。
- 主配置 `<nginx>/conf/nginx.conf` 的 `http {}` 内**一次性注入** `include sites/*.conf;`（已存在则跳过；注入前 backup + 原子写，复用 `config_editor` 的备份/原子写模式）。
- 上传的静态包解压到 `<nginx>/sites-data/<id>/<location-key>/`，`root` 指向该目录。

**server 块生成（`nginx_conf.rs` 纯函数）**，示例：
```
server {
    listen 80;
    server_name admin.demo.com;   # 或 _
    location / {
        root  ".../sites-data/<id>/root";
        index index.html;
        try_files $uri $uri/ /index.html;   # spa_fallback=true 时
    }
    location /api {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

**生效流程（保存并生效）**
1. 写/更新 `<id>.conf`（enabled=false 或删除 → 移除该 conf）。
2. 若目标 nginx 正在运行：`nginx -t -c <conf>` 校验；失败则回滚 conf 并返回错误。
3. 校验通过：`nginx -s reload`。
4. 若 nginx 未运行：仅落盘，提示"将在 nginx 启动后生效"。

**保存 vs 保存并生效**
- 保存：持久化 `websites.json` + 重新生成该站点 conf（不 reload）。
- 保存并生效：同上 + 校验 + reload。

## 7. UI 设计

**列表页（卡片网格，已选布局 B）** `pages/WebsiteListPage.vue`
- `PageHeader`（图标 `mdi:web-box`，标题=网站管理）+ 右上"新建站点"。
- 每个站点一张卡片：站点名、地址（`server_name:listen` 或 `:listen`）、location 概览 chip（`/ 静态`、`/api → :8080` 反代黄色）、状态徽标（运行中/已下线）、操作（编辑 / 上线·下线 / 删除）。
- 末尾一张虚线"＋ 新建站点"卡。

**新建/编辑对话框** `components/SiteEditDialog.vue`（已确认布局）
- 基本信息：站点名称；域名 `server_name`（可空，含"留空则只按端口访问"提示）；监听端口。
- HTTPS 行：**置灰预留**，禁用勾选 + "即将支持"。
- **Location 编辑器** `components/LocationEditor.vue`：每条 location 一块
  - 顶部：路径输入 + 「静态/反代」分段切换 + 删除。
  - 静态：「指向目录 / 上传部署」分段；Dir 填路径，Upload 显示已上传受管目录 + 重新上传按钮。
  - 反代：后端地址输入（附"仅转发，不启停后端"提示）。
  - "＋ 添加路由规则"。
- 底部：取消 / 保存 / **保存并生效（reload nginx）**。

状态展示复用现有 `StatusBadge` 风格；上传用 `@tauri-apps/plugin-dialog` 选文件。

## 8. Tauri 命令

| 命令 | 说明 |
|------|------|
| `list_websites() -> Vec<Site>` | 列出所有站点 |
| `save_website(site: Site, apply: bool) -> Result<()>` | 新建/更新（按 id 有无区分）；`apply=true` 时校验+reload |
| `delete_website(id) -> Result<()>` | 删除站点 + 移除 conf + reload |
| `set_website_enabled(id, enabled) -> Result<()>` | 上线/下线（生成或移除 conf + reload） |
| `upload_site_bundle(id, path, zip_path) -> Result<String>` | 解压 zip 到受管目录，返回目录路径 |

## 9. 错误处理与校验
- 无已安装 nginx：`save_website`/`set_website_enabled` 返回明确错误，前端弹框（复用现有 alert 提示机制）。
- `nginx -t` 校验失败：回滚该站点 conf 文件，返回 stderr 摘要，站点不生效。
- 端口/路径/target 非法：命令层先校验，返回中文错误。
- 上传解压失败（非法 zip、路径穿越）：拒绝并清理临时文件（复用相对路径白名单校验思路，见 `lifecycle::validate` 现有防御）。

## 10. 测试
- `nginx_conf.rs`：给定 Site 生成 server 块字符串的快照式断言（静态 / 反代 / 混合 / server_name 为空 → `_`）。
- `include` 注入幂等：注入两次结果不变；已有 include 时跳过。
- 目标 nginx 解析：无 nginx 时返回 Err。
- 校验函数：非法端口/路径/target 被拒。
- 均为纯函数或可注入路径的函数，遵循项目现有 `#[cfg(test)]` 内联测试风格，不引入测试框架。

## 11. 预留与后续迭代
- HTTPS/SSL 证书管理（数据模型已留 `SslConfig`）。
- 多 nginx 实例选择（Site 加 `nginx_id`）。
- 负载均衡 / upstream 多后端。
- 站点访问日志、在线预览入口。
