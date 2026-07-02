import { createI18n } from 'vue-i18n'
import zhCN from '../locales/zh-CN'
import enUS from '../locales/en-US'

export type Messages = typeof zhCN

export function loadMessages() {
  return {
    'zh-CN': zhCN,
    'en-US': enUS,
  }
}

/// localStorage 缓存的语言 key
const LANG_STORAGE_KEY = 'opx_language'

/// 读取上次语言（localStorage 优先，无则默认 zh-CN）
function loadInitialLocale(): string {
  try {
    const saved = localStorage.getItem(LANG_STORAGE_KEY)
    if (saved === 'zh-CN' || saved === 'en-US') {
      return saved
    }
  } catch {
    // localStorage 不可用时静默回退
  }
  return 'zh-CN'
}

export const i18n = createI18n({
  legacy: false,
  locale: loadInitialLocale(),
  fallbackLocale: 'zh-CN',
  messages: loadMessages(),
})
