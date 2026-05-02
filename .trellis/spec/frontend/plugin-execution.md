# Plugin Execution (Browser)

> How Cr/Lx music plugins are executed in the browser WebView via `PluginRunner.ts`.

---

## Overview

Plugins are JavaScript modules that export `musicUrl(source, musicInfo, quality)`. The Rust backend stores plugin files but does NOT execute them. Instead, the frontend reads the source code via `plugin__get_code` and executes it in a `new Function()` sandbox in the WebView.

This replaces the Node.js `vm.runInNewContext` approach used in CeruMusic-main, adapted for browser constraints.

---

## Architecture

```
Frontend (Vue)
    ↓ PluginRunner.getMusicUrl(pluginId, source, songInfo, quality)
    ↓ loadPlugin(pluginId) — cache check
    ↓ getPluginCode(pluginId) — ipcInvoke('plugin__get_code', { pluginId })
    ↓ executePluginCode(code) — new Function() sandbox
    ↓ plugin.exports.musicUrl(source, songInfo, quality)
    ↓ httpProxy(rawUrl) or audioProxy(rawUrl) — bypass CORS
    ↓ returns data: URI → <audio>.src
```

---

## Sandbox Execution Model

### `new Function()` Parameters

The sandbox is a `new Function()` with these named parameters (must match the argument list order exactly):

```
globalThis, lx, console, setTimeout, clearTimeout,
setInterval, clearInterval, Buffer, JSON, require,
_m, _e, process, cerumusic
```

The Function body also declares these local variables:

```javascript
var module = { exports: {} };   // Plugin module system
var exports = module.exports;   // Shorthand
var global = globalThis;        // Node.js compat (vm.runInNewContext provides this)
```

**Parameter 14 (`cerumusic`)**: The Cr API object — must be injected for ALL plugin types, not just native CR. See "Gotcha: cerumusic Must Be Injected for ALL Plugin Types" below.

### Cr Plugin Detection

```typescript
function isCrPlugin(code: string): boolean {
  return /\bcerumusic\b/.test(code)
}
```

If `cerumusic` appears anywhere in the code, it's treated as a Cr plugin and receives:
- The cerumusic API object as the `cerumusic` parameter
- A proxied `fetch` function (via `var fetch = function(...)` in the Function body)
- Both are injected into the sandbox

---

## CeruMusic API Object

The `cerumusic` parameter passed to Cr plugins provides:

```typescript
{
  env: 'browser',
  version: '1.0.3',
  utils: {
    buffer: { from(data, encoding?), bufToString(buffer, encoding?) },
    crypto: { md5(str), aesEncrypt(data, mode, key, iv?), randomBytes(size), rsaEncrypt(data, pemKey) }
  },
  request(url, options?, callback?)  // Proxied via httpProxy
  NoticeCenter(type, data)           // Plain function, NOT a class
}
```

### Request Function

Calls `window.api.httpProxy(url, opts)` under the hood. Supports both Promise and callback styles:

```typescript
// Promise style
const result = await cerumusic.request(url, { method: 'GET', headers: {...} })
// result = { statusCode, headers, body }

// Callback style
cerumusic.request(url, (err, result) => { ... })
```

The `body` in the response is already parsed (JSON object or string) — not raw text.

---

## Gotcha: cerumusic Scope (CRITICAL)

**Symptom**: `Can't find variable: cerumusic` — plugin code fails, `musicUrl` is never exported.

**Cause**: Node.js `vm.runInNewContext(code, sandbox)` makes all sandbox properties top-level variables in the executed code. Browser `new Function()` does NOT — parameters are local to the function scope only.

**The fix**: `cerumusic` must be passed as a named parameter in the Function constructor (parameter 14). This makes it a local variable accessible in the plugin code's scope, matching `vm.runInNewContext` behavior.

```typescript
// Correct — cerumusic is a named parameter
const sandbox = new Function(
  'globalThis', 'lx', 'console', 'setTimeout', 'clearTimeout',
  'setInterval', 'clearInterval', 'Buffer', 'JSON', 'require',
  '_m', '_e', 'process', 'cerumusic',    // <-- parameter 14
  `/* plugin code here */`
)

// Call: pass cerumusicApi as 14th argument
sandbox(sandboxGlobal, mockLx, ..., cerumusicApi)
```

> **Warning**: Do NOT rely on `globalThis.cerumusic = ...` alone. The `globalThis` argument is a plain object `{}`, and plugins referencing `cerumusic` as a bare identifier look in the function's local scope, not `globalThis`. Both approaches should be used: parameter + globalThis assignment.

---

## Gotcha: NoticeCenter Must Be a Function

**Symptom**: `TypeError: cerumusic.NoticeCenter is not a function` — plugin crashes on any notification call.

**Cause**: In CeruMusic-main's Node.js API, `NoticeCenter` is a **function** (not a class). Plugins call it as:

```javascript
cerumusic.NoticeCenter('error', { title: '...', content: '...' })
// NOT: new cerumusic.NoticeCenter(...)
```

**The fix**: Implement `NoticeCenter` as a plain function, not a class:

