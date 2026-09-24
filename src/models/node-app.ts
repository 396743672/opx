// Node 应用管理前端类型（与后端 models/node_app.rs serde 对应）

export type NodeAppStatus =
  | 'stopped'
  | 'running'
  | 'starting'
  | 'stopping'
  | 'error'

export interface NodeApp {
  id: string
  name: string
  entry_path: string
  /** 指定 Node 实例 installed_id（空 = 自动） */
  node_installed_id: string
  args: string[]
  env_vars: [string, string][]
  auto_start: boolean
  /** 进程意外退出后自动重启 */
  auto_restart: boolean
  startup_order: number
  status: NodeAppStatus
  pid: number | null
  last_error: string | null
  log_path: string
}