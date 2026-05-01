/**
 * PluginRunner - 在前端 WebView 中执行音乐源插件的 JS 代码
 *
 * 利用 WebView 天然的 JS 执行能力，通过 new Function() 在沙箱中执行插件代码，
 * 调用插件导出的 musicUrl() 方法来解析歌曲的真实播放 URL。
 *
 * 支持两种插件格式:
 * - cr (CeruMusic 原生格式): 代码中包含 cerumusic 全局变量引用
 * - lx (洛雪事件驱动格式): 通过 lx.on('request') 注册 handler
 *
 * 参考: CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts
 */

interface PluginExports {
  pluginInfo?: { name: string; version: string; author: string; description: string }
  sources?: Record<string, { name: string; type: string; qualitys: string[] }>
  musicUrl?: (source: string, musicInfo: any, quality: string) => Promise<string | { error?: string }>
}

interface LoadedPlugin {
  exports: PluginExports
  code: string
}

const pluginCache = new Map<string, LoadedPlugin>()

async function getPluginCode(pluginId: string): Promise<string> {
  const res = await (window as any).api.plugins.getCode(pluginId)
  if (res?.success && typeof res.data === 'string') return res.data
  throw new Error(res?.error || '无法获取插件代码')
}

// ==================== 浏览器兼容的加密工具 ====================

