use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppStatus {
    Running,
    Stopped,
    Error,
    Starting,
    Stopping,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringBootApp {
    pub id: String,
    pub name: String,
    pub jar_path: String,
    pub version: String,
    /// 构建该 JAR 的 Spring Boot 版本（MANIFEST 的 `Spring-Boot-Version`）。
    ///
    /// 由 repackage 自动写入，2.x/3.x/4.x 都有（如 `3.5.11`、`4.0.8`、`2.3.3.RELEASE`）；
    /// 非 Spring Boot 打包的 jar 或旧数据为 None（`serde(default)` 兼容旧 apps.json）。
    /// 用途：显示真实框架版本，并据此提示该 jar 需要的最低 JDK。
    #[serde(default)]
    pub spring_boot_version: Option<String>,
    pub jdk_installed_id: String,
    /// 自动生成的优化 JVM 参数 + 用户手动修改后的合并结果
    pub jvm_opts: Vec<String>,
    pub program_args: Vec<String>,
    pub profile: String,
    pub env_vars: Vec<(String, String)>,
    pub status: AppStatus,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub log_path: String,
    pub start_time: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    /// 依赖的已安装软件 ID 列表（如 mysql/redis 的 installed_id）
    pub dependencies: Vec<String>,
    pub auto_start: bool,
    pub startup_order: u32,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub jdk_type: String,
    /// 停止等待时长（秒）：发出优雅停止请求后，等待进程自行退出的上限。
    ///
    /// 默认 30，对齐 Spring Boot 的 `spring.lifecycle.timeout-per-shutdown-phase` 默认值——
    /// 那个阶段超时本身就是「应用清理最多可能花 30 秒」的意思，等得比它短只会误杀。
    #[serde(default = "default_stop_timeout_secs")]
    pub stop_timeout_secs: u64,
}

/// 从 JAR 读出的元信息（供表单展示，不落库）。
///
/// 与 [`SpringBootApp::spring_boot_version`] 的关系：这里只是一次「探测结果」，
/// 用于在选择 jar 的当下就给出「需要什么 JDK」的提示；落库用的是同一读取逻辑。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JarInfo {
    /// 应用自身版本（MANIFEST 的 `Implementation-Version`）——多数项目缺失
    pub version: Option<String>,
    /// 构建该 JAR 的 Spring Boot 版本（MANIFEST 的 `Spring-Boot-Version`）
    pub spring_boot_version: Option<String>,
    /// 该 Spring Boot 版本官方兼容的**最低** Java 主版本；无法判断时为 None
    pub min_jdk: Option<u32>,
    /// 官方兼容的**最高** Java 主版本；`None` = 未知（不提示上限）。
    ///
    /// 这是「官方测试到哪」而非硬约束（且随补丁号上抬），所以只用于**警告**、不拦截保存。
    #[serde(default)]
    pub max_jdk: Option<u32>,
    /// 构建该 JAR 的 JDK 主版本（MANIFEST 的 `Build-Jdk-Spec`），仅作「最佳搭配」参考。
    ///
    /// 它不等于运行门槛：同一份源码可以用 `--release 17` 在 JDK 21 上编出 v61 字节码。
    #[serde(default)]
    pub build_jdk: Option<u32>,
}

/// 停止等待默认值（秒）
pub fn default_stop_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppGroup {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub env_vars: Vec<(String, String)>,
}

