/// 操作记录（对应后端 services/software_manager/audit.rs）

export interface AuditEntry {
  ts: string
  action: string
  target: string
  detail: string
}

export interface AuditQuery {
  entries: AuditEntry[]
  /** 过滤后的总条数（不受分页影响） */
  total: number
  /** 是否还有下一页 */
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
