/**
 * 浏览器端文件读取的小工具。
 *
 * Web 模式没有本地文件系统路径（`@tauri-apps/plugin-dialog` 的 `open` 不可用），
 * 上传一律走「`<input type="file">` → base64 → invoke 后端落盘」。
 * 三处上传（JAR / 站点 zip / Node 入口）共用本函数，避免各写一份分块拼接逻辑。
 */

/** 单次 `String.fromCharCode` 的字节数：过大（如整体展开）会爆调用栈。 */
const CHUNK = 0x8000

/** 把 `File` 读成标准 base64（不含 `data:` 前缀）。 */
export async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ''
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(binary)
}
