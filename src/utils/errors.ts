import { i18n } from './i18n'

const errorMap: Record<string, string> = {
  ERR_NGINX_NOT_RUNNING: 'nginxNotRunning',
  ERR_SITE_RUNNING_DELETE: 'deleteRunningHint',
  ERR_NOT_FOUND: 'unknown',
  ERR_SAVE_FAILED: 'configSaveFailed',
  ERR_SITE_NAME_CJK: 'siteNameNoCJK',
  ERR_SITE_NAME_EMPTY: 'siteNameRequired',
  ERR_NGINX_CONF_FAILED: 'nginxNotRunning',
}

/** 将后端错误消息翻译为当前语言。若为已知错误码则查表，否则返回原文。 */
export function translateError(err: unknown): string {
  const msg = typeof err === 'string' ? err : String(err)
  const match = msg.match(/^ERR_(\w+):/)
  if (match) {
    const key = errorMap[match[1]]
    if (key) return i18n.global.t(key)
  }
  return msg
}
