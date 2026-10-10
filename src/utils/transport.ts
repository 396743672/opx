// 双通道传输层（批次 4.4，设计 §3.2 A2 / §3.4 B2 / D3 / D3.2 / D4）。
//
// 运行时探测 `__TAURI_INTERNALS__`（A2 已定）：有 → Tauri IPC（桌面行为不变），
// 无 → HTTP（同一份产物通吃，零双构建）。
//
// HTTP 通道契约（与 4.2/4.3 定稿的后端信封一一对应）：
// - invoke：`POST /api/{cmd}` + `Authorization: Bearer`，body = 参数表。
//   顶层键做 camelCase → snake_case 转换——与 Tauri IPC 的参数名转换行为
//   **对齐**（Tauri 自动把 `{ installedId }` 映射到 Rust 参数 `installed_id`，
//   HTTP 分发器按 snake_case 提参，故这里补上同一层转换；嵌套对象不动，
//   模型 serde 自管命名，两传输下形状一致）。
//   响应：200 = 命令返回值；400/404/409/500 = `{code,message}` 信封，
//   message（含 `i18n:key`）以**字符串**抛出，由 ipc.ts 的 translateError
//   统一转译（错误转译单一入口不变）；401 = 清除本地 token 并通知
//   unauthorized 监听者（TokenGate 重新显示输入页）。
// - listen：应用内**单条** WS 连接（`/api/events/ws`，D3.2：每次连接先经
//   Bearer 换 60 秒单次 ticket，浏览器 WS 无法设 Authorization 头），
//   本地按事件名多路分发到各订阅者；帧格式 `{event, payload}`（4.3 定稿）。
//   断线指数退避重连（1s 起、2 倍递增、30s 封顶）；**重连成功即触发 resync
//   回调**（D4：订阅方全量刷新自己的状态缓存——重连期间的事件无法补投，
//   补偿责任在前端）。桌面模式 resync 恒不触发（事件实时推送无需补偿）。
// - token（D3）：Http 模式从 URL fragment `#token=<hex>` 读取（不随请求
//   上送、不入访问日志），读后立即从地址栏剥离并转存 sessionStorage
//   （随标签页会话失效，比 localStorage 泄漏面小——团队裁定与文档
//   §3.4 的 localStorage 取舍不同，取更严一侧）；无 token 时调用方
//   （App.vue TokenGate）展示最简输入页。

import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen } from '@tauri-apps/api/event'

/** 与 @tauri-apps/api/event 的 UnlistenFn 同形，调用点零改动迁移 */
export type UnlistenFn = () => void

/** 传输层统一接口（§3.2）：invoke 签名与旧 ipc.ts 完全一致，listen 与
 *  Tauri event.listen 完全一致——两实现之外的调用方不感知通道差异。 */
export interface Transport {
  /** 桌面专属入口（quit/exit/hide/更新器等）按此隐藏 UI */
  readonly isDesktop: boolean
  invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T>
  listen<T = unknown>(
    event: string,
    handler: (event: { payload: T }) => void,
  ): Promise<UnlistenFn>
  /** 注册 WS 重连成功后的补偿刷新回调（D4）；返回取消注册函数 */
  onResync(cb: () => void): UnlistenFn
  /** HTTP 模式是否已持有 token（桌面模式恒 true） */
  readonly authenticated: boolean
  /** 写入 token（sessionStorage + 内存）；随后自动重试建立 WS 连接 */
  setToken(token: string): void
  /** 注册「token 被拒（401）」回调；返回取消注册函数 */
  onUnauthorized(cb: () => void): UnlistenFn
}

// 运行时探测（A2）：Tauri WebView 注入 `__TAURI_INTERNALS__`，普通浏览器没有
declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

const IS_DESKTOP: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// =====================================================================
// Tauri 实现：桌面行为逐字节不变（invoke/listen 直通官方 API）
// =====================================================================

class TauriTransport implements Transport {
  readonly isDesktop = true

  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return tauriInvoke<T>(cmd, args)
  }

  listen<T>(
    event: string,
    handler: (event: { payload: T }) => void,
  ): Promise<UnlistenFn> {
    return tauriListen<T>(event, handler)
  }

  // 桌面事件实时推送，无需重连补偿
  onResync(_cb: () => void): UnlistenFn {
    return () => {}
  }

  get authenticated(): boolean {
    return true
  }

  setToken(_token: string): void {}

  onUnauthorized(_cb: () => void): UnlistenFn {
    return () => {}
  }
}

// =====================================================================
// HTTP 实现：fetch invoke + 单 WS 连接多路分发 + 重连 resync
// =====================================================================

const TOKEN_KEY = 'opx_web_token'
/** 重连退避：1s 起步、2 倍递增、30s 封顶 */
const RECONNECT_BASE_MS = 1_000
const RECONNECT_MAX_MS = 30_000

