import en from '../locales/en.json'
import zh from '../locales/zh-CN.json'

export function loadMessages() {
  return {
    'en': en,
    'zh-CN': zh
  }
}

export function getLocale() {
  return navigator.language || 'zh-CN'
}