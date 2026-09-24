// 全局确认对话框（替代原生 window.confirm，后者在 Tauri WebView2 中不可靠）。
// 模块级单例状态 + promise 队列：confirmAsync 返回 Promise<boolean>。
import { ref } from 'vue'

interface ConfirmState {
  visible: boolean
  message: string
  title: string
  okText: string
  cancelText: string
  danger: boolean
  resolver: ((v: boolean) => void) | null
}

const state = ref<ConfirmState>({
  visible: false,
  message: '',
  title: '',
  okText: '',
  cancelText: '',
  danger: false,
  resolver: null,
})

/** confirmAsync：在确认对话框弹出并得到用户选择后 resolve(true/false) */
export function confirmAsync(
  message: string,
  opts?: { title?: string; okText?: string; cancelText?: string; danger?: boolean },
): Promise<boolean> {
  // 同一时刻只允许一个确认；若已有未决的，直接拒绝新请求
  if (state.value.resolver) return Promise.resolve(false)
  return new Promise<boolean>((resolve) => {
    state.value.visible = true
    state.value.message = message
    state.value.title = opts?.title ?? ''
    state.value.okText = opts?.okText ?? ''
    state.value.cancelText = opts?.cancelText ?? ''
    state.value.danger = opts?.danger ?? false
    state.value.resolver = (v) => resolve(v)
  })
}

export function useConfirm() {
  function resolve(value: boolean) {
    const r = state.value.resolver
    state.value.visible = false
    state.value.resolver = null
    r?.(value)
  }
  return { state, resolve, confirm: confirmAsync }
}
