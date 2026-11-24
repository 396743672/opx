/**
 * 运行时错误兜底展示。
 *
 * 为什么需要：前端在事件回调里抛错时，Vue 只会打一行 console.error，界面**完全没反应**
 * ——用户看到的就是「点了没反应」（2026-11-23「Web 中新建站点点击无反应」即此形态，
 * 真实原因是 `crypto.randomUUID` 在非安全上下文不存在）。这类静默失败排查成本极高，
 * 这里统一把未捕获错误渲染成右下角提示条，既不打断操作，也不再无声无息。
 *
 * 刻意不依赖 Vue：组件渲染崩掉时 Vue 已不可靠，DOM 是唯一还能用的通道。
 */
import { i18n } from './i18n'

const BANNER_ID = 'opx-runtime-error'
/** 同一条错误在此窗口内重复出现只展示一次（事件循环里连抛会刷屏） */
const DEDUPE_MS = 3000
const MAX_TEXT = 800

let banner: HTMLElement | null = null
let textEl: HTMLElement | null = null
let lastKey = ''
let lastAt = 0

function tr(key: string, fallback: string): string {
  try {
    const s = i18n.global.t(key)
    return s && s !== key ? s : fallback
  } catch {
    return fallback
  }
}

/** 把任意抛出物转成可读一行文本（错误对象只取 message，避免把整个堆栈糊在界面上） */
function describe(err: unknown): string {
  let text: string
  if (err instanceof Error) text = err.message || err.name
  else if (typeof err === 'string') text = err
  else if (err == null) text = String(err)
  else {
    try {
      text = JSON.stringify(err)
    } catch {
      text = String(err)
    }
  }
  text = (text || tr('runtimeErrorUnknown', '未知错误')).trim()
  return text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}…` : text
}

function ensureBanner(): HTMLElement {
  if (banner && banner.isConnected) return banner
  const root = document.createElement('div')
  root.id = BANNER_ID
  root.setAttribute('role', 'alert')
  root.style.cssText = [
    'position:fixed',
    'right:16px',
    'bottom:16px',
    'z-index:2147483000',
    'max-width:min(460px,calc(100vw - 32px))',
    'padding:10px 12px',
    'border-radius:8px',
    'border:1px solid #991b1b',
    'background:#7f1d1d',
    'color:#fff',
    'font-size:12px',
    'line-height:1.5',
    'box-shadow:0 10px 28px rgba(0,0,0,.38)',
  ].join(';')

  const head = document.createElement('div')
  head.style.cssText = 'display:flex;align-items:center;gap:8px;font-weight:600'

  const title = document.createElement('span')
  title.style.cssText = 'flex:1 1 auto'
  title.textContent = tr('runtimeErrorTitle', '运行出错')

  const close = document.createElement('button')
  close.type = 'button'
  close.textContent = tr('close', '关闭')
  close.style.cssText =
    'flex:none;border:0;background:transparent;color:#fecaca;font-size:12px;cursor:pointer;padding:0;text-decoration:underline'
  close.addEventListener('click', () => {
    banner?.remove()
    banner = null
    textEl = null
  })

  head.append(title, close)

  const body = document.createElement('pre')
  body.style.cssText =
    'margin:6px 0 0;white-space:pre-wrap;word-break:break-word;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:11.5px;max-height:30vh;overflow:auto;color:#fee2e2'

  root.append(head, body)
  document.body.appendChild(root)
  banner = root
  textEl = body
  return root
}

/** 立即把错误展示出来（供全局钩子与 Vue errorHandler 调用） */
export function reportRuntimeError(err: unknown): void {
  const text = describe(err)
  const now = Date.now()
  if (text === lastKey && now - lastAt < DEDUPE_MS) return
  lastKey = text
  lastAt = now

  // 控制台保留完整堆栈：提示条只给人看结论，排查仍看 devtools
  console.error('[opx]', err)

  if (typeof document === 'undefined' || !document.body) return
  ensureBanner()
  if (textEl) textEl.textContent = text
}

/** 安装全局钩子（main.ts 在 mount 之前调用） */
export function installRuntimeErrorHandlers(): void {
  if (typeof window === 'undefined') return
  window.addEventListener('error', (ev) => {
    const e = (ev as ErrorEvent).error
    if (e != null) reportRuntimeError(e)
    else if ((ev as ErrorEvent).message) reportRuntimeError((ev as ErrorEvent).message)
    // 资源加载失败（img/script 404）既无 error 也无 message，按噪音忽略
  })
  window.addEventListener('unhandledrejection', (ev) => {
    reportRuntimeError((ev as PromiseRejectionEvent).reason)
  })
}
