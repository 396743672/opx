// 统一的 Tauri invoke 包装：把后端返回的 "i18n:key" 错误码就地转译为当前语言的文案，
// 一次覆盖全部已注册命令，无需在每个显示点单独挂 translateError。
//
// 两个刻意的设计决定：
// 1. reject 时抛「字符串」而非 Error，与 Tauri 原生行为保持一致，
//    从而兼容现有 `typeof e === 'string' ? e : e?.message` 的调用点写法（已机械替换 import 的 36 个文件无需改动）。
// 2. 转译发生在抛错那一刻、读取运行时 i18n 实例，因此自动跟随语言切换。
//
// 注意：只覆盖 invoke 的 reject。来自事件 payload（如 ACME 进度）或前端本地拼装的错误串
// 不经过这里，仍需在显示点用 translateError 兜底。
import { invoke as tauriInvoke, type InvokeArgs } from '@tauri-apps/api/core'
import { i18n } from './i18n'
import { translateError } from './i18nError'

export async function invoke<T = unknown>(cmd: string, args?: InvokeArgs): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args)
  } catch (e) {
    const raw = typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
    throw translateError(
      raw,
      i18n.global.t as (k: string) => string,
      i18n.global.te as (k: string) => boolean
    )
  }
}
