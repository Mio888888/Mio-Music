# Playback Cross-Layer Contracts

> Executable contracts for frontend music playback calls that cross the Vue/Tauri/Rust boundary.

---

## Scenario: Subsonic SDK Requests and Player Loading State

### 1. Scope / Trigger

- Trigger: playback/search/lyrics requests cross from Vue/TypeScript into Tauri/Rust via `window.api.music.requestSdk`, `musicSdk.request`, and `player__play`.
- Applies when adding or changing music SDK calls, Subsonic built-in source handling, global playback state, player loading UI, or Rust player events.
- Goal: keep Subsonic configuration attached at the IPC boundary and keep frontend loading state aligned with actual backend playback start, not with command dispatch.

### 2. Signatures

Frontend SDK bridge:

```ts
window.api.music.requestSdk(method: string, args: Record<string, any>): Promise<any>
```

Frontend service wrapper:

```ts
musicSdk.request(method: string, args?: Record<string, any>): Promise<any>
```

Rust SDK command:

```rust
#[tauri::command]
pub async fn service_music_sdk_request(
    method: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String>
```

Player command:

```rust
#[tauri::command]
pub fn player__play(
    player: State<'_, SharedPlayer>,
    url: String,
    slot: Option<String>,
    cache_key: Option<String>,
) -> Result<serde_json::Value, String>
```

Player events consumed by `src/store/ControlAudio.ts`:

```ts
listen('player:state', (event: { payload: {
  state: 'Stopped' | 'Playing' | 'Paused'
  position: number
  duration: number
  volume: number
  url: string
  isPlaying: boolean
}}) => void)

listen('player:error', (event: { payload?: { error?: string } }) => void)
```

### 3. Contracts

#### Subsonic request payload

When `args.source === 'subsonic'`, the payload sent to `service_music_sdk_request` must include:

```ts
{
  source: 'subsonic',
  subsonicConfig: {
    baseUrl: string,
    username: string,
    password: string,
    apiVersion: string,   // default '1.16.1'
    clientName: string,   // default 'Mio'
  },
  ...methodSpecificArgs
}
```

Boundary responsibility:

- `src/services/musicSdk.ts` injects `subsonicConfig` for calls made through `musicSdk.request`.
- `src/bridge/index.ts` injects `subsonicConfig` for legacy/direct `window.api.music.requestSdk` calls.
- Call sites should pass `source: songInfo.source` or current source correctly; they should not duplicate Subsonic config mapping.

#### Player loading state

- `isLoadingSong.value = true` is set when a new playback request starts.
- `isLoadingSong.value = false` must be set when Rust emits `player:state` with `state === 'Playing'`, or when Rust emits `player:error`.
- Do not clear playback loading immediately after `invoke('player__play')` resolves; that only means the command was accepted, not that audio has started.

#### Search source selector

- Built-in Subsonic is a playback/discovery source, not a plugin search source selector option.
- Search source dropdowns must filter out `subsonic` from `userInfo.supportedSources`.
- If no plugin-backed searchable sources remain, the dropdown must not show a fake/default icon or open an empty menu.

### 4. Validation & Error Matrix

| Condition | Required behavior |
|---|---|
| `source === 'subsonic'` and valid config exists in `LocalUserDetailStore.userInfo.subsonicConfig` | Inject `subsonicConfig` before IPC. |
| `source === 'subsonic'` and config is missing/empty | Rust Subsonic source may return `请先配置 Subsonic 服务器地址、用户名和密码`; frontend should surface/fail gracefully. |
| `invoke('player__play')` resolves but no `player:state Playing` yet | Keep switching/loading animation active. |
| Rust emits `player:state` with `state === 'Playing'` | Clear `isLoadingSong`; publish play state. |
| Rust emits `player:error` | Clear `isLoadingSong`; publish error state. |
| Current selected source is stale/uninstalled and Subsonic is available | Repair selected source to `subsonic`. |
| Only Subsonic is available and no music-source plugin is installed | Discovery page can render Subsonic content; search dropdown remains empty/hidden. |

### 5. Good/Base/Bad Cases

- Good: `getLyric` called through `window.api.music.requestSdk('getLyric', { source: 'subsonic', songInfo })` receives injected `subsonicConfig` at the bridge.
- Good: `playSong()` starts loading, `player__play` returns quickly, loading spinner remains until `player:state Playing` arrives.
- Base: plugin source playback/search still passes `source` without `subsonicConfig` and uses plugin/native source handling.
- Base: local file playback is not affected by Subsonic config injection.
- Bad: direct `requestSdk` Subsonic calls omit config and Rust returns the missing-server-config error.
- Bad: clearing `isLoadingSong` in `finally` or immediately after `player__play` causes the switching animation to finish before audio starts.
- Bad: falling back to hard-coded `kw`, `wy`, or icon defaults when no searchable plugin source exists.

### 6. Tests Required

Minimum verification for changes in this area:

- TypeScript: run `npx vue-tsc --noEmit` or `npm run build`.
- Rust: run `cargo build --manifest-path src-tauri/Cargo.toml` or `cargo check --manifest-path src-tauri/Cargo.toml`.
- Manual playback assertions:
  - Configure valid Subsonic credentials, select/play a Subsonic song, and confirm no missing-config IPC warning appears.
  - Switch tracks and confirm the loading animation ends only after playback begins.
  - With only Subsonic configured and no music-source plugin installed, confirm the discovery page uses Subsonic while the search source dropdown shows no plugin source.
  - Play at least one non-Subsonic source if available to confirm config injection does not regress plugin/native sources.

### 7. Wrong vs Correct

#### Wrong

```ts
// Command accepted, but audio may still be buffering/initializing in Rust.
await invoke('player__play', { url, cacheKey })
isLoadingSong.value = false
```

```ts
// Direct requestSdk call loses Subsonic credentials.
await window.api.music.requestSdk('getLyric', {
  source: 'subsonic',
  songInfo,
})
```

#### Correct

```ts
await invoke('player__play', { url, cacheKey })
// ControlAudio clears loading only after backend confirms playback.
listen('player:state', (event) => {
  if (event.payload.state === 'Playing') {
    isLoadingSong.value = false
  }
})
```

```ts
// Bridge/service owns Subsonic config injection.
await window.api.music.requestSdk('getLyric', {
  source: 'subsonic',
  songInfo,
})
// src/bridge/index.ts injects subsonicConfig before service_music_sdk_request.
```