interface ApiErrorEnvelope {
  code?: string
  message?: string
}

interface WsTicketResponse {
  ticket: string
  expires_in: number
  ws_url: string
}

type WsHandler = (event: { payload: unknown }) => void

class HttpTransport implements Transport {
  readonly isDesktop = false

  private token: string | null = null
  private readonly resyncCbs = new Set<() => void>()
  private readonly unauthorizedCbs = new Set<() => void>()
  /** 事件名 → 订阅者集合（单 WS 多路分发的路由表） */
  private readonly handlers = new Map<string, Set<WsHandler>>()
  private ws: WebSocket | null = null
  /** 进行中的建连 Promise（并发 listen 去重） */
  private connecting: Promise<void> | null = null
  private reconnectTimer: number | null = null
  private backoffMs = RECONNECT_BASE_MS
  private everConnected = false

  constructor() {
    // D3：fragment 携带 token → 转存 sessionStorage 并立即从地址栏剥离
    //（fragment 不随请求上送，但不能停留在 URL/历史记录里）。
    // 批次 4.6：接受新 10 位 Crockford Base32 与旧 64 hex 两种形态。
    // 批次 4.7：本应用是 vue-router **hash 模式**，裸 `#token=X` 会被路由器
    // 当作路由路径解析并重写（#token=X → #/dashboard），token 在本模块被
    // 求值前就已销毁（实测：打开浏览器后仍弹 TokenGate）。故自动登录 URL
    // 改用 router 兼容形态 `#/?token=X`（token 位于 hash 内的 query，
    // vue-router 导航会保留 query，任意求值时序都能读到）；旧裸形态仍兼容读。
    const TOKEN_RE = /^(?:[0-9A-HJKMNP-TV-Za-z]{10}|[0-9a-fA-F]{64})$/
    const raw = window.location.hash.startsWith('#')
      ? window.location.hash.slice(1)
      : window.location.hash
    let candidate: string | null = null
    let bareLegacy = false
    if (raw.startsWith('token=')) {
      // 旧裸形态 #token=X（router 改写前才存在，兼容直开旧链接）
      candidate = raw.slice('token='.length)
      bareLegacy = true
    } else {
      const qi = raw.indexOf('?')
      if (qi >= 0) {
        const t = new URLSearchParams(raw.slice(qi + 1)).get('token')
        if (t) candidate = t
      }
    }
    if (candidate && TOKEN_RE.test(candidate)) {
      sessionStorage.setItem(TOKEN_KEY, candidate)
      // 从地址栏剥离：保留路由路径、去掉 token query；裸形态路径本身已被
      // 路由污染，直接回落 `#/`
      const pathPart = bareLegacy ? '/' : (raw.split('?')[0] || '/')
      window.history.replaceState(null, '', `${window.location.pathname}#${pathPart}`)
    }
    this.token = sessionStorage.getItem(TOKEN_KEY)
  }

  get authenticated(): boolean {
    return !!this.token
  }