function md5(str: string): string {
  function safeAdd(x: number, y: number): number {
    const lsw = (x & 0xffff) + (y & 0xffff)
    const msw = (x >> 16) + (y >> 16) + (lsw >> 16)
    return (msw << 16) | (lsw & 0xffff)
  }
  function bitRotateLeft(num: number, cnt: number): number {
    return (num << cnt) | (num >>> (32 - cnt))
  }
  function md5cmn(q: number, a: number, b: number, x: number, s: number, t: number): number {
    return safeAdd(bitRotateLeft(safeAdd(safeAdd(a, q), safeAdd(x, t)), s), b)
  }
  function md5ff(a: number, b: number, c: number, d: number, x: number, s: number, t: number): number {
    return md5cmn((b & c) | (~b & d), a, b, x, s, t)
  }
  function md5gg(a: number, b: number, c: number, d: number, x: number, s: number, t: number): number {
    return md5cmn((b & d) | (c & ~d), a, b, x, s, t)
  }
  function md5hh(a: number, b: number, c: number, d: number, x: number, s: number, t: number): number {
    return md5cmn(b ^ c ^ d, a, b, x, s, t)
  }
  function md5ii(a: number, b: number, c: number, d: number, x: number, s: number, t: number): number {
    return md5cmn(c ^ (b | ~d), a, b, x, s, t)
  }

  const encoder = new TextEncoder()
  const msg = encoder.encode(str)
  const msgLen = msg.length

  const bitLen = msgLen * 8
  const padLen = (((bitLen + 64) >>> 9) << 4) + 15
  const words = new Int32Array(padLen + 1)
  for (let i = 0; i < msgLen; i++) {
    words[i >> 2] |= msg[i] << ((i % 4) << 3)
  }
  words[msgLen >> 2] |= 0x80 << ((msgLen % 4) << 3)
  words[padLen] = bitLen

  let a = 0x67452301
  let b = 0xefcdab89
  let c = 0x98badcfe
  let d = 0x10325476

  for (let i = 0; i <= padLen; i += 16) {
    const oa = a, ob = b, oc = c, od = d

    // Round 1
    a = md5ff(a, b, c, d, words[i], 7, -680876936)
    d = md5ff(d, a, b, c, words[i + 1], 12, -389564586)
    c = md5ff(c, d, a, b, words[i + 2], 17, 606105819)
    b = md5ff(b, c, d, a, words[i + 3], 22, -1044525330)
    a = md5ff(a, b, c, d, words[i + 4], 7, -176418897)
    d = md5ff(d, a, b, c, words[i + 5], 12, 1200080426)
    c = md5ff(c, d, a, b, words[i + 6], 17, -1473231341)
    b = md5ff(b, c, d, a, words[i + 7], 22, -45705983)
    a = md5ff(a, b, c, d, words[i + 8], 7, 1770035416)
    d = md5ff(d, a, b, c, words[i + 9], 12, -1958414417)
    c = md5ff(c, d, a, b, words[i + 10], 17, -42063)
    b = md5ff(b, c, d, a, words[i + 11], 22, -1990404162)
    a = md5ff(a, b, c, d, words[i + 12], 7, 1804603682)
    d = md5ff(d, a, b, c, words[i + 13], 12, -40341101)
    c = md5ff(c, d, a, b, words[i + 14], 17, -1502002290)
    b = md5ff(b, c, d, a, words[i + 15], 22, 1236535329)

    // Round 2
    a = md5gg(a, b, c, d, words[i + 1], 5, -165796510)
    d = md5gg(d, a, b, c, words[i + 6], 9, -1069501632)
    c = md5gg(c, d, a, b, words[i + 11], 14, 643717713)
    b = md5gg(b, c, d, a, words[i], 20, -373897302)
    a = md5gg(a, b, c, d, words[i + 5], 5, -701558691)
    d = md5gg(d, a, b, c, words[i + 10], 9, 38016083)
    c = md5gg(c, d, a, b, words[i + 15], 14, -660478335)
    b = md5gg(b, c, d, a, words[i + 4], 20, -405537848)
    a = md5gg(a, b, c, d, words[i + 9], 5, 568446438)
    d = md5gg(d, a, b, c, words[i + 14], 9, -1019803690)
    c = md5gg(c, d, a, b, words[i + 3], 14, -187363961)
    b = md5gg(b, c, d, a, words[i + 8], 20, 1163531501)
    a = md5gg(a, b, c, d, words[i + 13], 5, -1444681467)
    d = md5gg(d, a, b, c, words[i + 2], 9, -51403784)
    c = md5gg(c, d, a, b, words[i + 7], 14, 1735328473)
    b = md5gg(b, c, d, a, words[i + 12], 20, -1926607734)

    // Round 3
    a = md5hh(a, b, c, d, words[i + 5], 4, -378558)
    d = md5hh(d, a, b, c, words[i + 8], 11, -2022574463)
    c = md5hh(c, d, a, b, words[i + 11], 16, 1839030562)
    b = md5hh(b, c, d, a, words[i + 14], 23, -35309556)
    a = md5hh(a, b, c, d, words[i + 1], 4, -1530992060)
    d = md5hh(d, a, b, c, words[i + 4], 11, 1272893353)
    c = md5hh(c, d, a, b, words[i + 7], 16, -155497632)
    b = md5hh(b, c, d, a, words[i + 10], 23, -1094730640)
    a = md5hh(a, b, c, d, words[i + 13], 4, 681279174)
    d = md5hh(d, a, b, c, words[i], 11, -358537222)
    c = md5hh(c, d, a, b, words[i + 3], 16, -722521979)
    b = md5hh(b, c, d, a, words[i + 6], 23, 76029189)
    a = md5hh(a, b, c, d, words[i + 9], 4, -640364487)
    d = md5hh(d, a, b, c, words[i + 12], 11, -421815835)
    c = md5hh(c, d, a, b, words[i + 15], 16, 530742520)
    b = md5hh(b, c, d, a, words[i + 2], 23, -995338651)

    // Round 4
    a = md5ii(a, b, c, d, words[i], 6, -198630844)
    d = md5ii(d, a, b, c, words[i + 7], 10, 1126891415)
    c = md5ii(c, d, a, b, words[i + 14], 15, -1416354905)
    b = md5ii(b, c, d, a, words[i + 5], 21, -57434055)
    a = md5ii(a, b, c, d, words[i + 12], 6, 1700485571)
    d = md5ii(d, a, b, c, words[i + 3], 10, -1894986606)
    c = md5ii(c, d, a, b, words[i + 10], 15, -1051523)
    b = md5ii(b, c, d, a, words[i + 1], 21, -2054922799)
    a = md5ii(a, b, c, d, words[i + 8], 6, 1873313359)
    d = md5ii(d, a, b, c, words[i + 15], 10, -30611744)
    c = md5ii(c, d, a, b, words[i + 6], 15, -1560198380)
    b = md5ii(b, c, d, a, words[i + 13], 21, 1309151649)
    a = md5ii(a, b, c, d, words[i + 4], 6, -145523070)
    d = md5ii(d, a, b, c, words[i + 11], 10, -1120210379)
    c = md5ii(c, d, a, b, words[i + 2], 15, 718787259)
    b = md5ii(b, c, d, a, words[i + 9], 21, -343485551)

    a = safeAdd(a, oa)
    b = safeAdd(b, ob)
    c = safeAdd(c, oc)
    d = safeAdd(d, od)
  }

  function hex(x: number): string {
    const s = (x >>> 0).toString(16)
    return '00000000'.slice(s.length) + s
  }
  return hex(a) + hex(b) + hex(c) + hex(d)
}

