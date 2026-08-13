// 从完整 MDI 图标集提取应用实际使用的图标，生成精简离线子集。
// 自动扫描 src/ 和 src-tauri/src/（后端 catalog 图标）中的 "mdi:xxx" 用法。
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const root = resolve(__dirname, '..')

// 递归收集目录下 .vue/.ts/.rs 文件
function walk(dir) {
  let files = []
  for (const name of readdirSync(dir)) {
    const p = resolve(dir, name)
    if (statSync(p).isDirectory()) files = files.concat(walk(p))
    else if (name.endsWith('.vue') || name.endsWith('.ts') || name.endsWith('.rs')) files.push(p)
  }
  return files
}

// 扫描源码里的 icon 用法：icon="mdi:xxx"、:icon="'mdi:xxx'"、icon: "mdi:xxx"、catalog 里的 icon 字段
const iconRe = /mdi:([\w-]+)/g
const USED_ICONS = [...new Set(
  walk(resolve(root, 'src')).concat(walk(resolve(root, 'src-tauri/src'))).flatMap((f) => {
    const text = readFileSync(f, 'utf8')
    const found = []
    let m
    while ((m = iconRe.exec(text)) !== null) found.push(m[1])
    return found
  })
)].sort()

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
