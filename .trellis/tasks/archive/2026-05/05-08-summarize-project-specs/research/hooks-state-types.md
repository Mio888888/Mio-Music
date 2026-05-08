# Research: Hooks, State, and Type Safety Conventions

- **Query**: Research actual hook/composable, state management, and type-safety conventions for summarizing into `.trellis/spec/frontend/hook-guidelines.md`, `.trellis/spec/frontend/state-management.md`, and `.trellis/spec/frontend/type-safety.md`.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/composables/useEventListener.ts` | Small lifecycle composable that registers DOM/Tauri/IPC/timer cleanup callbacks and disposes them on unmount. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/composables/useBackgroundRender.ts` | Larger composable wrapping AMLL background renderer initialization, reactive configuration, Tauri event listening, and explicit cleanup. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/composables/useDynamicSongTheme.ts` | Composable deriving dynamic CSS theme tokens from global playback store state via `storeToRefs`, `computed`, and `watch`. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/services/musicSdk.ts` | Typed frontend service facade over Tauri `invoke`, with DTO-like interfaces for music/search/playlist/singer responses and response normalization. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/store/Settings.ts` | Pinia setup-style store that manually persists nested settings to `localStorage` and merges saved settings with typed defaults. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/store/GlobalPlayStatus.ts` | Pinia setup-style store for global playback UI/server state, async watchers, lyric/comment fetching, image/color transformations, and cancellation. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/store/download.ts` | Pinia option-style store for backend download tasks, Tauri event synchronization, task getters, and command actions. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/types/songList.ts` | Shared playlist IPC/API contract types, generic `IPCResponse<T>`, result DTOs, and `SongListAPI` interface. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/types/audio.ts` | Audio event unions, enum, subscriber contracts, and `ControlAudioState` type. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/views/music/search.vue` | Representative component-level data fetching using `ref` state, `watch`, local loading flags, pagination, and `musicSdk`. |
| `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0/src/components/Find/PlaylistCategory.vue` | Representative local server-state cache pattern for playlist categories with a module-local `Record` cache and lifecycle hooks. |

### Code Patterns

#### Hook / composable patterns observed

- The project uses Vue 3 Composition API and names custom composables with a `use*` prefix under `src/composables/`. Existing files are `useEventListener.ts`, `useBackgroundRender.ts`, and `useDynamicSongTheme.ts`.
- Composables generally accept typed option objects or no arguments, create reactive local state with `ref`/`computed`, encapsulate side effects, and return a small explicit public API.
  - `useBackgroundRender` defines a local options interface whose fields are `Ref<...>` values (`UseBackgroundRenderOptions` at `src/composables/useBackgroundRender.ts:8-21`) and returns `{ isInitialized, init, pause, resume, dispose, bgRender }` (`src/composables/useBackgroundRender.ts:297-304`).
  - `useDynamicSongTheme` takes an optional callback option (`DynamicSongThemeOptions` at `src/composables/useDynamicSongTheme.ts:5-7`) and does not return values; it binds a global side effect to component lifetime (`src/composables/useDynamicSongTheme.ts:441-470`).
  - `useLifecycle` returns registration helpers plus `dispose` (`src/composables/useEventListener.ts:64-72`).
- Cleanup is explicit and lifecycle-bound.
  - `useLifecycle` stores registrations in an array, runs each `dispose`, clears the array, and wires cleanup to `onUnmounted` (`src/composables/useEventListener.ts:9-20`). It covers DOM listeners, Tauri listeners, Electron-like IPC listeners, intervals, timeouts, and arbitrary cleanup callbacks (`src/composables/useEventListener.ts:21-62`).
  - `useBackgroundRender` keeps a `spectrumUnlisten` reference, stops the listener before dispose, removes the canvas from the DOM, and calls renderer `dispose()` (`src/composables/useBackgroundRender.ts:173-211`).
  - `useDynamicSongTheme` stores the watcher stop handle and clears CSS variables in `onBeforeUnmount` (`src/composables/useDynamicSongTheme.ts:452-469`).
- Reactive effects use `watch` for input refs and store-derived values rather than a separate query library.
  - `useBackgroundRender` watches cover image, lyric availability, deep config, playing state, enabled state, and document visibility (`src/composables/useBackgroundRender.ts:216-295`).
  - `useDynamicSongTheme` derives a palette from `globalPlayStatus.player` via `computed`, applies it through an immediate `watch`, and invokes an optional callback on changes (`src/composables/useDynamicSongTheme.ts:441-463`).
