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

export const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: loadMessages(),
})
