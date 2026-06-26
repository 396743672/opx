export enum AppStatus {
  Running = 'Running',
  Stopped = 'Stopped',
  Error = 'Error',
  Starting = 'Starting',
  Stopping = 'Stopping',
}

export interface SpringApp {
  id: string
  name: string
  jar_path: string
  version: string
  env: string
  port: number
  jvm_opts: string
  args: string
  status: AppStatus
  log_path: string
  start_time: string | null
  backup_enabled: boolean
  auto_restart: boolean
  group: string | null
  auto_start_on_app_start: boolean
  startup_order: number
}

export interface AppGroup {
  id: string
  name: string
  order: number
  depends_on: string[]
}

export interface JvmInfo {
  heap_used: number
  heap_max: number
  non_heap_used: number
  thread_count: number
  gc_count: number
  gc_time: number
}

export interface SpringAppList {
  applications: SpringApp[]
  groups: AppGroup[]
}