- Components commonly use script setup with local `ref`/`computed` state plus lifecycle hooks.
  - `src/views/music/search.vue:13-27` declares local search results/loading/pagination/tab state. It watches the search store value, selected source, and active tab to fetch results (`src/views/music/search.vue:36-65`).
  - `src/components/Find/PlaylistCategory.vue:15-32` declares local playlist/category/pagination/cache state. It runs an immediate deep watcher on `storeToRefs(localUserStore).userSource` inside `onMounted` to refresh tags and playlists (`src/components/Find/PlaylistCategory.vue:158-172`), and registers/removes document listeners in `onMounted`/`onUnmounted` (`src/components/Find/PlaylistCategory.vue:174-180`).

#### Data-fetching patterns observed

- There is no TanStack Query/SWR-style server-state library in use. Data fetching is performed directly in services, stores, components, and utility modules using async functions, loading flags, `try/catch/finally`, pagination refs, and manual cache objects.
- `musicSdk` is the main typed service facade for music-source calls. It wraps Tauri `invoke('service_music_sdk_request', ...)`, injects the current selected source from `LocalUserDetailStore`, rewrites image URLs, and exposes method-specific functions such as `search`, `getCategoryPlaylists`, `getPlaylistDetail`, `getMusicUrl`, and `getPic` (`src/services/musicSdk.ts:90-208`).
- Some code calls the Electron-compatible `window.api` bridge directly rather than going through `musicSdk`.
  - `src/bridge/index.ts:117-144` defines `window.api.music.requestSdk` as an IPC wrapper around `service-music-sdk-request` and rewrites image URLs.
  - `src/store/search.ts:30-63` performs search and suggestion calls directly through `(window as any).api?.music?.requestSdk`.
  - `src/store/LocalUserDetail.ts:140-176` uses `(window as any).api?.songList` for playlist persistence backed by Rust SQLite.
  - `src/store/download.ts:48-95` loads download tasks via `window.api.download.getTasks()` and then synchronizes updates with Tauri `listen` events.
- Typical component fetching shape:
  - Guard against duplicate loads (`if (loading.value || !keyword.value.trim()) return` in `src/views/music/search.vue:74-77`).
  - Set loading flags before requests, update arrays/pagination totals on success, catch/log errors, and reset flags in `finally` (`src/views/music/search.vue:79-90`, `src/views/music/search.vue:97-110`).
  - Infinite scroll checks distance to bottom and calls the same fetch function for the next page (`src/views/music/search.vue:119-132`, `src/components/Find/PlaylistCategory.vue:127-132`).
- Manual cache patterns are local and route/source keyed.
  - `PlaylistCategory` uses `const categoryCache: Record<string, { list: any[]; page: number; total: number }> = {}` (`src/components/Find/PlaylistCategory.vue:29`) and keys by tag plus source (`src/components/Find/PlaylistCategory.vue:52-58`). Cached entries restore list/page/total during reset loads (`src/components/Find/PlaylistCategory.vue:59-66`) and are invalidated when the selected source changes (`src/components/Find/PlaylistCategory.vue:160-165`).
  - The IPC bridge has a short TTL cache for selected backend calls: `ipcCache` and `cachedInvoke` (`src/bridge/index.ts:24-47`), used for `httpProxy` with 5-minute TTL (`src/bridge/index.ts:146-148`).
- Cancellation/cleanup is used for long-running reactive fetches in playback state.
  - `GlobalPlayStatus` watches song identity/source/lyrics trigger, creates an `AbortController`, uses `onCleanup` to mark the request inactive and abort, then only writes lyrics/loading state when still active (`src/store/GlobalPlayStatus.ts:534-574`).
- DTO transformations are usually inline at service/component/store boundaries.
  - `musicSdk.getCategoryPlaylists` sorts Kuwo playlists by parsed play count and ID before returning a copied response (`src/services/musicSdk.ts:146-160`).
  - `src/views/music/search.vue:81-87` maps search results to add a UI `id` fallback.
  - `src/views/music/search.vue:99-107` maps playlist results into card fields (`title`, `description`, `cover`, etc.).
  - `src/views/music/list.vue:121-127` converts persisted playlist rows from JSON strings into `MusicItem`-like objects with a fallback shape.
  - `src/store/GlobalPlayStatus.ts:440-491` transforms song image source into proxied/default cover URL and includes source-specific branches for local and MG sources.

