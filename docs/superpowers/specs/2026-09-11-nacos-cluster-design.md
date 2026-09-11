# Nacos cluster 模式（单节点加入外部集群）+ 清理 cluster 置灰机制

> 状态：已确认设计
> 日期：2026-09-11

## 背景与现状

- OPX 的 Nacos provider（`src-tauri/src/services/software_manager/providers/nacos.rs`）目前**只支持 standalone**：`start_command` 硬编码 `-Dnacos.standalone=true`，配置里选到非 standalone 直接报错「集群模式暂未支持」。
- 配置表单的 `mode` 字段把 `cluster` 选项**前置置灰**，靠 `ConfigFieldType::Select { disabled_options, disabled_hint_i18n }` 通用机制实现。
- 该置灰机制在全仓**只有 Nacos cluster 这一个真实用例**（其余 provider 全填空 vec/None）。实现 cluster 后这套机制成为死代码。

## 已确认决策

1. **语义：单节点加入外部集群**。OPX 管理的 Nacos 实例作为集群的一个节点启动；集群成员列表由用户提供。不做本机多节点编排（与「一个软件=一个实例」模型冲突）。
2. **节点列表输入：新增 `Textarea` 字段类型**，一行一个 `ip:port`。
3. **存储校验：cluster 必须 MySQL，否则阻止启动**（Nacos 官方：集群模式依赖外部 MySQL）。
4. **清理范围：只清理 cluster 置灰机制**产生的死代码。
5. **顺带清理**仓库中游离损坏提交 `3fc1261`（已确认：其 tree `5f77c4ff` 缺失、无法重建；其余悬挂对象均为 07-29~08-25 旧 rebase WIP，已被取代，无价值）。

## 设计

### 1. 配置模型

`src-tauri/src/models/software.rs`：

- `ConfigFieldType` 新增变体 `Textarea`（无附加参数）。
- `ConfigFieldType::Select` **移除** `disabled_options`、`disabled_hint_i18n` 两个字段。

`src/models/software.ts` 同步：`ConfigFieldType` 增加 `{ type: 'Textarea' }`；`Select` 移除对应两个可选字段。

### 2. Nacos 配置表单（nacos.rs）

- `mode` 改为普通 Select（`standalone` / `cluster`），移除置灰字段。
- 新增字段 `cluster_nodes`：`Textarea`，`default_value: ""`，label/desc i18n 为 `configField.nacosClusterNodes` / `configField.nacosClusterNodesDesc`（文案提示「每行一个 ip:port，需包含本节点」）。
- `ConfigSchema.field_rules` 增加一条：
  `FieldRule { field_key: "cluster_nodes", visible_when: Some(FieldCondition { key: "mode", equals: json!("cluster") }), required: true }`
  （复用现有 `field_rules` 条件显示/必填机制，前端 `ConfigFormTab`/`ConfigEditDialog` 已支持。）
- 需在 nacos.rs 引入 `FieldRule`、`FieldCondition`。

### 3. 启动逻辑（nacos.rs::start_command）

新增两个纯函数（便于单测）：

- `parse_cluster_nodes(raw: &str) -> Result<Vec<String>, String>`：
  按行/逗号切分 → trim → 滤空行；每项须匹配 `host:port`（host 非空、port 1–65535），返回规范化 `host:port` 列表；空列表或任一项非法返回 Err。
- `render_cluster_conf(nodes: &[String]) -> String`：每行一个 `host:port` + 结尾换行。

`start_command` 的 mode 分支：

- `standalone`：行为完全不变（`-Dnacos.standalone=true`）。
- `cluster`：
  1. 若 `storage != "mysql"` → `Err(i18n:nacosClusterNeedsMysql)`；
  2. 解析 `cluster_nodes`，失败 → `Err(i18n:nacosClusterNodesInvalid)`；
  3. 确保 `<install_path>/conf/` 存在，幂等覆写 `conf/cluster.conf`；
  4. 不传 `-Dnacos.standalone=true`（即集群模式）。

其余参数（port / console_port / storage / heap / function_mode / context_path / add-opens / identity）保持不变。

> 注：切回 standalone 时残留的 `cluster.conf` 不生效（以 standalone flag 为准），无需删除。

### 4. 前端渲染

`src/modules/software-manager/components/ConfigFormTab.vue`：

- 新增 `Textarea` 渲染分支（`<textarea class="input">`）。
- 删除 `isDisabledOption` / `disabledHint` 两个 helper 及模板中 `<option>` 的 `:disabled` / `:title`。

### 5. 死代码清理清单

- `models/software.rs`：`Select` 去掉两字段。
- 全部 provider 去掉 `disabled_options` / `disabled_hint_i18n` 填充：`nginx.rs`、`redis.rs`、`mysql.rs`、`kafka.rs`、`elasticsearch.rs`、`nacos.rs`。
- `src/models/software.ts`：同步去掉。
- `ConfigFormTab.vue`：删两个 helper + 模板属性。
- i18n：删除 `configField.nacosModeClusterHint`（中英），更新 `configField.nacosModeDesc`，新增 `nacosClusterNodes`、`nacosClusterNodesDesc`、`nacosClusterNeedsMysql`、`nacosClusterNodesInvalid`（中英）。

### 6. 仓库维护

游离损坏提交 `3fc1261`（引用缺失 tree `5f77c4ff`）导致 `git maintenance` 的 geometric-repack 报 `bad tree object`。处理方式：`git reflog expire --expire-unreachable=now --all` + `git gc --prune=now`，丢弃全部不可达对象。

> 已执行并验证：`git fsck --full` 无 missing/broken；geometric-repack 正常；分支历史完好。

## 测试

- `parse_cluster_nodes` 单测：合法多行、逗号分隔、空串、缺端口、非法端口(0/65536/非数字)、多余空白/空行。
- `render_cluster_conf` 单测：行格式与结尾换行。
- cluster + embedded storage → Err（分支校验）。
- `cargo test --lib`；`npx vue-tsc --noEmit`。

## 边界（不做）

- 不在本机编排多 Nacos 节点；不分配/校验 Raft(7848)、gRPC(9848/9849) 端口。
- 不做节点连通性探测或集群健康编排。
- 不校验「本节点地址是否在列表中」（host 可能是用户自定义域名/IP，无法可靠推断）；仅在字段描述中提示。

## 改动文件清单

- `src-tauri/src/services/software_manager/providers/nacos.rs`
- `src-tauri/src/models/software.rs`
- `src-tauri/src/services/software_manager/providers/{nginx,redis,mysql,kafka,elasticsearch}.rs`
- `src/modules/software-manager/components/ConfigFormTab.vue`
- `src/models/software.ts`
- `src/locales/zh-CN.ts`、`src/locales/en-US.ts`
