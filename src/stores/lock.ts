import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@/utils/ipc'
import router from '@/router'

/** 闲置自动锁定时长（分钟）。仅「有锁」时生效。 */
export const IDLE_MINUTES = 5
const IDLE_MS = IDLE_MINUTES * 60 * 1000

export const useLockStore = defineStore('lock', () => {
  /** 是否已设置锁屏密码（锁开关状态） */
  const hasPassword = ref(false)
  /** 当前是否已解锁（内存态，绝不持久化） */
  const unlocked = ref(true)
  /** 被锁重定向前的目标路由，解锁后回跳 */
  const pendingRoute = ref<string | null>(null)
  let idleTimer: number | null = null

  /** 从后端拉取锁开关状态，并据此设定 unlocked（有锁→锁住；无锁→解锁） */
  async function refresh() {
    try {
      hasPassword.value = await invoke<boolean>('has_lock_password')
    } catch {
      hasPassword.value = false
    }
    unlocked.value = !hasPassword.value
  }

  async function init() {
    await refresh()
    setupIdle()
  }

  /** 校验密码；成功则解锁并回跳目标路由。keyring 不可用等错误会向上抛出（已转译） */
  async function unlock(pw: string): Promise<boolean> {
    const ok = await invoke<boolean>('verify_lock_password', { pw })
    if (ok) {
      unlocked.value = true
      const target = pendingRoute.value ?? '/dashboard'
      pendingRoute.value = null
      router.push(target)
      // 解锁后重新挂上闲置计时器：之后任意 5 分钟无操作都会再次自动锁屏。
      // （解锁点击发生时 unlocked 仍为 false，事件续期不会生效，必须显式续期。）
      resetIdle()
    }
    return ok
  }

  /** 手动 / 自动锁定。仅在有锁时生效。置为锁定态并跳转到锁屏页（守卫据此渲染）。 */
  function lock() {
    if (hasPassword.value) {
      unlocked.value = false
      pendingRoute.value = router.currentRoute.value.fullPath
      router.push('/lock')
    }
  }

  /** 启用 / 更换锁屏密码 */
  async function setPassword(pw: string) {
    await invoke('set_lock_password', { pw })
    hasPassword.value = true
    unlocked.value = true
    // 启锁后立即开始闲置监控（否则要等下一次用户事件才挂上计时器）
    resetIdle()
  }

  /** 清空锁屏密码（关闭锁） */
  async function clearPassword() {
    await invoke('clear_lock_password')
    hasPassword.value = false
    unlocked.value = true
  }

  function resetIdle() {
    if (!unlocked.value) return
    if (idleTimer) window.clearTimeout(idleTimer)
    idleTimer = window.setTimeout(() => {
      if (hasPassword.value && unlocked.value) lock()
    }, IDLE_MS)
  }

  function setupIdle() {
    const events = ['mousemove', 'mousedown', 'keydown', 'click', 'touchstart']
    events.forEach((ev) =>
      document.addEventListener(ev, resetIdle, { passive: true })
    )
    resetIdle()
  }

  return {
    hasPassword,
    unlocked,
    pendingRoute,
    init,
    refresh,
    unlock,
    lock,
    setPassword,
    clearPassword,
    IDLE_MINUTES,
  }
})