#### State management libraries and patterns observed

- Global state uses Pinia 3 with the persisted-state plugin installed in `main.ts`.
  - `createPinia()` is created and `pinia.use(piniaPluginPersistedstate)` is applied before `app.use(pinia)` (`src/main.ts:40-42`).
  - `src/store/index.ts:1-10` re-exports store factories under `use*Store` names, including an alias from `searchValue` to `useSearchStore`.
- Both Pinia setup-style and option-style stores are used.
  - Setup-style examples: `useSettingsStore` (`src/store/Settings.ts:43-139`), `useGlobalPlayStatusStore` (`src/store/GlobalPlayStatus.ts:384-659`), `LocalUserDetailStore` (`src/store/LocalUserDetail.ts:38-316`), `ControlAudioStore` (`src/store/ControlAudio.ts:16-149`), `usePluginStore` (`src/store/plugin.ts:36-...`), `useS3BackupStore` (`src/store/S3Backup.ts:23-197`).
  - Option-style examples: `searchValue` (`src/store/search.ts:10-83`), `useDownloadStore` (`src/store/download.ts:30-170`), and `playSetting` (`src/store/playSetting.ts:2-52`).
- Store naming conventions are mixed but mostly `useXStore` for exported store factories. Exceptions include `LocalUserDetailStore`, `ControlAudioStore`, `playSetting`, and the raw `searchValue` factory that is aliased in `src/store/index.ts`.
- Stores use `ref`, `reactive`, `computed`, getters, and actions for state transitions.
  - `Settings` uses a single `ref<SettingsState>` object with `updateSettings`, `toggleFloatBall`, and `saveSettings` (`src/store/Settings.ts:92-136`).
  - `ControlAudio` uses `reactive<ControlAudioState>` for audio state and a reactive subscriber registry keyed by an audio event union (`src/store/ControlAudio.ts:16-31`). It exposes `subscribe`, `publish`, and cleanup methods (`src/store/ControlAudio.ts:34-53`, `src/store/ControlAudio.ts:137-148`).
  - `download` uses option-store getters for active/completed/failed/downloading counts (`src/store/download.ts:36-46`) and actions to call backend commands (`src/store/download.ts:111-167`).
- Persistence has two forms:
  - Pinia persisted-state plugin with `persist: true`, e.g. `AudioEffects` (`src/store/AudioEffects.ts:49-51`), `Equalizer` (`src/store/Equalizer.ts:62-64`), `playSetting` (`src/store/playSetting.ts:51`), and `Auth` (`src/store/Auth.ts:96-98`). `audioOutput` persists only selected paths (`src/store/audioOutput.ts:341-345`).
  - Manual `localStorage` persistence for complex/nested or legacy data, usually with `persist: false`. `Settings` loads/merges/saves `appSettings` manually (`src/store/Settings.ts:63-123`, `src/store/Settings.ts:137-139`). `LocalUserDetail` loads/saves `userInfo` and `songList`, then starts debounced deep watchers for persistence (`src/store/LocalUserDetail.ts:48-91`, `src/store/LocalUserDetail.ts:316`). `search` manually persists only search history while the store itself has `persist: false` (`src/store/search.ts:16`, `src/store/search.ts:69-82`). `plugin` stores selected plugin ID/name in `localStorage` (`src/store/plugin.ts:42-58`). `S3Backup` stores config-related fields under local storage keys and initializes itself by calling `loadConfig()` at store creation (`src/store/S3Backup.ts:49-68`, `src/store/S3Backup.ts:175-178`).
- Backend/server state can live in stores when it is cross-component or event-synchronized.
  - `LocalUserDetail` keeps local playback list/user preferences plus SQLite-backed playlists and exposes playlist CRUD methods over `window.api.songList` (`src/store/LocalUserDetail.ts:44-46`, `src/store/LocalUserDetail.ts:140-176`).
  - `download` stores backend tasks and listens for `download:*` events to update local state in real time (`src/store/download.ts:48-95`).
  - `ControlAudio` mirrors Rust player events (`player:state`, `player:time`, `player:ended`, `player:crossfade_swap`, `player:error`) into frontend audio state and subscriber notifications (`src/store/ControlAudio.ts:55-101`).