```typescript
// Correct
NoticeCenter(type: string, data: any) {
  console.log(`[CeruMusic] NoticeCenter [${type}]:`, data?.title || data)
}

// Wrong — causes TypeError when called without `new`
NoticeCenter: class { ... }
```

---

## Fetch Proxy (Cr Plugins)

Cr plugins often call `fetch()` directly. In the Tauri WebView, native `fetch()` is subject to CORS.

**Solution**: Inject a proxied `fetch` variable into the Function body that routes through `httpProxy`:

```javascript
// Injected as `var fetch = function(url, opts) { ... }` before plugin code
var fetch = function(url, opts) {
  var httpProxy = (typeof window !== 'undefined' && window.api && window.api.httpProxy);
  if (!httpProxy) return Promise.reject(new Error('HTTP 代理不可用'));
  // ... route through httpProxy
  return httpProxy(url, { method, headers, body, timeout: 15000 })
    .then(function(res) {
      return {
        ok: res && res.statusCode >= 200 && res.statusCode < 300,
        status: res ? res.statusCode : 0,
        json: function() { return Promise.resolve(res && res.body); },
        text: function() { ... }
      };
    });
};
```

The proxy response's `body` is already a parsed JSON object or string — matching what native `fetch().json()` or `fetch().text()` would return.

---

## LX Plugin Compatibility

LX plugins use an event-driven pattern (`lx.on('request', handler)`). They are converted to CR format by `converter.rs` at import time. The converted code wraps the original plugin in a `mockLx` sandbox.

### Execution Flow (LX Plugin)

```
1. User imports LX plugin → Rust converter.rs converts to CR format
2. Converted code stored on disk (contains `cerumusic` reference → isCrPlugin = true)
3. PluginRunner loads converted code
4. cerumusic API injected (always, not just for native CR plugins)
5. Converted code's initializePlugin() runs:
   - const { request, utils } = cerumusic  // gets HTTP + crypto
   - Creates mockLx with request, utils, on, send, etc.
   - new Function(originalPluginCode) with mockLx as `lx` parameter
   - Plugin registers handler via lx.on('request', handler)
6. module.exports = { musicUrl, getPic, getLyric }
```

### After execution, if no musicUrl but lxState.requestHandler exists:

```typescript
result.musicUrl = async (source, musicInfo, quality) => {
  return handler({ source, action: 'musicUrl', info: { musicInfo, type: quality } })
}
```

### Converter's mockLx must include (from original converter-event-driven.ts)

The `mockLx` inside the converter-generated code MUST provide:

```javascript
{
  EVENT_NAMES: { request, inited, updateAlert },
  on(event, handler),        // captures request handler
  send(event, data),         // handles 'inited' for source registration
  request: cerumusic.request,         // HTTP client — CRITICAL for lx.request()
  utils: {
    buffer: cerumusic.utils.buffer,
    crypto: {
      aesEncrypt, md5, randomBytes, rsaEncrypt  // delegate to cerumusic.utils.crypto
    }
  },
  version, apiVersion, currentScriptInfo, env: 'nodejs'
}
```

> **Warning**: If `mockLx` is missing `request` or `utils`, LX plugins that call `lx.request()` or `lx.utils.crypto.*` will fail silently (errors caught by inner try-catch), resulting in `sign=fail` or empty responses.

---

## Gotcha: cerumusic Must Be Injected for ALL Plugin Types

**Symptom**: LX plugin's `initializePlugin()` throws `TypeError: Cannot destructure property 'request' of undefined` silently, `requestHandler` stays null, "插件请求处理器未初始化".

**Cause**: The converter-generated code references `cerumusic` (e.g., `const { request, utils } = cerumusic`). If `isCrPlugin(code)` returns false for LX plugins, `cerumusicApi` is null.

**The fix**: Always create and inject `cerumusicApi`, regardless of `isCrPlugin()`:

```typescript
// Correct — always available
const cerumusicApi = createCerumusicApi()  // NOT: isCr ? createCerumusicApi() : null
sandboxGlobal.cerumusic = cerumusicApi     // NOT: if (isCr) { ... }
```

> Note: The fetch proxy (`var fetch = ...`) is still CR-only since it's only needed for direct `fetch()` calls.

---

## Gotcha: Buffer Polyfill Must Support toString('hex')

**Symptom**: Plugin crypto operations produce wrong results, `sign=fail`, or garbled data.

**Cause**: `BufferPolyfill.from()` returned bare `Uint8Array` instances. LX plugins use `Buffer.from(data).toString('hex')` — but `Uint8Array.toString()` ignores the encoding argument and returns "0,1,2,...".

**The fix**: Patch returned `Uint8Array` instances with a custom `toString`:

```typescript
function patchBufferToString(bytes: Uint8Array): Uint8Array {
  bytes.toString = function(encoding?: string) {
    if (encoding === 'hex') return Array.from(this).map(b => b.toString(16).padStart(2, '0')).join('')
    if (encoding === 'base64') { /* btoa */ }
    if (!encoding || encoding === 'utf8') return new TextDecoder().decode(this)
    return origToString()
  }
  return bytes
}
```

