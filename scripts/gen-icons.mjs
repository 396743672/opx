// 从完整 MDI 图标集提取应用实际使用的图标，生成精简离线子集。
// 新增图标用法后，更新下方 USED_ICONS 列表并运行：node scripts/gen-icons.mjs
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = resolve(__dirname, '..')

// 应用中使用的 mdi 图标（不含 "mdi:" 前缀）
const USED_ICONS = [
  'alert-circle',
  'arrow-down-bold',
  'arrow-up-bold',
  'check',
  'check-circle',
  'chevron-down',
  'close',
  'cog',
  'cpu-64-bit',
  'database',
  'download',
  'download-box',
  'gauge',
  'harddisk',
  'information-outline',
  'lan',
  'laptop',
  'leaf',
  'loading',
  'memory',
  'menu',
  'monitor',
  'moon-waning-crescent',
  'package-variant-closed',
  'play-circle',
  'plus-box',
  'power',
  'progress-clock',
  'refresh',
  'server',
  'star',
  'theme-light-dark',
  'upload',
  'weather-sunny',
  'web',
  'window-close',
]

const full = JSON.parse(
  readFileSync(resolve(root, 'node_modules/@iconify-json/mdi/icons.json'), 'utf8')
)

const subset = {
  prefix: full.prefix,
  icons: {},
}
if (full.width) subset.width = full.width
if (full.height) subset.height = full.height

const missing = []
for (const name of USED_ICONS) {
  if (full.icons[name]) {
    subset.icons[name] = full.icons[name]
  } else if (full.aliases && full.aliases[name]) {
    // 解析别名：把别名指向的父图标一并纳入
    const parent = full.aliases[name].parent
    subset.icons[name] = full.aliases[name]
    if (parent && full.icons[parent]) subset.icons[parent] = full.icons[parent]
  } else {
    missing.push(name)
  }
}

if (missing.length) {
  console.error('未找到的图标:', missing.join(', '))
  process.exit(1)
}

const outDir = resolve(root, 'src/assets')
mkdirSync(outDir, { recursive: true })
const outPath = resolve(outDir, 'mdi-icons.json')
writeFileSync(outPath, JSON.stringify(subset))
console.log(`已生成 ${Object.keys(subset.icons).length} 个图标 → src/assets/mdi-icons.json`)
