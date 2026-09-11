// 启动编排报告前端类型（与后端 services/startup_bootstrap.rs 对应）

export interface StartupItemReport {
  kind: string // software / node / stack
  id: string
  name: string
  status: string // running / failed / skipped
  elapsed_ms: number
  message: string
}

export interface StartupReport {
  started_at: string
  total_elapsed_ms: number
  items: StartupItemReport[]
}