Both `BufferPolyfill.from()` and `cerumusic.utils.buffer.from()` must apply this patch.

---

## Gotcha: RSA Must Be Synchronous PKCS1 v1.5

**Symptom**: Plugin sign generation fails because `rsaEncrypt` returns a Promise instead of a string, or produces wrong ciphertext.

**Cause**: Original Electron uses `crypto.publicEncrypt()` with `RSA_PKCS1_PADDING` — synchronous, returns Buffer. Web Crypto API uses `RSA-OAEP` — async (returns Promise), different padding produces different ciphertext.

**The fix**: Implement RSA PKCS1 v1.5 synchronously using BigInt:

```typescript
// Correct: synchronous, PKCS1 v1.5 padding, returns base64 string
function rsaEncrypt(data: string, pemKey: string): string {
  // Parse ASN.1 DER → extract n, e from public key
  // Apply PKCS1 v1.5 padding: 0x00 0x02 [random non-zero] 0x00 [data]
  // Compute ciphertext = padded^e mod n using BigInt
  // Return base64
}

// Wrong: async + wrong padding
async function rsaEncrypt(data: string, pemKey: string): Promise<string> {
  await crypto.subtle.encrypt({ name: 'RSA-OAEP' }, ...)  // OAEP != PKCS1
}
```

---

## Gotcha: `global` Variable Must Be Provided

**Symptom**: Plugin code throws `ReferenceError: global is not defined` during sign computation.

**Cause**: Original `vm.runInNewContext(code, { global: {}, ... })` provides `global`. Browser `new Function()` does not. Some obfuscated plugins use `global.xxx`.

**The fix**: Add `var global = globalThis;` in the Function body and pass `global` as a parameter in the converter's inner sandbox:

```typescript
// PluginRunner outer sandbox body
var global = globalThis;

// converter.rs inner sandbox
new Function('globalThis', 'lx', ..., 'process', 'global', originalPluginCode)
// Pass: { lx: mockLx }, ..., { env: {...} }, { lx: mockLx }
```

---

## Common Mistake: Plugin Server Issues vs Code Issues

**Symptom**: All requests from a specific LX plugin return `sign=fail` → 404.

**Debugging**: Before investigating code, verify the plugin works in the ORIGINAL environment:

```bash
# Run the plugin in Node.js with native crypto/Buffer
# If sign=fail also occurs → it's a server-side issue, not code
node -e "
  const crypto = require('crypto');
  // ... mock lx environment with native Node.js APIs
  // ... run originalPluginCode
"
```

**Key insight**: The `[独家音源]` (洛雪科技 v4) plugin's server at `88.lxmusic.xn--fiqs8s` returns `{code: 0, data: null}` for `checkUpdate`. The `data: null` means no sign token is provided, causing `sign=fail`. This happens identically in Node.js — it's a server issue.

---

## Plugin Caching

```typescript
const pluginCache = new Map<string, LoadedPlugin>()
```

**Cache invalidation**: Must be called explicitly when a plugin is added, updated, or uninstalled:

```typescript
// In store/plugin.ts
PluginRunner.clearCache(pluginId)
```

**Why**: The plugin code doesn't change after loading, so re-executing `new Function()` on every play is wasteful. Cache stores the evaluated exports.

---

## Audio URL Resolution Flow

After the plugin returns a URL, `globaPlayList.ts::getSongRealUrl()` processes it:

```
Plugin returns URL (rawUrl)
    ↓ starts with 'data:' or 'file:'? → return directly
    ↓ audioProxy(rawUrl) — Rust backend fetches, returns data: URI
    ↓ audioProxy returns valid data: URI? → return it
    ↓ audioProxy failed? → return rawUrl as fallback
```

**Gotcha**: If `rawUrl` is an HTTP 302 redirect, `audioProxy` follows it (up to 10 redirects). If the final response is not audio, the data URI will contain non-audio data, causing `MediaError code: 4 (MEDIA_ERR_SRC_NOT_SUPPORTED)` on the `<audio>` element.

---

## CORS Proxy for Images

`src/utils/cors-proxy.ts` intercepts `window.fetch()` globally:

- Matches image URLs: `/\.(jpg|jpeg|png|gif|webp|bmp|avif)(\?.*)?$/i`
- Proxies through `httpProxy({ raw: true })` to get base64 image data
- Returns a `Blob` response that the caller can use normally
- Non-image or proxy failures fall through to native `fetch()`

**Used by**: PixiJS album art loading (`@applemusic-like-lyrics/core`), any other `fetch()`-based image loading in third-party libraries.

---

## Key Files

| File | Purpose |
|------|---------|
| `src/utils/plugin/PluginRunner.ts` | Sandbox execution engine, cerumusic API, caching |
| `src/utils/audio/globaPlayList.ts` | `getSongRealUrl()` — calls PluginRunner, then audioProxy |
| `src/utils/cors-proxy.ts` | Global fetch interceptor for cross-origin images |
| `src/bridge/index.ts` | `api.plugins.getCode(pluginId)` — IPC to `plugin__get_code` |
| `src/store/plugin.ts` | Plugin CRUD, calls `PluginRunner.clearCache()` on add/uninstall |
