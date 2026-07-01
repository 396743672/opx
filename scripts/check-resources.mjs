#!/usr/bin/env node
/**
 * 校验 src-tauri/resources/software/ 下的 zip 完整性
 * - 检查 manifest.json 中每个条目对应的 zip 文件存在
 * - 校验 sha256 匹配
 *
 * 退出码：0 全部通过，1 有缺失或不匹配
 */
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, statSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const RESOURCES_DIR = join(__dirname, '..', 'src-tauri', 'resources', 'software')
const MANIFEST_PATH = join(RESOURCES_DIR, 'manifest.json')

function sha256(filePath) {
  const buf = readFileSync(filePath)
  return createHash('sha256').update(buf).digest('hex')
}

let errors = 0

if (!existsSync(MANIFEST_PATH)) {
  console.error('✗ manifest.json 不存在，请先运行 npm run fetch:builtin')
  process.exit(1)
}

const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'))

for (const [key, versions] of Object.entries(manifest)) {
  for (const [version, info] of Object.entries(versions)) {
    const zipPath = join(RESOURCES_DIR, key, `${version}.zip`)
    if (!existsSync(zipPath)) {
      console.error(`✗ 缺失: ${key}/${version}.zip`)
      errors++
      continue
    }
    const actualHash = sha256(zipPath)
    if (actualHash !== info.sha256) {
      console.error(`✗ sha256 不匹配: ${key}/${version}.zip`)
      console.error(`  期望: ${info.sha256}`)
      console.error(`  实际: ${actualHash}`)
      errors++
      continue
    }
    const actualSize = statSync(zipPath).size
    if (actualSize !== info.size) {
      console.error(`✗ size 不匹配: ${key}/${version}.zip`)
      console.error(`  期望: ${info.size}`)
      console.error(`  实际: ${actualSize}`)
      errors++
      continue
    }
    console.log(`✓ ${key}/${version}.zip`)
  }
}

if (errors > 0) {
  console.error(`\n✗ ${errors} 个错误，请运行 npm run fetch:builtin -- --force 重新下载`)
  process.exit(1)
}
console.log('\n✓ 全部校验通过')
