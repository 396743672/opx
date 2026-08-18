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
  return m && te(m[1]) ? t(m[1]) : raw
}