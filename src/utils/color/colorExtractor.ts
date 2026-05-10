import { canProxyImageUrl, proxyImageUrl } from '@/utils/imageProxy'

export interface Color { r: number; g: number; b: number }
export interface ImageAnalysisResult { dominantColor: Color; useBlackText: boolean }

const colorCache = new Map<string, ImageAnalysisResult>()
const MAX_CACHE = 50
const FALLBACK: ImageAnalysisResult = { dominantColor: { r: 76, g: 116, b: 206 }, useBlackText: false }

let worker: Worker | null = null
let requestIdSeed = 0

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(new URL('./colorAnalysis.worker.ts', import.meta.url), { type: 'module' })
  }
  return worker
}

export async function analyzeImageColors(imageSrc: string): Promise<ImageAnalysisResult> {
  const cached = colorCache.get(imageSrc)
  if (cached) return cached

  const cacheAndReturn = (result: ImageAnalysisResult): ImageAnalysisResult => {
    if (colorCache.size >= MAX_CACHE) {
      const firstKey = colorCache.keys().next().value
      if (firstKey !== undefined) colorCache.delete(firstKey)
    }
    colorCache.set(imageSrc, result)
    return result
  }

  if (!imageSrc || imageSrc.startsWith('@')) {
    return cacheAndReturn(FALLBACK)
  }

  const pixels = await getImagePixelsWithFallback(imageSrc)
  if (!pixels) return cacheAndReturn(FALLBACK)

  // Worker 计算颜色分析
  return new Promise((resolve) => {
    const w = getWorker()
    const requestId = ++requestIdSeed
    const timeout = setTimeout(() => {
      w.removeEventListener('message', handler as EventListener)
      resolve(cacheAndReturn(FALLBACK))
    }, 5000)
    const handler = (e: MessageEvent<{ requestId: number; dominantColor: Color; useBlackText: boolean }>) => {
      if (e.data?.requestId !== requestId) return
      clearTimeout(timeout)
      w.removeEventListener('message', handler as EventListener)
      resolve(cacheAndReturn({ dominantColor: e.data.dominantColor, useBlackText: e.data.useBlackText }))
    }
    w.addEventListener('message', handler as EventListener)
    w.postMessage({ requestId, pixels })
  })
}

async function getImagePixelsWithFallback(imageSrc: string): Promise<Uint8ClampedArray | null> {
  const directPixels = await getImagePixels(imageSrc)
  if (directPixels) return directPixels

  if (!canProxyImageUrl(imageSrc)) return null

  const fallbackUrl = proxyImageUrl(imageSrc)
  if (fallbackUrl === imageSrc) return null
  return getImagePixels(fallbackUrl)
}

function getImagePixels(imageSrc: string): Promise<Uint8ClampedArray | null> {
  return new Promise((resolve) => {
    const img = new Image()
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas')
        const ctx = canvas.getContext('2d')
        if (!ctx) { resolve(null); return }
        const size = 100
        canvas.width = size
        canvas.height = size
        ctx.drawImage(img, 0, 0, size, size)
        resolve(ctx.getImageData(0, 0, size, size).data)
      } catch {
        resolve(null)
      }
    }
    img.onerror = () => resolve(null)
    img.crossOrigin = 'anonymous'
    img.src = imageSrc
    if (img.complete) img.onload!(new Event('load'))
  })
}
