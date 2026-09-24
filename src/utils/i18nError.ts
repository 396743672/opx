// 后端错误可能带 "i18n:key" 标记，且外层可能包前缀（如 "启动失败：i18n:xxx"）。
// 提取 i18n:key 并调 t() 转译；key 不存在或无标记则显示原文（避免暴露裸 key）。
// 供卡片/列表等非 ErrorDialog 的错误显示处复用同一转译逻辑。
export function translateError(
  raw: string | null | undefined,
  t: (key: string) => string,
  te: (key: string) => boolean
): string {
  if (!raw) return ''
  const m = raw.match(/i18n:([A-Za-z0-9_.-]+)/)
  if (!m || !te(m[1])) return raw
  // 只替换 "i18n:key" 片段，保留外层前缀（如「删除失败：」）；
  // 用函数式替换，避免译文里的 $ 被当成替换模式。
  return raw.replace(m[0], () => t(m[1]))
}