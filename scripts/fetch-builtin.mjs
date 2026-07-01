#!/usr/bin/env node
/**
 * 下载内置 zip 到 src-tauri/resources/software/{key}/{version}.zip
 * 生成 manifest.json（sha256 + size）
 *
 * 用法：
 *   node scripts/fetch-builtin.mjs          # 下载缺失的 zip
 *   node scripts/fetch-builtin.mjs --force  # 强制重新下载
 */
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawn } from 'node:child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const RESOURCES_DIR = join(__dirname, '..', 'src-tauri', 'resources', 'software')

const BUILTIN = {
  jre: {
    '17.0.15':
      'https://github.com/adoptium/temurin17-binaries/releases/download/jdk-17.0.10%2B7/OpenJDK17U-jre_x64_windows_hotspot_17.0.10_7.zip',
    '1.8':
      'https://github.com/adoptium/temurin8-binaries/releases/download/jdk8u422-b05/OpenJDK8U-jre_x64_windows_hotspot_8u422b05.zip',
  },
  mysql: {
    '8.4.0': 'https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.0-winx64.zip',
  },
  redis: {
    '7.4.9':
      'https://github.com/redis-windows/redis-windows/releases/download/7.4.9/Redis-7.4.9-Windows-x64-cygwin.zip',
  },
  nginx: {
    '1.31.2': 'https://mirrors.huaweicloud.com/nginx/nginx-1.31.2.zip',
  },
}

const force = process.argv.includes('--force')

function sha256(filePath) {
  const buf = readFileSync(filePath)
  return createHash('sha256').update(buf).digest('hex')
}

function download(url, dest) {
  // 使用 curl 子进程下载：自动跟随重定向、自动读取 HTTP(S)_PROXY 环境变量。
  // 本环境的 Node fetch 在 GitHub release 资源上会 ECONNRESET，curl 无此问题。
  return new Promise((resolve, reject) => {
    const args = [
      '--location', // 跟随 302 重定向
      '--fail', // 4xx/5xx 返回非 0 退出码
      '--show-error',
      '--silent', // 默认不打印进度条，用 --progress-bar 单独开
      '--progress-bar',
      '--output',
      dest,
      url,
    ]
    const proc = spawn('curl', args, { stdio: 'inherit' })
    proc.on('error', (e) => {
      if (e.code === 'ENOENT') {
        reject(new Error('未找到 curl 命令，请确认系统已安装 curl 并在 PATH 中'))
      } else {
        reject(e)
      }
    })
    proc.on('close', (code) => {
      if (code === 0) resolve()
      else reject(new Error(`curl 退出码 ${code}`))
    })
  })
}

async function main() {
  mkdirSync(RESOURCES_DIR, { recursive: true })
  const manifest = {}

  for (const [key, versions] of Object.entries(BUILTIN)) {
    manifest[key] = {}
    for (const [version, url] of Object.entries(versions)) {
      const keyDir = join(RESOURCES_DIR, key)
      mkdirSync(keyDir, { recursive: true })
      const zipPath = join(keyDir, `${version}.zip`)

      if (existsSync(zipPath) && !force) {
        console.log(`✓ 跳过已存在: ${key}/${version}.zip`)
      } else {
        console.log(`↓ 下载: ${key}/${version}.zip`)
        console.log(`  URL: ${url}`)
        await download(url, zipPath)
      }

      const hash = sha256(zipPath)
      const size = statSync(zipPath).size
      manifest[key][version] = { sha256: hash, size }
      console.log(`  sha256: ${hash.slice(0, 16)}... size: ${size}`)
    }
  }

  const manifestPath = join(RESOURCES_DIR, 'manifest.json')
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n')
  console.log(`\n✓ manifest.json 已生成: ${manifestPath}`)
}

main().catch((e) => {
  console.error('✗ 失败:', e.message)
  process.exit(1)
})
