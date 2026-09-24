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
  /** 构建该 JAR 的 Spring Boot 版本（MANIFEST 的 Spring-Boot-Version）；
   *  非 Spring Boot 打包或旧数据为 null */
  spring_boot_version: string | null
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

/**
 * 停止/重启的结果。
 *
 * 强制终止**也是停止成功**，只是没走应用的 shutdown hook —— 那是 `message` 里的提示，
 * 不是错误。所以 `invoke` 不会 reject，界面据此提示而不是报错。
 */
export interface StopOutcome {
  /** 是否走完了应用的优雅停机（false = 强制终止） */
  graceful: boolean
  /** 需要告知用户的提示，仅在 graceful 为 false 时存在 */
  message: string | null
}

/** 从 JAR 读出的元信息（选择 jar 时的探测结果，用于提示该 jar 需要什么 JDK） */
export interface JarInfo {
  /** 应用自身版本（MANIFEST 的 Implementation-Version），多数项目缺失 */
  version: string | null
  /** 构建该 JAR 的 Spring Boot 版本（repackage 自动写入，2.x/3.x/4.x 都有） */
  spring_boot_version: string | null
  /** 该 Spring Boot 版本官方兼容的**最低** Java 主版本；无法判断时为 null */
  min_jdk: number | null
  /** 官方兼容的**最高** Java 主版本；null = 未知（不提示上限） */
  max_jdk: number | null
  /** 构建该 jar 的 JDK 主版本（MANIFEST Build-Jdk-Spec），仅作「最佳搭配」参考 */
  build_jdk: number | null
}

/** 单个 GC 选项在「所选 JDK」上的可用性，由后端下发（与推荐逻辑同源） */
export interface GcOption {
  name: string
  /** 该 JDK 是否支持；false 时下拉框应禁用该选项 */
  supported: boolean
  /** 是否是该 JDK 版本的推荐值 */
  recommended: boolean
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
}

export interface JvmOptsTemplate {
  xms_mb: number
  xmx_mb: number
  metaspace_mb: number
  gc_type: string
  extra_flags: string[]
}

/**
 * `get_recommended_jvm_opts` 的返回：推荐参数 + 该 JDK 的 GC 目录。
 *
 * 目录单独放在这里而不是塞进 `JvmOptsTemplate`——后者既是表单 `jvm` 的响应式形状，
 * 又是 `parseJvmOpts`（从已存的 jvm_opts 数组反推）的产物，那两处根本拿不到目录。
 */
export interface JvmOptsRecommendation extends JvmOptsTemplate {
  gc_options: GcOption[]
}
