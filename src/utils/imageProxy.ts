/**
 * 图片 URL 代理工具。
 *
 * 桌面端：保留原始 HTTP(S) 链接，加载失败时通过 imgproxy:// 兜底。
 * 移动端（Android）：主动将所有外部图片转为 imgproxy:// 协议，
 *   因为 Android WebView origin 为 https://tauri.localhost，
 *   音乐 CDN 会拒绝非本站 Referer 的图片请求。
 */

const PROXY_PREFIX = 'imgproxy://localhost/'
const FALLBACK_MARK = 'imageProxyFallbackApplied'
const isMobile = /android|iphone|ipad/i.test(navigator.userAgent)

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

/**
 * 根据平台解析图片 URL：
 * - 移动端：外部图片主动转为 imgproxy:// 协议
 * - 桌面端：返回原始直链
 */
export function resolveImageUrl(url: string): string {
  if (!isMobile) return directImageUrl(url)
  if (isProxiedImageUrl(url)) return url
  if (canProxyImageUrl(url)) return proxyImageUrl(url)
  return url
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
      record[field] = resolveImageUrl(record[field] as string)
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