- Local component state is used for transient UI concerns: tabs, loading flags, pagination, hover/sort state, search query, selected rows, scroll positions, modal visibility. Example: `SongVirtualList` keeps sort type, selected set, virtualizer state, and computed sorted/selected lists locally (`src/components/Music/SongVirtualList.vue:87-197`).
- `storeToRefs` is used when consuming Pinia state reactively from setup code, for example in `useDynamicSongTheme` (`src/composables/useDynamicSongTheme.ts:442-444`) and `PlaylistCategory` (`src/components/Find/PlaylistCategory.vue:8-10`).

#### TypeScript type conventions, models, and DTO transformations observed

- The project uses TypeScript with Vue SFC `<script setup lang="ts">`. Build script runs `vue-tsc --noEmit && vite build` (`package.json:7`). Dependencies include Vue 3, Pinia, Tauri, TDesign, `@vueuse/core`, and `@tanstack/vue-virtual` (`package.json:11-63`).
- Shared domain/API types are in `src/types/*.ts`, while feature-specific interfaces often live next to stores/services/components.
  - Shared: `src/types/audio.ts`, `src/types/songList.ts`, `src/types/userInfo.ts`, `src/types/hotkeys.ts`, `src/types/background.ts`, `src/types/window.d.ts`.
  - Service-local: `MusicItem`, `SearchResult`, `PlaylistItem`, `PlaylistResult`, `PlaylistDetailResult`, `SingerInfo`, and related result interfaces in `src/services/musicSdk.ts:4-88`.
  - Store-local: `SettingsState` and settings subtypes in `src/store/Settings.ts:5-41`; plugin interfaces in `src/store/plugin.ts:5-34`; download task interfaces/enums in `src/store/download.ts:3-28`; S3 backup interfaces in `src/store/S3Backup.ts:3-19`.
  - Component-local: props and sort unions in `src/components/Music/SongVirtualList.vue:18-43` and `src/components/Music/SongVirtualList.vue:87-95`.
- Type aliases and interfaces are both used. Interfaces are common for object models/DTOs; type aliases are common for unions, function signatures, and utility-shaped types.
  - `HotkeyAction` is a string-literal union; `HotkeyConfig` and `HotkeyStatus` use `Partial<Record<...>>` (`src/types/hotkeys.ts:1-21`).
  - `PlayMode` is an enum for playback modes (`src/types/audio.ts:18-23`).
  - `BackgroundRenderPreset` is a string-literal union and `BACKGROUND_PRESETS` is typed as `Record<Exclude<BackgroundRenderPreset, 'auto' | 'custom'>, BackgroundRenderConfig>` (`src/types/background.ts:7`, `src/types/background.ts:40-61`).
- Generic response wrappers model IPC/API results.
  - `IPCResponse<T = any>` has `success`, optional `data`, `error`, `message`, and `code` fields (`src/types/songList.ts:14-20`).
  - `SongListAPI` defines method return contracts like `Promise<IPCResponse<SongList[]>>`, `Promise<IPCResponse<boolean>>`, `Promise<IPCResponse<readonly Songs[]>>`, and `Partial<Omit<SongList, 'id' | 'createTime'>>` for edits (`src/types/songList.ts:22-48`).
  - `src/api/songList.ts` implements `SongListAPI`, catching thrown bridge errors and returning typed failure `IPCResponse` objects (`src/api/songList.ts:14-36`, `src/api/songList.ts:38-58`, `src/api/songList.ts:71-95`).
- The codebase often permits flexible plugin/backend payloads with `any`, index signatures, and casts.
  - `playList` has `types?: any`, `_types?: any`, `typeUrl?: any`, and `[key: string]: any` (`src/types/playList.ts:0-13`).
  - `UserInfo` has `[key: string]: any` for extra user/plugin fields (`src/types/userInfo.ts:6-21`).
  - `MusicItem` narrows core fields but still uses `_types?: Record<string, any>` and `typeUrl?: Record<string, any>` (`src/services/musicSdk.ts:4-19`).
  - `window` bridge types expose `any` arguments/results for dynamic IPC (`src/types/window.d.ts:3-17`).
  - Tauri/listener payloads are commonly typed as `any`, e.g. `useBackgroundRender` listens to `player:spectrum` with `(event: any)` (`src/composables/useBackgroundRender.ts:149-153`), and `download` keeps `song_info: any` in `DownloadTask` (`src/store/download.ts:12-16`).