// ==================== AES 加密 (CBC/ECB, 128-bit) ====================

const AES_SBOX = [
  0x63,0x7c,0x77,0x7b,0xf2,0x6b,0x6f,0xc5,0x30,0x01,0x67,0x2b,0xfe,0xd7,0xab,0x76,
  0xca,0x82,0xc9,0x7d,0xfa,0x59,0x47,0xf0,0xad,0xd4,0xa2,0xaf,0x9c,0xa4,0x72,0xc0,
  0xb7,0xfd,0x93,0x26,0x36,0x3f,0xf7,0xcc,0x34,0xa5,0xe5,0xf1,0x71,0xd8,0x31,0x15,
  0x04,0xc7,0x23,0xc3,0x18,0x96,0x05,0x9a,0x07,0x12,0x80,0xe2,0xeb,0x27,0xb2,0x75,
  0x09,0x83,0x2c,0x1a,0x1b,0x6e,0x5a,0xa0,0x52,0x3b,0xd6,0xb3,0x29,0xe3,0x2f,0x84,
  0x53,0xd1,0x00,0xed,0x20,0xfc,0xb1,0x5b,0x6a,0xcb,0xbe,0x39,0x4a,0x4c,0x58,0xcf,
  0xd0,0xef,0xaa,0xfb,0x43,0x4d,0x33,0x85,0x45,0xf9,0x02,0x7f,0x50,0x3c,0x9f,0xa8,
  0x51,0xa3,0x40,0x8f,0x92,0x9d,0x38,0xf5,0xbc,0xb6,0xda,0x21,0x10,0xff,0xf3,0xd2,
  0xcd,0x0c,0x13,0xec,0x5f,0x97,0x44,0x17,0xc4,0xa7,0x7e,0x3d,0x64,0x5d,0x19,0x73,
  0x60,0x81,0x4f,0xdc,0x22,0x2a,0x90,0x88,0x46,0xee,0xb8,0x14,0xde,0x5e,0x0b,0xdb,
  0xe0,0x32,0x3a,0x0a,0x49,0x06,0x24,0x5c,0xc2,0xd3,0xac,0x62,0x91,0x95,0xe4,0x79,
  0xe7,0xc8,0x37,0x6d,0x8d,0xd5,0x4e,0xa9,0x6c,0x56,0xf4,0xea,0x65,0x7a,0xae,0x08,
  0xba,0x78,0x25,0x2e,0x1c,0xa6,0xb4,0xc6,0xe8,0xdd,0x74,0x1f,0x4b,0xbd,0x8b,0x8a,
  0x70,0x3e,0xb5,0x66,0x48,0x03,0xf6,0x0e,0x61,0x35,0x57,0xb9,0x86,0xc1,0x1d,0x9e,
  0xe1,0xf8,0x98,0x11,0x69,0xd9,0x8e,0x94,0x9b,0x1e,0x87,0xe9,0xce,0x55,0x28,0xdf,
  0x8c,0xa1,0x89,0x0d,0xbf,0xe6,0x42,0x68,0x41,0x99,0x2d,0x0f,0xb0,0x54,0xbb,0x16,
]

const AES_RCON = [0x01,0x02,0x04,0x08,0x10,0x20,0x40,0x80,0x1b,0x36]

function xorBlocks(a: Uint8Array, b: Uint8Array): Uint8Array {
  const r = new Uint8Array(16)
  for (let i = 0; i < 16; i++) r[i] = a[i] ^ b[i]
  return r
}

