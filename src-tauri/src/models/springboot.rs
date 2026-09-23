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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JvmOptsTemplate {
    pub xms_mb: u64,
    pub xmx_mb: u64,
    pub metaspace_mb: u64,
    pub gc_type: String,
    pub extra_flags: Vec<String>,
}