- Local type assertions are common at boundary points.
  - JSON parsing is asserted to expected types in settings/user/list stores (`src/store/Settings.ts:67`, `src/store/LocalUserDetail.ts:52`, `src/store/LocalUserDetail.ts:62`).
  - `audioOutput` casts its partial persist config as `any` (`src/store/audioOutput.ts:341-345`).
  - Components cast service/store models when passing between legacy song shapes, e.g. `playStatus.updatePlayerInfo(song as any)` and `playSong(song as any)` in `src/views/music/search.vue:113-117`.
- Runtime validation is mostly ad hoc with `typeof`, `Array.isArray`, optional chaining, default fallbacks, `Number.isFinite`, and `try/catch`, not a validation library.
  - `musicSdk.parsePlaylistPlayCount` validates unknown input before converting Chinese unit strings to numbers (`src/services/musicSdk.ts:95-106`).
  - Fetch code checks arrays before assigning (`src/components/Find/LeaderBord.vue:17-19`, `src/components/Find/PlaylistCategory.vue:70-74`).
  - Settings load merges parsed objects with defaults and nested fallback fields rather than validating against a schema (`src/store/Settings.ts:63-84`).
  - Playlist row parsing catches invalid JSON and creates a fallback song shape (`src/views/music/list.vue:121-127`).
- Model transformations are performed by object spreading and field mapping rather than classes.
  - Settings merge default and parsed settings with nested default repair (`src/store/Settings.ts:67-84`).
  - Plugin supported sources are transformed into the user store's `supportedSources` record (`src/store/plugin.ts:77-85`).
  - Playlist search results are transformed into UI card data in `src/views/music/search.vue:101-107` and `src/components/Find/PlaylistCategory.vue:74-83`.

### External References

No external search was needed; the request was to summarize actual repository conventions.

### Related Specs

| File Path | Current State |
|---|---|
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/hook-guidelines.md` | Placeholder with sections for Overview, Custom Hook Patterns, Data Fetching, Naming Conventions, and Common Mistakes. |
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/state-management.md` | Placeholder with sections for Overview, State Categories, When to Use Global State, Server State, and Common Mistakes. |
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/type-safety.md` | Placeholder with sections for Overview, Type Organization, Validation, Common Patterns, and Forbidden Patterns. |
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/index.md` | States frontend guideline docs should capture actual project conventions and that documentation should be written in English. |

## Recommendations for the Three Spec Files

### `hook-guidelines.md`

Recommended content:

1. Document that this project uses Vue 3 Composition API composables, not React hooks.
2. Record naming/location convention: shared composables live in `src/composables/` and are named `useXxx`.
3. Describe custom composable structure:
   - Define local option interfaces when accepting structured inputs.
   - Prefer `Ref<T>` options for reactive external inputs.
   - Keep internal state in `ref`/`computed`/plain module variables as appropriate.
   - Return an explicit object of state and methods, or return nothing when the composable only binds a global side effect to lifecycle.
4. Include cleanup conventions:
   - Register listeners/timers and remove them in `onUnmounted`/`onBeforeUnmount`.
   - Store Tauri `UnlistenFn` handles and call them during cleanup.
   - Provide an explicit `dispose` method when the composable owns external resources.
5. Document watcher patterns:
   - Use `watch` for reactive input changes and side effects.
   - Use `deep: true` for nested settings objects where existing code does so.
   - Use `immediate: true` for initial application/fetching when needed.
6. Data-fetching section should state that the project does not use a query library; it uses direct async functions in services/components/stores with loading refs, pagination refs, `try/catch/finally`, and manual caches where needed.
7. Concrete examples to cite in the spec:
   - `src/composables/useEventListener.ts:9-72`
   - `src/composables/useBackgroundRender.ts:8-21`, `src/composables/useBackgroundRender.ts:216-304`
   - `src/composables/useDynamicSongTheme.ts:441-470`
   - `src/views/music/search.vue:36-111`
   - `src/components/Find/PlaylistCategory.vue:29-102`, `src/components/Find/PlaylistCategory.vue:158-180`

### `state-management.md`

