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
  args: string[]
  env_vars: [string, string][]
  auto_start: boolean
  startup_order: number
  status: NodeAppStatus
  pid: number | null
  last_error: string | null
  log_path: string
}

export interface CreateNodeAppParams {
  name: string
  entry_path: string
  args?: string[]
  env_vars?: [string, string][]
  auto_start?: boolean
  startup_order?: number
}

export interface UpdateNodeAppParams {
  name?: string | null
  entry_path?: string | null
  args?: string[] | null
  env_vars?: [string, string][] | null
  auto_start?: boolean | null
  startup_order?: number | null
}