/// JVM 运行指标快照。
///
/// 采集源有三处，均为只读操作、不产生 STW 停顿（详见 `services/springboot_manager/monitor.rs`）：
/// - `jstat -gc`：堆各区、Metaspace、分代 GC 计数与耗时
/// - `jcmd PerfCounter.print`：线程数与类加载（读 perfdata 共享内存）
/// - `jcmd VM.flags`：堆上限 Xmx
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmInfo {
    /// 堆已用（S0U + S1U + EU + OU）
    pub heap_used: u64,
    /// 堆上限（`-XX:MaxHeapSize`）。取不到时回退为 `heap_committed`。
    pub heap_max: u64,
    /// 堆当前提交容量（S0C + S1C + EC + OC），随堆增长动态变化，
    /// 与 `heap_max` 不同——除非堆已扩满，否则它不是上限。
    pub heap_committed: u64,
    /// Metaspace 已用
    pub non_heap_used: u64,
    /// Metaspace 提交容量，作为非堆的对比基准
    pub non_heap_committed: u64,
    /// 当前存活线程数
    pub thread_count: usize,
    /// 其中守护线程数
    pub thread_daemon: usize,
    /// 启动至今的峰值存活线程数
    pub thread_peak: usize,
    /// 启动至今累计创建的线程数
    pub thread_started: u64,
    pub classes_loaded: u64,
    pub classes_unloaded: u64,
    /// 年轻代 GC 次数与累计耗时（毫秒）
    pub gc_young_count: u64,
    pub gc_young_time_ms: u64,
    /// Full GC 次数与累计耗时（毫秒）。次数大于 0 是值得运维关注的信号。
    pub gc_full_count: u64,
    pub gc_full_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringBootStore {
    pub applications: Vec<SpringBootApp>,
    pub groups: Vec<AppGroup>,
    #[serde(default)]
    pub global_env_vars: Vec<(String, String)>,
}

impl Default for SpringBootStore {
    fn default() -> Self {
        Self {
            applications: Vec::new(),
            groups: Vec::new(),
            global_env_vars: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceResult {
    pub backup_path: String,
    pub old_version: String,
    pub new_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAppParams {
    pub jar_path: String,
    pub name: String,
    pub jdk_installed_id: String,
    pub jvm_opts: Vec<String>,
    pub program_args: Vec<String>,
    pub profile: String,
    pub env_vars: Vec<(String, String)>,
    pub port: Option<u16>,
    pub log_path: String,
    pub dependencies: Vec<String>,
    pub auto_start: bool,
    pub startup_order: u32,
    pub auto_restart: bool,
    pub group: Option<String>,
    pub jdk_type: String,
    #[serde(default = "default_stop_timeout_secs")]
    pub stop_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAppParams {
    pub name: Option<String>,
    pub jdk_installed_id: Option<String>,
    pub jvm_opts: Option<Vec<String>>,
    pub program_args: Option<Vec<String>>,
    pub profile: Option<String>,
    pub env_vars: Option<Vec<(String, String)>>,
    pub port: Option<u16>,
    pub log_path: Option<String>,
    pub dependencies: Option<Vec<String>>,
    pub auto_start: Option<bool>,
    pub startup_order: Option<u32>,
    pub auto_restart: Option<bool>,
    pub group: Option<Option<String>>,
    pub jdk_type: Option<String>,
    pub stop_timeout_secs: Option<u64>,
}

/// 单个 GC 选项在「所选 JDK」上的可用性（供表单渲染下拉框）。
///
/// 之所以要后端下发而不是前端自己判断：JDK 8 上选 ZGC / Shenandoah 会让 JVM 直接
/// 拒绝启动（`Unrecognized VM option`），可用性规则必须与 `generate_opts` 同源，
/// 否则又会出现「推荐的」和「能选的」两套说法。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcOption {
    pub name: String,
    /// 该 JDK 是否支持；false 时表单应禁用该选项
    pub supported: bool,
    /// 是否是该 JDK 版本的推荐值
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmOptsTemplate {
    pub xms_mb: u64,
    pub xmx_mb: u64,
    pub metaspace_mb: u64,
    pub gc_type: String,
    pub extra_flags: Vec<String>,
    /// 该 JDK 可选的 GC 目录（不支持的已标记），供前端置灰与标注「推荐」
    #[serde(default)]
    pub gc_options: Vec<GcOption>,
}