Recommended content:

1. Document Pinia as the global state solution and note setup in `src/main.ts:40-42`.
2. Describe both setup-style and option-style store usage as actual conventions, with examples.
3. Define observed state categories:
   - Component-local UI state: tabs, forms/search, loading flags, pagination, scroll position, selection/sort state.
   - Global app/user/playback state: user settings, current playlist, playback status, plugin selection, auth, equalizer/audio effects.
   - Backend/event-synchronized state: downloads, Rust player state, SQLite-backed playlists.
   - Manual service/server state: fetched music lists and playlist categories in components/stores.
4. Describe when state is global in current code:
   - Shared across pages/components.
   - Persisted between sessions.
   - Mirrors backend events/commands.
   - Needed by global playback/control flows.
5. Persistence section should document both approaches:
   - `pinia-plugin-persistedstate` with `persist: true` or path-limited `persist.paths`.
   - Manual `localStorage` for nested/legacy/partial state with merging defaults, debounced watchers, or per-key persistence.
6. Server state section should explain that there is no central query cache; patterns are direct `musicSdk`/`window.api` calls, local loading flags, manual component caches, backend event listeners, and explicit refresh/invalidation.
7. Concrete examples to cite in the spec:
   - `src/main.ts:40-42`
   - `src/store/Settings.ts:43-139`
   - `src/store/LocalUserDetail.ts:48-91`, `src/store/LocalUserDetail.ts:140-176`
   - `src/store/download.ts:30-170`
   - `src/store/ControlAudio.ts:16-149`
   - `src/components/Find/PlaylistCategory.vue:29-102`

### `type-safety.md`

Recommended content:

1. Document that TypeScript is used throughout frontend code, with Vue SFCs using `<script setup lang="ts">`.
2. Describe type organization:
   - Shared domain/API contracts in `src/types/`.
   - Service DTOs in service files when scoped to the service.
   - Store-specific state/config interfaces in store files.
   - Component-specific prop/sort/event types inside SFCs.
3. Common patterns to document:
   - Interfaces for object models/DTOs.
   - Type aliases for unions, function signatures, re-exported external types, and object utility types.
   - Generic wrappers like `IPCResponse<T>`.
   - Utility types such as `Partial`, `Omit`, `Record`, `Exclude`, `readonly` arrays.
   - Literal unions/enums for finite states (`HotkeyAction`, `BackgroundRenderPreset`, `PlayMode`, `DownloadStatus`).
4. Boundary typing and transformation patterns:
   - Frontend services wrap IPC/Tauri calls and expose typed methods.
   - Raw backend/plugin responses are normalized with object mapping, URL rewriting, defaults, and array checks.
   - JSON/localStorage parsing is asserted to known shapes then merged with defaults.
5. Validation section should state that runtime validation is ad hoc; the code uses `Array.isArray`, `typeof`, `Number.isFinite`, optional chaining, defaults, and `try/catch`, with no schema validation library observed.
6. Mention flexible plugin/back-end payload areas as actual conventions:
   - Several boundary models use `any`, `Record<string, any>`, and index signatures for plugin metadata and dynamic IPC.
   - `window.api` is globally typed but still uses `any` for dynamic IPC arguments and results.
7. Concrete examples to cite in the spec:
   - `src/types/songList.ts:14-48`
   - `src/types/hotkeys.ts:1-21`
   - `src/types/background.ts:7-76`
   - `src/types/playList.ts:0-13`
   - `src/services/musicSdk.ts:4-88`, `src/services/musicSdk.ts:114-208`
   - `src/types/window.d.ts:2-17`
   - `src/components/Music/SongVirtualList.vue:18-43`, `src/components/Music/SongVirtualList.vue:87-95`

## Caveats / Not Found

- The active `.trellis` directory is not present in the worktree root; it exists at `/Users/vant/Documents/开发/Mio/音乐/.trellis`. The requested research file was written there.
- `python3 ./.trellis/scripts/task.py current --source` and `python3 ./.trellis/scripts/get_context.py --mode packages` could not run in the worktree because `.trellis/scripts/` is absent from `/Users/vant/Documents/开发/Mio/音乐/.claude/worktrees/agent-ae47a1ed7d2c50ac0`.
- No external research was performed because the request is about actual project conventions.
- No code or spec files were modified; only this research markdown file was written.
