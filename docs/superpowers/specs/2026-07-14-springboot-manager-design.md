# Spring Boot 管理模块设计规格

## 概述

在 OPX 桌面运维工具中增加 Spring Boot 应用管理模块，允许用户注册本地 JAR 包、配置启动参数、管理生命周期、监控 JVM 状态。

## 功能列表

| 功能 | 说明 |
|------|------|
| 注册应用 | 文件选择器选 JAR → 自动读取版本 → 参数表单填写 |
| 参数表单 | 名称、JDK选择、JVM选项、程序参数、Profile、环境变量、日志路径 |
| JDK自动优化 | 根据所选 JDK 版本自动填充优化后的 JVM 参数 |
| 生命周期 | 启动/停止/重启 |
| 前置依赖验证 | 可选关联本机已安装软件，启动前检查/启动依赖 |
| 换包（部署） | 选新JAR替换旧JAR，自动备份旧JAR（时间戳后缀） |
| JVM 监控 | 堆内存、GC、线程数（通过 jcmd 采集） |
| 日志查看 | 查看/tail 日志文件 |
| 分组管理 | 将应用分组，控制启动顺序 |
| 自动启停 | 随 OPX 自动启动、崩溃自动重启 |

## 约束

- 仅管理**本机** Spring Boot 应用
- 运行中的应用不允许修改配置、换包、删除

## 架构

### 整体结构

Spring Boot Manager 为独立模块，遵循现有 SoftwareManager 模式：

```
SpringBootManager (Rust)
  ├─ apps.json (持久化，位于 {data_dir}/springboot/)
  ├─ 引用 SoftwareManager (JDK列表 + 依赖状态)
  ├─ 复用 ProcessRegistry (进程跟踪)
  └─ 复用 health_check (HTTP探活)

Tauri Commands → springboot.rs ←→ SpringBootManager
Vue Pages → SpringBootPage.vue ← invoke() → Tauri Commands
```

### Rust 后端

#### 数据模型 (`src-tauri/src/models/springboot.rs`)

```rust
pub struct SpringBootApp {
    pub id: String,
    pub name: String,
    pub jar_path: String,
    pub version: String,

    // JDK 关联
    pub jdk_installed_id: String,

    // 启动参数
    pub jvm_opts: Vec<String>,
    pub program_args: Vec<String>,
    pub profile: String,
    pub env_vars: Vec<(String, String)>,

    // 运行时
    pub status: AppStatus,
    pub pid: Option<u32>,
    pub port: u16,
    pub log_path: String,
    pub start_time: Option<NaiveDateTime>,
    pub last_error: Option<String>,

    // 前置依赖
    pub dependencies: Vec<String>,

    // 高级设置
    pub auto_start: bool,
    pub startup_order: u32,
    pub auto_restart: bool,
    pub group: Option<String>,
}

pub struct SpringBootStore {
    pub applications: Vec<SpringBootApp>,
    pub groups: Vec<AppGroup>,
}
```

#### SpringBootManager (`src-tauri/src/services/springboot_manager/`)

```
mod.rs           — SpringBootManager 结构体，RwLock<SpringBootStore> + JSON 持久化
lifecycle.rs     — 启动/停止/重启流程
jvm_opts.rs      — JDK 版本检测 + 自动生成优化参数
monitor.rs       — jcmd 采集 JVM 指标
deps.rs          — 前置依赖验证
```

启动流程：
1. 验证应用已停止
2. 验证前置依赖（如启用）
3. 拼接 `java -jar` 命令（使用所选 JDK 路径）
4. 启动子进程，stdout/stderr 重定向到日志文件
5. HTTP 探活（端口，30s 超时）
6. 注册到 ProcessRegistry
7. 启动 auto_restart 监听器

停止流程：
1. 发 SIGTERM (Windows: taskkill /PID)
2. 等待 10s 优雅退出
3. 超时未退出 → 强制杀进程
4. 标记 Stopped，从 ProcessRegistry 注销

换包流程：
1. 仅已停止的应用可换包
2. 选新 JAR，弹出确认对话框
3. 备份旧 JAR 到 `{backups_dir}/{app_name}/{jar}.{timestamp}.bak`
4. 替换 JAR 文件
5. 更新版本信息

#### JDK 自动优化策略

根据所选 JDK 版本 + 本机物理内存生成默认参数：

| JDK | GC策略 | 特定参数 |
|-----|--------|---------|
| 8 | G1GC | `-XX:MetaspaceSize=128m -XX:MaxMetaspaceSize=256m` |
| 11 | G1GC | 同上 + `-XX:+UseStringDeduplication` |
| 17+ | ZGC | `-XX:+UseZGC` + Metaspace 限制 |

通用参数：`-Xms` = Xmx/2, `-Xmx` = 本机内存 50%, `-XX:+ExitOnOutOfMemoryError`, `-XX:+HeapDumpOnOutOfMemoryError`

