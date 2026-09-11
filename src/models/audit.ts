/// 操作记录（对应后端 services/software_manager/audit.rs）

export interface AuditEntry {
  ts: string
  action: string
  target: string
  detail: string
}

export interface AuditQuery {
  entries: AuditEntry[]
  truncated: boolean
}

export interface ActionCount {
  action: string
  count: number
}

export interface AuditStats {
  today: number
  total: number
  by_action: ActionCount[]
  last_ts: string | null
}
