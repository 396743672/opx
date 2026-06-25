import zhCN from '../locales/zh-CN'
import enUS from '../locales/en-US'

export type Messages = typeof zhCN

export function loadMessages() {
  return {
    'zh-CN': zhCN,
    'en-US': enUS,
  }
}