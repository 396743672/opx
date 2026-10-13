// 把三平台构建产物（dist 目录）合并成 Tauri updater 用的多平台 latest.json。
//
// 背景：updater 端点（tauri.conf.json plugins.updater.endpoints）指向单个
// latest.json，其 platforms 字段按「平台键 → { url, signature }」组织。
// 三平台 matrix 构建各自产出安装包与 .sig，本脚本在 publish 阶段把它们
// 汇总成一份 latest.json（Windows / Linux / macOS 各条目）。
//
// 平台键与产物约定（Tauri 官方键名）：
//   windows-x86_64  → opx_<v>_x64-setup.exe          NSIS 安装版（已装用户被动升级）
//   linux-x86_64    → opx_<v>_amd64.AppImage.tar.gz  AppImage 更新包；deb/rpm 不走
//                     updater（由 apt/dnf 包管理器升级），仍随 Release 发布供手动安装
//   darwin-x86_64   → opx_universal.app.tar.gz       universal 一条构建覆盖两架构，
//   darwin-aarch64  → 同上                           两个键指向同一产物
//
// 注意：windows 条目显式指向 setup.exe（而非 .msi）——老用户是 NSIS 装的，
// 指向 msi 会让更新时被 msiexec 再装一遍造成双份安装。
// signature 取 <产物>.sig 文件内容，校验的是二进制本身，与下载地址无关。
import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'

const [distDir, tag, outPath, notesPath] = process.argv.slice(2)
if (!distDir || !tag || !outPath) {
  console.error('usage: node scripts/merge-latest.mjs <dist-dir> <tag> <out.json> [notes.md]')
  process.exit(1)
}

// Actions 环境自带 GITHUB_REPOSITORY；本地测试时回退到默认仓库
const REPO = process.env.GITHUB_REPOSITORY ?? '396743672/opx'
const assetUrl = (name) => `https://github.com/${REPO}/releases/download/${tag}/${name}`
const version = tag.replace(/^v/, '')

const files = readdirSync(distDir)
const find = (pattern) => files.find((f) => pattern.test(f))

function entry(pattern, label) {
  const file = find(pattern)
  if (!file) {
    console.log(`[${label}] 无匹配产物（${pattern}），跳过`)
    return null
  }
  const sigPath = join(distDir, `${file}.sig`)
  if (!existsSync(sigPath)) {
    throw new Error(`[${label}] 产物 ${file} 缺少签名文件 ${file}.sig（createUpdaterArtifacts 未生效？）`)
  }
  const signature = readFileSync(sigPath, 'utf-8').trim()
  if (!signature) throw new Error(`[${label}] 签名文件 ${file}.sig 为空`)
  console.log(`[${label}] ${file}`)
  return { url: assetUrl(file), signature }
}

const platforms = {}

const windows = entry(/^opx_.+_x64-setup\.exe$/, 'windows-x86_64')
if (windows) platforms['windows-x86_64'] = windows

const linux = entry(/^opx_.+_amd64\.AppImage\.tar\.gz$/, 'linux-x86_64')
if (linux) platforms['linux-x86_64'] = linux

// universal 构建：Intel 与 Apple Silicon 同一产物，两个平台键都指向它。
// 实际产物名是 opx.app.tar.gz（bundler 对 bundle/macos/opx.app 直接加 .tar.gz，无版本号）
const darwin = entry(/^opx.*\.app\.tar\.gz$/, 'darwin')
if (darwin) {
  platforms['darwin-x86_64'] = darwin
  platforms['darwin-aarch64'] = darwin
}

const keys = Object.keys(platforms)
if (keys.length === 0) {
  console.error(`dist 目录 ${distDir} 中没有任何可发布的平台产物`)
  process.exit(1)
}

const notes = notesPath && existsSync(notesPath) ? readFileSync(notesPath, 'utf-8') : ''

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
}

writeFileSync(outPath, `${JSON.stringify(latest, null, 2)}\n`)
console.log(`wrote ${outPath}（平台: ${keys.join(', ')}）`)