function aes128EncryptBlock(block: Uint8Array, expandedKey: Uint8Array[]): Uint8Array {
  const state = new Uint8Array(16)
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) state[r * 4 + c] = block[c * 4 + r]
  }

  // AddRoundKey(0)
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) state[r * 4 + c] ^= expandedKey[0][c * 4 + r]
  }

  for (let round = 1; round <= 10; round++) {
    // SubBytes
    for (let i = 0; i < 16; i++) state[i] = AES_SBOX[state[i]]
    // ShiftRows
    let t: number
    t = state[1]; state[1] = state[5]; state[5] = state[9]; state[9] = state[13]; state[13] = t
    t = state[2]; state[2] = state[10]; state[10] = t; t = state[6]; state[6] = state[14]; state[14] = t
    t = state[15]; state[15] = state[11]; state[11] = state[7]; state[7] = state[3]; state[3] = t
    // MixColumns (skip in round 10)
    if (round < 10) {
      for (let c = 0; c < 4; c++) {
        const i = c * 4
        const a0 = state[i], a1 = state[i + 1], a2 = state[i + 2], a3 = state[i + 3]
        const xtime = (b: number) => (b << 1) ^ (((b >> 7) & 1) * 0x1b)
        state[i]     = xtime(a0) ^ (xtime(a1) ^ a1) ^ a2 ^ a3
        state[i + 1] = a0 ^ xtime(a1) ^ (xtime(a2) ^ a2) ^ a3
        state[i + 2] = a0 ^ a1 ^ xtime(a2) ^ (xtime(a3) ^ a3)
        state[i + 3] = (xtime(a0) ^ a0) ^ a1 ^ a2 ^ xtime(a3)
      }
    }
    // AddRoundKey
    for (let c = 0; c < 4; c++) {
      for (let r = 0; r < 4; r++) state[r * 4 + c] ^= expandedKey[round][c * 4 + r]
    }
  }

  const out = new Uint8Array(16)
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) out[c * 4 + r] = state[r * 4 + c]
  }
  return out
}

function aesExpandKey(key: Uint8Array): Uint8Array[] {
  const w: Uint8Array[] = []
  for (let i = 0; i < 4; i++) {
    w[i] = new Uint8Array([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]])
  }
  for (let i = 4; i < 44; i++) {
    let temp = new Uint8Array(w[i - 1])
    if (i % 4 === 0) {
      temp = new Uint8Array([AES_SBOX[temp[1]], AES_SBOX[temp[2]], AES_SBOX[temp[3]], AES_SBOX[temp[0]]])
      temp[0] ^= AES_RCON[(i / 4) - 1]
    }
    const nw = new Uint8Array(4)
    for (let j = 0; j < 4; j++) nw[j] = w[i - 4][j] ^ temp[j]
    w[i] = nw
  }
  const rounds: Uint8Array[] = []
  for (let r = 0; r <= 10; r++) {
    const rk = new Uint8Array(16)
    for (let c = 0; c < 4; c++) {
      for (let row = 0; row < 4; row++) rk[c * 4 + row] = w[r * 4 + c][row]
    }
    rounds.push(rk)
  }
  return rounds
}

function pkcs7Pad(data: Uint8Array, blockSize: number): Uint8Array {
  const padLen = blockSize - (data.length % blockSize)
  const padded = new Uint8Array(data.length + padLen)
  padded.set(data)
  for (let i = data.length; i < padded.length; i++) padded[i] = padLen
  return padded
}

