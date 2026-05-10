/**
 * 图片 URL 代理工具。
 *
 * 默认保留远程 HTTP(S) 原始链接，只有在图片加载 / 像素读取失败时，
 * 才转换为 imgproxy:// 自定义协议交给 Tauri Rust 后端兜底。
 */

const PROXY_PREFIX = 'imgproxy://localhost/'
const FALLBACK_MARK = 'imageProxyFallbackApplied'

export function isProxiedImageUrl(url: string): boolean {
  return url.startsWith(PROXY_PREFIX) || url.startsWith('imgproxy://')
}

export function isRemoteHttpImageUrl(url: string): boolean {
  return url.startsWith('http://') || url.startsWith('https://')
}

export function canProxyImageUrl(url: string): boolean {
  return !!url && isRemoteHttpImageUrl(url) && !isProxiedImageUrl(url)
}

export function getOriginalImageUrl(url: string): string {
  if (!url.startsWith(PROXY_PREFIX)) return url

  const encoded = url.slice(PROXY_PREFIX.length)
  try {
    return decodeURIComponent(encoded)
  } catch {
    return url
  }
}

/**
 * 将外部图片 URL 转为 imgproxy:// 自定义协议 URL。
 * 仅用于失败兜底，不应在数据进入前端时提前重写。
 */
export function proxyImageUrl(url: string): string {
  if (!canProxyImageUrl(url)) return url
  return `${PROXY_PREFIX}${encodeURIComponent(url)}`
}

/**
 * direct-first 语义：保留原始 URL，仅规范化历史已代理 URL 以外的非远程值。
 */
export function directImageUrl(url: string): string {
  return getOriginalImageUrl(url)
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
      record[field] = directImageUrl(record[field] as string)
    }
  }
  for (const key of Object.keys(record)) {
    if (typeof record[key] === 'object' && record[key] !== null) {
      rewriteImageUrls(record[key])
    }
  }
  return obj
}

function handleImageError(event: Event): void {
  const target = event.target
  if (!(target instanceof HTMLImageElement)) return

  const declaredSrc = target.getAttribute('src') || ''
  if (declaredSrc && !canProxyImageUrl(declaredSrc)) return

  const currentSrc = target.currentSrc || target.src || declaredSrc
  const fallbackSource = declaredSrc || currentSrc
  if (!canProxyImageUrl(fallbackSource)) return
  if (target.dataset[FALLBACK_MARK] === fallbackSource) return

  const fallbackUrl = proxyImageUrl(fallbackSource)
  if (fallbackUrl === currentSrc) return

  target.dataset[FALLBACK_MARK] = fallbackSource
  target.src = fallbackUrl
}

let imageFallbackInstalled = false

/**
 * 安装全局 <img> direct-first 失败兜底。
 * 使用捕获阶段监听 error，因为 img 的 error 事件不会冒泡。
 */
export function installImageProxyFallback(): () => void {
  if (imageFallbackInstalled) return () => {}

  window.addEventListener('error', handleImageError, true)
  imageFallbackInstalled = true

  return () => {
    window.removeEventListener('error', handleImageError, true)
    imageFallbackInstalled = false
  }
}