  setToken(token: string): void {
    const t = token.trim()
    if (t) {
      this.token = t
      sessionStorage.setItem(TOKEN_KEY, t)
    } else {
      this.token = null
      sessionStorage.removeItem(TOKEN_KEY)
    }
    // 已有订阅者等待事件（如 401 后重输 token）：立即重试建连
    if (t && this.handlers.size > 0) {
      void this.ensureConnection().catch(() => {})
    }
  }

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (!this.token) throw 'i18n:webTokenRequired'
    // 顶层键 camelCase → snake_case（与 Tauri IPC 参数名转换对齐，见模块注释）
    const body = Object.fromEntries(
      Object.entries(args ?? {}).map(([k, v]) => [camelToSnake(k), v]),
    )
    const res = await fetch(`/api/${encodeURIComponent(cmd)}`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${this.token}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(body),
    })
    if (res.status === 401) {
      this.handleUnauthorized()
      throw 'i18n:webTokenInvalid'
    }
    if (!res.ok) {
      // 统一错误信封（D5）：message 原样抛出（含 i18n:key），ipc.ts 转译
      let message = `HTTP ${res.status}`
      try {
        const j = (await res.json()) as ApiErrorEnvelope
        if (typeof j?.message === 'string' && j.message) message = j.message
      } catch {
        // 非 JSON 响应（如中间层纯文本错误）保留默认消息
      }
      throw message
    }
    return (await res.json()) as T
  }

  async listen<T>(
    event: string,
    handler: (event: { payload: T }) => void,
  ): Promise<UnlistenFn> {
    let set = this.handlers.get(event)
    if (!set) {
      set = new Set()
      this.handlers.set(event, set)
    }
    set.add(handler as WsHandler)
    if (this.token) void this.ensureConnection().catch(() => {})
    return () => {
      const s = this.handlers.get(event)
      if (!s) return
      s.delete(handler as WsHandler)
      if (s.size === 0) this.handlers.delete(event)
    }
  }

  onResync(cb: () => void): UnlistenFn {
    this.resyncCbs.add(cb)
    return () => this.resyncCbs.delete(cb)
  }

  onUnauthorized(cb: () => void): UnlistenFn {
    this.unauthorizedCbs.add(cb)
    return () => this.unauthorizedCbs.delete(cb)
  }

  // ------------------------- 内部：连接管理 -------------------------

  /** token 被拒：清除本地凭据并通知 TokenGate 重新展示输入页 */
  private handleUnauthorized(): void {
    this.token = null
    sessionStorage.removeItem(TOKEN_KEY)
    for (const cb of [...this.unauthorizedCbs]) {
      try {
        cb()
      } catch {
        // 监听者异常不阻断其余通知
      }
    }
  }

  /** 幂等建连：已有连接/建连中直接复用 */
  private ensureConnection(): Promise<void> {
    if (this.connecting) return this.connecting
    if (
      this.ws &&
      (this.ws.readyState === WebSocket.OPEN ||
        this.ws.readyState === WebSocket.CONNECTING)
    ) {
      return Promise.resolve()
    }
    this.connecting = this.connect().finally(() => {
      this.connecting = null
    })
    return this.connecting
  }

  /** 单次建连：换 ticket → WebSocket 握手 → 挂接消息/断线处理 */
  private async connect(): Promise<void> {
    const token = this.token
    if (!token) throw new Error('no token')
    const res = await fetch('/api/ws-ticket', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
    })
    if (res.status === 401) {
      this.handleUnauthorized()
      throw new Error('unauthorized')
    }
    if (!res.ok) throw new Error(`ws-ticket HTTP ${res.status}`)
    const { ticket, ws_url } = (await res.json()) as WsTicketResponse
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const ws = new WebSocket(
      `${proto}//${window.location.host}${ws_url}?ticket=${encodeURIComponent(ticket)}`,
    )
    this.ws = ws
    // opened 标记区分「未握手成功的失败」与「长连接中断」——只有前者退避翻倍
    let opened = false
    ws.onopen = () => {
      opened = true
      this.backoffMs = RECONNECT_BASE_MS
      const firstConnect = !this.everConnected
      this.everConnected = true
      // D4：重连成功（非首连）→ 补偿刷新。首连时订阅方自己会拉初始数据
      if (!firstConnect) this.fireResync()
    }
    ws.onmessage = (ev: MessageEvent) => {
      // 帧格式定稿（4.3）：一帧一条 JSON 文本 {"event","payload"}
      try {
        const frame = JSON.parse(String(ev.data)) as {
          event: string
          payload: unknown
        }
        const set = this.handlers.get(frame.event)
        if (!set) return
        for (const h of [...set]) {
          try {
            h({ payload: frame.payload })
          } catch {
            // 单个订阅者异常不影响其余订阅者
          }
        }
      } catch {
        // 非 JSON 帧忽略
      }
    }
    ws.onclose = () => {
      if (this.ws === ws) this.ws = null
      if (!opened) {
        // 握手未成：指数退避（ticket 60s 过期 / 服务暂不可达等）
        this.backoffMs = Math.min(this.backoffMs * 2, RECONNECT_MAX_MS)
      }
      this.scheduleReconnect()
    }
    ws.onerror = () => {
      // onclose 必然跟随触发，重连逻辑集中在那里
    }
  }

  private fireResync(): void {
    for (const cb of [...this.resyncCbs]) {
      try {
        cb()
      } catch {
        // 补偿回调异常不拖垮连接
      }
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) return
    // 无 token（被拒等用户输入）或无订阅者时不再自动重连；
    // setToken / 新 listen 会重新拉起连接
    if (!this.token || this.handlers.size === 0) return
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null
      void this.ensureConnection().catch(() => {})
    }, this.backoffMs)
  }
}

/** camelCase → snake_case（仅顶层参数键，对齐 Tauri IPC 行为） */
function camelToSnake(key: string): string {
  return key.replace(/[A-Z]/g, (c) => '_' + c.toLowerCase())
}

// ------------------------- 单例 -------------------------

let instance: Transport | null = null

/** 应用启动后首次调用时选定通道（探测结果进程内不变） */
export function getTransport(): Transport {
  if (!instance) instance = IS_DESKTOP ? new TauriTransport() : new HttpTransport()
  return instance
}

// ------------------------- 模块级便捷导出 -------------------------
//
// 与 @tauri-apps/api/event 同形：15 个 listen 调用点机械换 import 即可，
// 无需改成 `getTransport().listen(...)` 的调用形态。

export function listen<T = unknown>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<UnlistenFn> {
  return getTransport().listen<T>(event, handler)
}

/** 桌面专属入口（quit/exit/hide/更新器等）按此隐藏 UI */
export const isDesktop = IS_DESKTOP
