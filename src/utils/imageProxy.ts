/**
 * 将外部图片 URL 转为 imgproxy:// 自定义协议 URL，
 * 由 Tauri Rust 后端代理请求，绕过 WebView CORS 限制。
 */

export function proxyImageUrl(url: string): string {
  if (
    !url ||
    url.startsWith('data:') ||
    url.startsWith('blob:') ||
    url.startsWith('imgproxy://') ||
    url.startsWith('/') ||
    url.startsWith('@') ||
    !(url.startsWith('http://') || url.startsWith('https://'))
  )
    return url
  return `imgproxy://localhost/${encodeURIComponent(url)}`
}

const IMAGE_FIELDS = ['img', 'avatar', 'cover', 'pic', 'imgUrl', 'coverUrl', 'picUrl', 'albumImg'] as const

export function rewriteImageUrls(obj: unknown): unknown {
  if (!obj || typeof obj !== 'object') return obj

  if (Array.isArray(obj)) {
    for (const item of obj) rewriteImageUrls(item)
    return obj
  }

  const record = obj as Record<string, unknown>
  for (const field of IMAGE_FIELDS) {
    if (typeof record[field] === 'string') {
      record[field] = proxyImageUrl(record[field] as string)
    }
  }
  for (const key of Object.keys(record)) {
    if (typeof record[key] === 'object' && record[key] !== null) {
      rewriteImageUrls(record[key])
    }
  }
  return obj
}