### Tauri 命令 (`commands/springboot.rs`)

```
list_apps                  → Vec<SpringBootApp>
create_app(params)         → SpringBootApp
update_app(id, params)     → SpringBootApp
delete_app(id)             → ()
start_app(id)              → ()
stop_app(id)               → ()
restart_app(id)            → ()
replace_jar(id, new_path)  → ReplaceResult
get_jvm_metrics(id)        → JvmInfo  (一次性查询)
list_available_jdks        → Vec<InstalledSoftware>  (从 SoftwareManager 读)
list_dependency_candidates → Vec<InstalledSoftware>  (MySQL/Redis 等)
```

### Vue 前端

#### 页面结构

```
src/modules/springboot-manager/
├── pages/
│   └── SpringBootPage.vue      — 主页面
├── components/
│   ├── AppCard.vue             — 应用卡片
│   ├── AppFormDialog.vue       — 注册/编辑参数表单
│   ├── JvmMetricsDialog.vue    — JVM 监控弹窗
│   ├── LogViewer.vue           — 日志查看器
│   ├── GroupManager.vue        — 分组管理
│   └── DependencyDialog.vue    — 前置依赖配置
└── stores/
    └── springboot.ts           — Pinia store
```

#### 组件交互

```
SpringBootPage.vue
  ├─ PageHeader (title + 刷新 + 新增按钮)
  ├─ Group tabs (可选过滤)
  ├─ AppCard[] 网格布局
  │   ├─ 状态指示 (Running/Stopped/Error)
  │   ├─ 基本信息 (名称、端口、JDK、PID、启动时间)
  │   └─ 操作按钮:
  │       ├─ 启动/停止/重启
  │       ├─ 换包 (仅 Stopped 时可用)
  │       ├─ 配置 (仅 Stopped 时可用)
  │       ├─ JVM 监控 (仅 Running 时可用)
  │       └─ 日志
  │
  ├─ AppFormDialog (新增/编辑)
  │   ├─ JDK 下拉选择 → 触发自动填充 JVM 参数
  │   ├─ JVM 参数表单 (结构化字段: Xms/Xmx/Metaspace/标志/额外参数)
  │   ├─ 程序参数 + Profile + 端口
  │   ├─ 环境变量 (键值对列表)
  │   ├─ 日志路径
  │   ├─ 前置依赖配置
  │   └─ 高级设置 (自动启动/顺序/自动重启/分组)
  │
  ├─ JvmMetricsDialog (Chart.js 仪表盘)
  │   ├─ 堆内存 (环形图 + 数值)
  │   ├─ Metaspace
  │   ├─ 线程数
  │   └─ GC 统计
  │
  ├─ LogViewer (嵌入式，tail -f 模式)
  ├─ GroupManager
  └─ DependencyDialog
```

#### Pinia Store

```typescript
// stores/springboot.ts
export const useSpringBootStore = defineStore('springboot', {
  state: () => ({
    apps: [] as SpringApp[],
    groups: [] as AppGroup[],
    loading: false,
    jdkList: [] as InstalledSoftware[], // 可用 JDK
    dependencyCandidates: [] as InstalledSoftware[],
    jvmMetrics: null as JvmInfo | null,
  }),
  actions: {
    async fetchApps(),
    async createApp(params),
    async updateApp(id, params),
    async deleteApp(id),
    async startApp(id),
    async stopApp(id),
    async restartApp(id),
    async replaceJar(id, newPath),
    async fetchJdkList(),
    async fetchJvmMetrics(id),
    async fetchDependencyCandidates(),
  }
})
```

### 路由

路由已注册：
```
/springboot → SpringBootPage.vue (meta.title: 'springBoot')
```

### 国际化

复用现有 `$t('springBoot')` key，新增模块级 key 如下：

```
springBootManager:
  registerApp: "注册应用"
  editApp: "编辑应用"
  replaceJar: "换包"
  jvmMetrics: "JVM 监控"
  appLog: "应用日志"
  jdkSelect: "选择 JDK"
  jvmOpts: "JVM 参数"
  dependencies: "前置依赖"
  backupEnabled: "启用备份"
  autoRestart: "自动重启"
  startupOrder: "启动顺序"
  groupManage: "管理分组"
  ...
```

## 验证规则

1. 运行中的应用：操作按钮（配置、换包、删除）禁用
2. 启动时验证前置依赖：未运行的依赖阻止启动并给出提示
3. JAR 文件存在性校验：注册时检查文件可读
4. 端口冲突：启动时检查端口是否被占用（可选）

## 测试策略

- 后端：单元测试（参数生成逻辑、换包备份逻辑）
- 前端：组件渲染测试（可选）
- 手动测试流程：注册 → 启动 → 停止 → 换包 → 删除
