/**
 * PluginRunner - 主线程代理层
 *
 * 将所有插件执行委托给 Web Worker，避免主线程阻塞。
 * IPC 调用（httpProxy, plugins.getCode, plugins.getConfig）
 * 在主线程执行后通过 postMessage 返回 Worker。
 */

let worker: Worker | null = null
const pendingCalls = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>()
let callSeq = 0

function getWorker(): Worker {
  if (!worker) {
    worker = new Worker(new URL('./pluginWorker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (e) => {
      const msg = e.data

      // Worker 请求 IPC 桥接
      if (msg.type === 'ipc') {
        handleIpc(msg.id, msg.method, msg.args)
        return
      }

      // Worker 返回方法调用结果
      if (msg.type === 'resolve') {
        const pending = pendingCalls.get(msg.id)
        if (pending) {
          pendingCalls.delete(msg.id)
          pending.resolve(msg.result)
        }
        return
      }
      if (msg.type === 'reject') {
        const pending = pendingCalls.get(msg.id)
        if (pending) {
          pendingCalls.delete(msg.id)
          pending.reject(new Error(msg.error))
        }
      }
    }
    worker.onerror = (e) => {
      console.error('[PluginRunner] Worker 错误:', e.message)
    }
  }
  return worker
}

async function handleIpc(ipcId: number, method: string, args: any) {
  try {
    let result: any
    const api = window as any
    switch (method) {
      case 'httpProxy':
        result = await api.api.httpProxy(args.url, args)
        break
      case 'plugins.getCode':
        result = await api.api.plugins.getCode(args.pluginId)
        break
      case 'plugins.getConfig':
        result = await api.api.plugins.getConfig(args.pluginId)
        break
      default:
        throw new Error(`未知 IPC 方法: ${method}`)
    }
    getWorker().postMessage({ type: 'ipc-resolve', id: ipcId, result })
  } catch (e: any) {
    getWorker().postMessage({ type: 'ipc-reject', id: ipcId, error: e?.message || String(e) })
  }
}

function callWorker(method: string, args: any[]): Promise<any> {
  return new Promise((resolve, reject) => {
    const id = ++callSeq
    pendingCalls.set(id, { resolve, reject })
    getWorker().postMessage({ type: 'call', id, method, args })
  })
}

async function getMusicUrl(pluginId: string, source: string, songInfo: any, quality: string): Promise<string> {
  return callWorker('getMusicUrl', [pluginId, source, songInfo, quality])
}

async function getPic(pluginId: string, source: string, songInfo: any): Promise<string> {
  return callWorker('getPic', [pluginId, source, songInfo])
}

async function getLyric(pluginId: string, source: string, songInfo: any): Promise<string | { lyric?: string; tlyric?: string; rlyric?: string; lxlyric?: string }> {
  return callWorker('getLyric', [pluginId, source, songInfo])
}

async function callMethod(pluginId: string, method: string, argsJson: string, options: { injectConfig?: boolean } = {}): Promise<any> {
  return callWorker('callMethod', [pluginId, method, argsJson, options])
}

async function testConnection(pluginId: string): Promise<{ success: boolean; message: string }> {
  return callWorker('testConnection', [pluginId])
}

function clearCache(pluginId?: string) {
  callWorker('clearCache', [pluginId]).catch(() => {})
}

function terminate() {
  if (worker) {
    worker.terminate()
    worker = null
    for (const pending of pendingCalls.values()) {
      pending.reject(new Error('Worker terminated'))
    }
    pendingCalls.clear()
  }
}

export default { getMusicUrl, getPic, getLyric, callMethod, testConnection, clearCache, terminate }