function aesEncrypt(
  data: string | Uint8Array,
  mode: string,
  key: string | Uint8Array,
  iv?: string | Uint8Array
): Uint8Array {
  const toBytes = (v: string | Uint8Array): Uint8Array =>
    typeof v === 'string' ? new TextEncoder().encode(v) : v
  const rawData = toBytes(data)
  const rawKey = toBytes(key)
  const keyBytes = rawKey.length >= 16 ? rawKey.slice(0, 16) : (() => {
    const k = new Uint8Array(16); k.set(rawKey); return k
  })()
  const expandedKey = aesExpandKey(keyBytes)

  if (mode === 'aes-128-ecb') {
    const padded = pkcs7Pad(rawData, 16)
    const out = new Uint8Array(padded.length)
    for (let i = 0; i < padded.length; i += 16) {
      out.set(aes128EncryptBlock(padded.slice(i, i + 16), expandedKey), i)
    }
    return out
  }

  // AES-128-CBC
  const rawIv = iv ? toBytes(iv) : new Uint8Array(16)
  const ivBytes = rawIv.length >= 16 ? rawIv.slice(0, 16) : (() => {
    const b = new Uint8Array(16); b.set(rawIv); return b
  })()
  const padded = pkcs7Pad(rawData, 16)
  const out = new Uint8Array(padded.length)
  let prev = ivBytes
  for (let i = 0; i < padded.length; i += 16) {
    const block = xorBlocks(padded.slice(i, i + 16), prev)
    const encrypted = aes128EncryptBlock(block, expandedKey)
    out.set(encrypted, i)
    prev = encrypted
  }
  return out
}

function randomBytes(size: number): Uint8Array {
  const buf = new Uint8Array(size)
  crypto.getRandomValues(buf)
  return buf
}

// ==================== RSA 加密 ====================

async function rsaEncrypt(data: string, pemKey: string): Promise<string> {
  const pemBody = pemKey
    .replace(/-----BEGIN PUBLIC KEY-----/, '')
    .replace(/-----END PUBLIC KEY-----/, '')
    .replace(/\s+/g, '')
  const binaryDer = Uint8Array.from(atob(pemBody), c => c.charCodeAt(0))

  const publicKey = await crypto.subtle.importKey(
    'spki', binaryDer.buffer, { name: 'RSA-OAEP', hash: 'SHA-256' }, false, ['encrypt']
  )
  const encrypted = await crypto.subtle.encrypt({ name: 'RSA-OAEP' }, publicKey, new TextEncoder().encode(data))
  return btoa(String.fromCharCode(...new Uint8Array(encrypted)))
}

// ==================== cerumusic API 工厂 ====================

function createCerumusicApi() {
  return {
    env: 'browser',
    version: '1.0.3',
    utils: {
      buffer: {
        from: (data: string | ArrayBuffer | Uint8Array, encoding?: string) => {
          if (typeof data === 'string') {
            const enc = (encoding || 'utf8').toLowerCase()
            if (enc === 'base64') return Uint8Array.from(atob(data), c => c.charCodeAt(0))
            if (enc === 'hex') {
              const bytes = new Uint8Array(data.length / 2)
              for (let i = 0; i < data.length; i += 2) bytes[i / 2] = parseInt(data.substr(i, 2), 16)
              return bytes
            }
            return new TextEncoder().encode(data)
          }
          if (data instanceof ArrayBuffer) return new Uint8Array(data)
          if (data instanceof Uint8Array) return data
          return new Uint8Array(data as any)
        },
        bufToString: (buffer: Uint8Array, encoding?: string) => {
          const enc = (encoding || 'utf8').toLowerCase()
          if (enc === 'base64') {
            let binary = ''; for (let i = 0; i < buffer.length; i++) binary += String.fromCharCode(buffer[i])
            return btoa(binary)
          }
          if (enc === 'hex') return Array.from(buffer).map(b => b.toString(16).padStart(2, '0')).join('')
          return new TextDecoder().decode(buffer)
        }
      },
      crypto: {
        md5,
        aesEncrypt,
        randomBytes,
        rsaEncrypt
      }
    },
    request: (
      url: string,
      options?: any,
      callback?: (err: any, result?: any) => void
    ): any => {
      if (typeof options === 'function') { callback = options; options = {} }
      const opts = options || {}
      const method = opts.method || (opts.headers?.['Content-Type'] ? 'POST' : 'GET')
      const body = opts.body
        ? (typeof opts.body === 'string' ? opts.body : JSON.stringify(opts.body))
        : undefined

      const httpProxy = (window as any).api?.httpProxy
      if (!httpProxy) {
        throw new Error('HTTP 代理不可用 (httpProxy 未注册)')
      }

      const doFetch = () => httpProxy(url, {
        method,
        headers: opts.headers || {},
        body,
        timeout: opts.timeout || 15000,
      }).then((res: any) => {
        return {
          statusCode: res?.statusCode ?? 0,
          headers: res?.headers || {},
          body: res?.body ?? ''
        }
      }).catch((e: any) => {
        throw new Error(`网络请求失败: ${e?.message || e} — ${url}`)
      })

      if (callback) {
        doFetch().then((r: any) => callback!(null, r)).catch((e: any) => callback!(e, { statusCode: 0, headers: {}, body: '' }))
        return
      }
      return doFetch()
    },
    // 原始 CeruMusic API 中 NoticeCenter 是一个函数（非 class）
    // 插件通过 cerumusic.NoticeCenter('error', {...}) 调用
    NoticeCenter(type: string, data: any) {
      console.log(`[CeruMusic] NoticeCenter [${type}]:`, data?.title || data)
    }
  }
}

