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
  health_check_timeout_secs: number
  jdk_type: string
}

export interface AppGroup {
  id: string
  name: string
  order: number
  depends_on: string[]
  env_vars: [string, string][]
}

export interface JvmInfo {
  heap_used: number
  heap_max: number
  non_heap_used: number
  thread_count: number
  gc_count: number
  gc_time: number
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
  health_check_timeout_secs: number
  jdk_type: string
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
  health_check_timeout_secs?: number
  jdk_type?: string
}

export interface JvmOptsTemplate {
  xms_mb: number
  xmx_mb: number
  metaspace_mb: number
  gc_type: string
  extra_flags: string[]
}
