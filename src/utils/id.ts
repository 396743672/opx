/**
 * 生成 v4 UUID（前端本地 id，用于新建站点/账号/分组等尚未落库的对象）。
 *
 * 🚨 不要直接用 `crypto.randomUUID()`：按规范它只在**安全上下文**（HTTPS 或
 * localhost/127.0.0.1）可用。Web 模式常以 `http://<局域网 IP>:17580` 打开，
 * 此时 `crypto.randomUUID` 是 `undefined`，调用会抛 TypeError——按钮点下去
 * 什么都不发生，控制台才看得到 `crypto.randomUUID is not a function`
 * （2026-11-23 实测「新建站点无反应」即此）。
 *
 * `crypto.getRandomValues` 是 Crypto 接口里唯一允许在非安全上下文使用的成员
 * （MDN: "the only member of the Crypto interface which can be used from an
 * insecure context"），用它做 fallback，两侧行为一致。
 */
export function newId(): string {
  const c: Crypto | undefined = globalThis.crypto
  if (c && typeof c.randomUUID === 'function') {
    try {
      return c.randomUUID()
    } catch {
      // 个别浏览器在非安全上下文仍暴露该方法但调用即抛错，走 fallback
    }
  }
  const bytes = new Uint8Array(16)
  if (c && typeof c.getRandomValues === 'function') {
    c.getRandomValues(bytes)
  } else {
    // 极端兜底（非浏览器测试环境）：不保证密码学强度，仅需唯一即可
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256)
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40 // version 4
  bytes[8] = (bytes[8] & 0x3f) | 0x80 // variant 10xx
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}