// ==================== 插件格式检测 ====================

function isCrPlugin(code: string): boolean {
  return /\bcerumusic\b/.test(code)
}

// ==================== 插件执行引擎 ====================

function executePluginCode(code: string): PluginExports {
  const isCr = isCrPlugin(code)
  const cerumusicApi = isCr ? createCerumusicApi() : null

  // Cr 插件：注入代理 fetch 拦截插件内的 fetch() 调用，绕过 CORS
  const fetchProxy = isCr ? `
    var fetch = function(url, opts) {
      var httpProxy = (typeof window !== 'undefined' && window.api && window.api.httpProxy);
      if (!httpProxy) return Promise.reject(new Error('HTTP 代理不可用'));
      var method = (opts && opts.method) || 'GET';
      var headers = (opts && opts.headers) || {};
      var body = opts && opts.body;
      if (body && typeof body !== 'string') body = JSON.stringify(body);
      return httpProxy(url, { method: method, headers: headers, body: body, timeout: 15000 })
        .then(function(res) {
          return {
            ok: res && res.statusCode >= 200 && res.statusCode < 300,
            status: res ? res.statusCode : 0,
            statusText: '',
            headers: new Map(Object.entries((res && res.headers) || {})),
            json: function() { return Promise.resolve(res && res.body); },
            text: function() { var b = res && res.body; return Promise.resolve(typeof b === 'string' ? b : JSON.stringify(b)); }
          };
        });
    };
  ` : ''

  // 注意: cerumusic 必须作为 Function 参数传入，
  // 这样它在插件代码中是一个局部变量（与 vm.runInNewContext 行为一致）
  const sandbox = new Function(
    'globalThis', 'lx', 'console', 'setTimeout', 'clearTimeout',
    'setInterval', 'clearInterval', 'Buffer', 'JSON', 'require',
    '_m', '_e', 'process', 'cerumusic',
    `
    ${fetchProxy}
    var module = { exports: {} };
    var exports = module.exports;
    try {
      ${code}
    } catch(e) {
      console.warn('[PluginRunner] 插件执行异常:', e.message);
    }
    return module.exports;
    `
  )

  const mockExports: any = { exports: {} }
  const mockModule: any = { exports: mockExports }

  const lxState: { requestHandler: Function | null } = { requestHandler: null }
  const mockLx = {
    on: (event: string, handler: Function) => {
      if (event === 'request') lxState.requestHandler = handler
    },
    send: () => {},
    version: '1.0.0',
    env: 'browser'
  }

  // Browser-compatible Buffer polyfill for global scope
  const BufferPolyfill: any = {
    from(data: any, encoding?: string) {
      if (typeof data === 'string') {
        const enc = (encoding || 'utf8').toLowerCase()
        if (enc === 'base64') return Uint8Array.from(atob(data), c => c.charCodeAt(0))
        if (enc === 'hex') {
          const bytes = new Uint8Array(data.length / 2)
          for (let i = 0; i < data.length; i += 2) bytes[i / 2] = parseInt(data.substr(i, 2), 16)
          return bytes
        }
        return new TextEncoder().encode(data)
      }
      if (data instanceof ArrayBuffer) return new Uint8Array(data)
      if (data instanceof Uint8Array) return data
      return new Uint8Array(data)
    },
    isBuffer(obj: any) { return obj instanceof Uint8Array },
    concat(list: Uint8Array[], totalLength?: number) {
      const len = totalLength ?? list.reduce((s, b) => s + b.length, 0)
      const result = new Uint8Array(len)
      let offset = 0
      for (const buf of list) { result.set(buf, offset); offset += buf.length }
      return result
    }
  }

  // cerumusicApi 作为第14个参数传入，对应 Function 参数列表中的 'cerumusic'
  // 这使得 cerumusic 在插件代码中作为局部变量可访问
  // 同时也赋值给 globalThis，确保间接引用也能找到
  const sandboxGlobal = {} as any
  if (isCr && cerumusicApi) {
    sandboxGlobal.cerumusic = cerumusicApi
    sandboxGlobal.fetch = fetch
  }

  try {
    const pluginExports = sandbox(
      sandboxGlobal, mockLx, console, setTimeout, clearTimeout,
      setInterval, clearInterval, BufferPolyfill, JSON,
      () => ({}), {}, {}, { env: { NODE_ENV: 'production' } },
      cerumusicApi
    )
    if (pluginExports && typeof pluginExports === 'object') {
      Object.assign(mockModule.exports, pluginExports)
    }
  } catch (e) {
    console.warn('[PluginRunner] 插件初始化失败:', e)
  }

  const result = mockModule.exports

  // LX 事件驱动兼容: 自动将 lx.on('request') handler 包装为 musicUrl
  if (!result.musicUrl && lxState.requestHandler) {
    const handler = lxState.requestHandler
    result.musicUrl = async (source: string, musicInfo: any, quality: string) => {
      const res = await handler({
        source,
        action: 'musicUrl',
        info: { musicInfo, type: quality }
      })
      return res
    }
  }

  // Cr 插件: 绑定 cerumusic 到 musicUrl 的 this 上下文
  if (isCr && cerumusicApi && typeof result.musicUrl === 'function') {
    const originalMusicUrl = result.musicUrl
    result.musicUrl = async (source: string, musicInfo: any, quality: string) => {
      return originalMusicUrl.call({ cerumusic: cerumusicApi }, source, musicInfo, quality)
    }
  }

  return result
}

