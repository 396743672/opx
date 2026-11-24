/**
 * 「访问端口」链接的主机名解析（软件卡片端口芯片、编排面板端口入口共用）。
 *
 * 🚨 不能硬编码 127.0.0.1：Web 模式下浏览器跑在管理机上，127.0.0.1 指向
 * 浏览器所在机器而非服务器——点端口芯片永远打不开（2026-11-24 实测 MinIO
 * 控制台 9001 芯片即此）。
 *
 * 规则：
 * - Web 模式：取浏览器地址栏的 hostname（即用户访问 opx 用的服务器地址，
 *   一定可达；SSH 隧道场景取回 127.0.0.1 也仍然正确）。
 * - 桌面模式：GUI 与服务同机，127.0.0.1 通常可达；但服务可能只 bind 具体
 *   网卡 IP（MySQL/Redis 按系统「对外访问地址」监听），此时回环连不上，
 *   所以配了具体 IP 就优先用它，未配/为通配值时回退 127.0.0.1。
 */
import { isDesktop } from '@/utils/transport'
import { useSettingsStore } from '@/stores/settings'

/** 这些值不指向具体网卡，不能当访问主机名用 */
const NON_HOST_VALUES = new Set(['', 'auto', '0.0.0.0', '::', '127.0.0.1', '::1', 'localhost'])

export function webAccessHost(): string {
  if (!isDesktop) return window.location.hostname || '127.0.0.1'
  const bind = (useSettingsStore().settings?.service_bind_address ?? '').trim().toLowerCase()
  if (bind && !NON_HOST_VALUES.has(bind)) return bind
  return '127.0.0.1'
}
