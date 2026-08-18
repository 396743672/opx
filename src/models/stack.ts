// B 扩展（一键启动栈 Stack）前端类型声明
// 与后端 src-tauri/src/models/stack.rs 的 serde 输出一一对应。

/** 栈成员引用类型（snake_case 序列化） */
export type StackItemRefType = 'software' | 'springboot'

/** 栈内单个成员 */
export interface StackItem {
  ref_type: StackItemRefType
  /** 栈内唯一引用键，即被引用实体的 id */
  ref_id: string
  /** 同层 tie-break（升序） */
  order: number
  /** 依赖的其它成员 ref_id */
  depends_on: string[]
  /** 是否纳入编排（默认 true） */
  enabled: boolean
  /** 启动失败重试次数（默认 0） */
  retry: number
}

/** 一个栈 */
export interface Stack {
  id: string
  name: string
  description: string
  items: StackItem[]
  created_at: string
  updated_at: string
  /** 上次启动由栈拉起的组外依赖（可选，运行时记录） */
  managed_externals?: string[] | null
}

/** 成员运行态（snake_case 序列化） */
export type StackMemberStatus =
  | 'pending'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'stopped'
  | 'failed'

/** 单个成员运行态快照 */
export interface StackMemberRuntime {
  ref_id: string
  status: StackMemberStatus
  message: string
}

/** 启动执行计划 */
export interface StackStartPlan {
  stack_id: string
  layers: string[][]
  cycle: string[] | null
}

/** 创建栈请求载荷 */
export interface CreateStackPayload {
  name: string
  description: string
  items: StackItem[]
}

/** 更新栈请求载荷（全可选） */
export interface UpdateStackPayload {
  name?: string | null
  description?: string | null
  items?: StackItem[] | null
}

/** stack-status-changed 事件载荷 */
export interface StackStatusEvent {
  stack_id: string
  status: StackMemberStatus
  members: StackMemberRuntime[]
}