async function loadPlugin(pluginId: string): Promise<LoadedPlugin> {
  const cached = pluginCache.get(pluginId)
  if (cached) return cached

  const code = await getPluginCode(pluginId)
  console.log(`[PluginRunner] 加载插件 ${pluginId}, 代码长度: ${code.length}, 是否Cr格式: ${isCrPlugin(code)}`)
  const exports = executePluginCode(code)

  const exportKeys = Object.keys(exports)
  console.log(`[PluginRunner] 插件导出: musicUrl=${typeof exports.musicUrl}, keys=[${exportKeys.join(', ')}]`)

  if (typeof exports.musicUrl !== 'function') {
    // 打印前200字符帮助诊断
    console.warn('[PluginRunner] 插件代码片段:', code.substring(0, 300))
    throw new Error('插件未导出 musicUrl 方法，可能格式不兼容')
  }

  const plugin: LoadedPlugin = { exports, code }
  pluginCache.set(pluginId, plugin)
  return plugin
}

async function getMusicUrl(
  pluginId: string,
  source: string,
  songInfo: any,
  quality: string
): Promise<string> {
  const plugin = await loadPlugin(pluginId)
  try {
    const result = await plugin.exports.musicUrl!(source, songInfo, quality)

    if (typeof result === 'string') {
      console.log(`[PluginRunner] ${source} musicUrl 返回字符串: ${result.substring(0, 120)}`)
      return result
    }
    if (typeof result === 'object' && result !== null) {
      console.log(`[PluginRunner] ${source} musicUrl 返回对象:`, JSON.stringify(result).substring(0, 200))
      if (result.error) throw new Error(result.error)
      // 有些插件返回 { url: '...', songmid: '...' } 格式
      if (result.url && typeof result.url === 'string') return result.url
    }

    throw new Error(`插件返回了无效的播放链接 (type=${typeof result})`)
  } catch (e: any) {
    const detail = e.message || String(e)
    throw new Error(`[${source}] ${detail}`)
  }
}

function clearCache(pluginId?: string) {
  if (pluginId) {
    pluginCache.delete(pluginId)
  } else {
    pluginCache.clear()
  }
}

export default { getMusicUrl, clearCache, loadPlugin }
