/**
 * 全局 fetch 拦截器：将跨域图片请求代理到 Tauri Rust 后端，
 * 绕过 WebView CORS 限制（PixiJS 等库内部使用 fetch 加载图片）。
 */

const IMAGE_RE = /\.(jpg|jpeg|png|gif|webp|bmp|avif)(\?.*)?$/i
const MIME: Record<string, string> = {
  '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.png': 'image/png',
  '.gif': 'image/gif', '.webp': 'image/webp', '.bmp': 'image/bmp', '.avif': 'image/avif'
}

const _fetch = window.fetch.bind(window)

function b64toBlob(b64: string, mime: string): Blob {
  const bin = atob(b64)
  const arr = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i)
  return new Blob([arr], { type: mime })
}

window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const url = typeof input === 'string' ? input
    : input instanceof Request ? input.url
    : input instanceof URL ? input.href
    : String(input)

  if (IMAGE_RE.test(url) && !url.startsWith('data:') && !url.startsWith('blob:')) {
    try {
      const proxy = (window as any).api?.httpProxy
      if (proxy) {
        const res = await proxy(url, { raw: true, timeout: 15000 })
        if (res?.isBase64 && res?.body) {
          const ext = (url.match(/\.(\w+)/)?.[1] || 'jpeg').toLowerCase()
          const mime = MIME[`.${ext}`] || 'image/jpeg'
          const blob = b64toBlob(res.body, mime)
          return new Response(blob, { status: 200, headers: { 'Content-Type': mime } })
        }
      }
    } catch (e) {
      console.warn('[CORS Proxy] 图片代理失败:', url.substring(0, 80), e)
    }
  }
  return _fetch(input, init)
}
