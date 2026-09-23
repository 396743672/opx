export enum AppStatus {
  Running = 'Running',
  Stopped = 'Stopped',
  Error = 'Error',
  Starting = 'Starting',
  Stopping = 'Stopping',
}

export interface SpringBootApp {
  id: string
  name: string
  jar_path: string
  version: string
  jdk_installed_id: string
  jvm_opts: string[]
  program_args: string[]
  profile: string
  env_vars: [string, string][]
  status: AppStatus
  pid: number | null
  port: number | null
  log_path: string
  start_time: string | null
  last_error: string | null
  dependencies: string[]
  auto_start: boolean
  startup_order: number
  auto_restart: boolean
  group: string | null
  jdk_type: string
  /** 停止等待时长（秒）：发出优雅停止请求后等应用自行退出的上限，默认 30 */
  stop_timeout_secs: number
  /** 优雅停止地址；留空则按端口推导 http://127.0.0.1:{port}/actuator/shutdown */
  actuator_shutdown_url: string | null
}

export interface AppGroup {
  id: string
  name: string
  order: number
  depends_on: string[]
  env_vars: [string, string][]
}

/** JVM 指标快照。内存字段单位字节，GC 耗时为毫秒。 */
export interface JvmInfo {
  heap_used: number
  /** 堆上限（-XX:MaxHeapSize），取不到时为当前提交容量 */
  heap_max: number
  /** 堆当前提交容量，随堆增长动态变化，通常小于 heap_max */
  heap_committed: number
  non_heap_used: number
  /** Metaspace 提交容量，作为非堆的对比基准 */
  non_heap_committed: number
  thread_count: number
  thread_daemon: number
  /** 启动至今的峰值存活线程数 */
  thread_peak: number
  thread_started: number
  classes_loaded: number
  classes_unloaded: number
  gc_young_count: number
  gc_young_time_ms: number
  /** Full GC 次数，大于 0 是值得关注的信号 */
  gc_full_count: number
  gc_full_time_ms: number
}

export interface ReplaceResult {
  backup_path: string
  old_version: string
  new_version: string
}

export interface CreateAppParams {
  jar_path: string
  name: string
  jdk_installed_id: string
  jvm_opts: string[]
  program_args: string[]
  profile: string
  env_vars: [string, string][]
  port?: number | null
  log_path: string
  dependencies: string[]
  auto_start: boolean
  startup_order: number
  auto_restart: boolean
  group: string | null
  jdk_type: string
  stop_timeout_secs?: number
  actuator_shutdown_url?: string | null
}

export interface UpdateAppParams {
  name?: string
  jdk_installed_id?: string
  jvm_opts?: string[]
  program_args?: string[]
  profile?: string
  env_vars?: [string, string][]
  port?: number | null
  log_path?: string
  dependencies?: string[]
  auto_start?: boolean
  startup_order?: number
  auto_restart?: boolean
  group?: string | null
  jdk_type?: string
  stop_timeout_secs?: number
  /** 传空串表示清空（停止时回退为按端口推导） */
  actuator_shutdown_url?: string
}

export interface JvmOptsTemplate {
  xms_mb: number
  xmx_mb: number
  metaspace_mb: number
  gc_type: string
  extra_flags: string[]
}
