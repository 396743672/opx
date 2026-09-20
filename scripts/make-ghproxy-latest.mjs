// 读取 tauri-action 生成的 latest.json，把每个 platform 的安装包 url 改写为
// ghfast.top GitHub 镜像地址，写出 latest-ghproxy.json。
// 签名（signature）保持不变——它校验的是安装包二进制内容，与下载地址无关。
import { readFileSync, writeFileSync } from 'node:fs'

const input = process.argv[2]
if (!input) {
  console.error('usage: node scripts/make-ghproxy-latest.mjs <latest.json>')
  process.exit(1)
}

const MIRROR = 'https://ghfast.top'
const data = JSON.parse(readFileSync(input, 'utf-8'))

for (const [key, plat] of Object.entries(data.platforms ?? {})) {
  if (plat && typeof plat.url === 'string' && plat.url.length > 0) {
    plat.url = `${MIRROR}/${plat.url}`
    console.log(`[${key}] -> ${plat.url}`)
  }
}

const out = input.replace(/latest\.json$/, 'latest-ghproxy.json')
writeFileSync(out, JSON.stringify(data, null, 2))
console.log('wrote', out)
