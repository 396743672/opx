// 全局弹出提示（toast）：模块级响应式状态，供任意组件调用。
import { ref } from 'vue'

export type ToastKind = 'ok' | 'err' | 'info'

interface ToastItem {
  id: number
  message: string
  kind: ToastKind
}

const toasts = ref<ToastItem[]>([])
let seq = 0

/** toast：显示一条自动消失的弹出提示 */
export function toast(message: string, kind: ToastKind = 'info') {
  const id = ++seq
  toasts.value.push({ id, message, kind })
  setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }, 3000)
}

export function useToast() {
  return { toasts, toast }
}

/** 给 ToastHost 组件使用的响应式状态 */
export const toastState = toasts
