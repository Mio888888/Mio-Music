# Research: current project plugin update

- **Query**: Research the active project for automatic checking for music/source plugin updates when the software home page opens: plugin management/update APIs, stores, settings UI, home lifecycle, notification patterns, and build scripts.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `src/store/plugin.ts` | Pinia setup-style plugin store. Defines `PluginInfo`, `PluginSource`, `LoadedPlugin`, plugin loading state, current plugin selection, initialize/refresh, add/update-by-target-id, uninstall, download/import, logs, config, and test connection APIs. |
| `src/bridge/index.ts` | Tauri/Electron-compatible `window.api` bridge. Exposes `window.api.plugins.*` methods used by the plugin store, including `downloadAndAdd(url, pluginType, targetPluginId?)`, `add(pluginCode, pluginName, targetPluginId?)`, `getCode`, and a currently exposed `onDeepLinkAdd` listener. |
| `src-tauri/src/plugin/commands.rs` | Tauri commands for plugin backend operations: initialize/list/add/uninstall/get info/call/download/import/config/logs/code. Downloads plugin code with `reqwest::get`, validates CR/LX format, optionally converts LX plugins, and calls `PluginManager::add_plugin`. |
| `src-tauri/src/plugin/manager.rs` | Backend plugin manager. Loads plugin files from app data `plugins/`, parses plugin metadata/sources via `PluginEngine`, writes plugin files, replaces an existing plugin file when `target_plugin_id` is passed or when the plugin name matches with a different version, and clears backend in-memory maps on uninstall. |
| `src-tauri/src/plugin/engine.rs` | Parses plugin JavaScript code for `PluginInfo` (`name`, `version`, `author`, `description`) and supported source IDs/qualities using regex/heuristics. |
| `src-tauri/src/plugin/types.rs` | Rust DTOs for plugin metadata, source list, config schema, and `LoadedPlugin`; fields mirror frontend store interfaces. |
| `src/components/Settings/PluginSettings.vue` | Settings UI for plugin management. Lists installed plugins and versions, supports refresh, local/online import, CR/LX selection, select active plugin, uninstall confirm, logs, service plugin config, and connection test. |
| `src/views/settings/sections/PluginSection.vue` | Thin settings section wrapper that renders `PluginSettings`. |
| `src/views/settings/sections/AboutSection.vue` | App update settings/UI. Uses Tauri updater `check()`, `downloadAndInstall`, `relaunch`, `MessagePlugin`, status cards, and `settings.autoUpdate` toggle. This is app-update logic, not plugin-update logic. |
| `src/store/Settings.ts` | Settings store using manual `localStorage` persistence under `appSettings`; currently includes `autoUpdate` for application update checks, but no plugin-update-specific setting. |
| `src/views/home/index.vue` | Home route-level view. Renders `HomeLayout`, nested child routes in `KeepAlive`, and `PlayMusic`. Has no current plugin update check. |
| `src/components/layout/HomeLayout.vue` | Main in-app shell with sidebar/header/source selector. Has `onMounted` setup and route watcher; reads selected plugin source data through `LocalUserDetailStore`. Has no current plugin update check. |
| `src/router/index.ts` | Route map. `/home` redirects to `/home/find` and uses `src/views/home/index.vue`; also preloads route components based on `routePreloadEnabled`. |
| `src/App.vue` | Root shell. On mount initializes local user/auth and redirects OAuth callbacks to `/home`; not specific to home-page opening. |
| `src/utils/plugin/PluginRunner.ts` | Main-thread proxy for plugin execution in a Web Worker. Provides `clearCache(pluginId?)`, called after plugin add/uninstall so updated plugin code is used. |
| `src/utils/plugin/pluginWorker.ts` | Web Worker plugin runtime. Loads plugin code via bridge `plugins.getCode`, executes CR/LX-like plugin code, caches loaded plugin exports by plugin ID, and supports `clearCache`. No plugin-update discovery logic found. |
| `src-tauri/src/lib.rs` | Tauri builder setup. Registers `PluginManager`, Tauri updater plugin, and plugin commands in `invoke_handler`. |
| `src-tauri/tauri.conf.json` | Tauri config. Contains app version, build commands, CSP, and Tauri app updater endpoint/public key; no plugin update endpoint/config found. |
| `package.json` | Scripts are `dev`, `build`, `preview`, `tauri`; dependencies include Tauri updater and TDesign. No plugin-specific build/update script found. |
| `.trellis/spec/frontend/state-management.md` | Frontend state guidelines: Pinia for plugins/settings/global state; manual server state via services/window.api; loading/error handling with `try/catch/finally`; manual `localStorage` persistence patterns. |
| `.trellis/spec/frontend/type-safety.md` | Type-safety guidelines for typed store contracts, pragmatic plugin/backend boundaries, and normalization/defaults at boundaries. |
| `.trellis/spec/frontend/directory-structure.md` | Directory conventions: route views under `src/views`, bridge under `src/bridge`, plugin utilities under `src/utils/plugin`, settings components/sections locations. |
| `.trellis/spec/frontend/hook-guidelines.md` | Lifecycle/composable guidelines: cleanup for listeners/timers/IPC, `try/catch/finally` for data fetching, and direct `window.api` usage when no wrapper exists or when editing bridge-oriented code. |

### Existing Plugin Update / Check Mechanisms

No explicit plugin update-check mechanism was found in the current project.

Existing mechanisms that can install or replace plugin code:

- Frontend store supports adding/replacing a plugin with an optional target ID:
  - `src/store/plugin.ts:158-166` calls `window.api.plugins.add(pluginCode, pluginName, targetPluginId)` and clears `PluginRunner` cache for the returned `plugin_id`.
  - `src/store/plugin.ts:188-194` calls `window.api.plugins.downloadAndAdd(url, pluginType, targetPluginId)` and refreshes the plugin list.
- Bridge exposes the optional replacement parameter:
  - `src/bridge/index.ts:158-171` maps `add` and `downloadAndAdd` to Tauri commands and passes `targetPluginId`.
- Backend commands accept `targetPluginId`:
  - `src-tauri/src/plugin/commands.rs:54-62` accepts `targetPluginId` for `plugin__add`.
  - `src-tauri/src/plugin/commands.rs:107-133` accepts `targetPluginId`, downloads plugin text with `reqwest::get`, validates plugin type, converts LX plugins when needed, then calls `pm.add_plugin`.
- Backend manager replacement behavior:
  - `src-tauri/src/plugin/manager.rs:82-132` builds a `PluginEngine`, parses info, chooses the plugin ID from `target_plugin_id` if provided, otherwise reuses an existing plugin ID when the plugin name matches and version differs, removes the old plugin file, writes the new code, reloads the engine, and updates in-memory maps.
  - `src-tauri/src/plugin/manager.rs:98-100` rejects duplicate install only when an existing plugin has the same plugin name and same version.
- Plugin metadata used for comparison is available:
  - `src/store/plugin.ts:5-24` defines frontend `PluginInfo.version` and `LoadedPlugin.plugin_info`.
  - `src-tauri/src/plugin/types.rs:3-10` defines Rust `PluginInfo.version`.
  - `src-tauri/src/plugin/engine.rs:208-222` parses `name`, `version`, `author`, `description` from plugin code, defaulting version to `1.0.0`.

Existing mechanisms that are not update checks:

- App updater in `src/views/settings/sections/AboutSection.vue` checks the Tauri application release, not music/source plugins. It imports `check` from `@tauri-apps/plugin-updater` at `src/views/settings/sections/AboutSection.vue:4`, calls `check()` at `src/views/settings/sections/AboutSection.vue:50-58`, downloads/installs app updates at `src/views/settings/sections/AboutSection.vue:76-104`, and uses `relaunch()` at `src/views/settings/sections/AboutSection.vue:111-117`.
- `src/bridge/index.ts:187-194` exposes `onDeepLinkAdd` for `plugin-add-link`, but focused search found no emitter/consumer for `plugin-add-link` in the current source beyond this bridge declaration.
- `src/utils/plugin/pluginWorker.ts:621-640` caches plugin code/exports by plugin ID for execution; this is runtime caching, not update discovery.

### Likely Files to Modify

Likely frontend files for automatic checking when home opens:

| File Path | Why likely relevant |
|---|---|
| `src/views/home/index.vue` | Route-level home component is loaded for `/home`; currently has no script-side lifecycle except imports. It is the most direct “home page opens” lifecycle point. |
| `src/components/layout/HomeLayout.vue` | Already has `onMounted` and route/source shell logic. If the intended trigger is “main home shell mounted,” this is another lifecycle point. Need to avoid repeated checks if remounted or route transitions occur. |
| `src/store/plugin.ts` | Existing shared plugin store is the natural place for plugin update state/actions because plugin operations are already centralized here. Existing actions already wrap add/download/import/uninstall/config/log APIs. |
| `src/bridge/index.ts` | Needed if a new backend command is introduced for update metadata/checking; existing bridge already exposes plugin methods. |
| `src-tauri/src/plugin/commands.rs` | Needed if update checking is implemented in Rust/Tauri, for example to fetch update manifests or remote plugin code and return comparison results. Existing download path is here. |
| `src-tauri/src/plugin/manager.rs` | Needed if backend needs a new “check installed plugins for updates” helper or access to plugin install metadata beyond current loaded plugin fields. |
| `src-tauri/src/plugin/types.rs` | Needed if new typed DTOs are returned for update availability/status. |
| `src-tauri/src/lib.rs` | Needed if any new Tauri command is added; plugin commands must be registered in the `generate_handler!` list near `src-tauri/src/lib.rs:351-366`. |
| `src/store/Settings.ts` | Needed if a user setting for automatic plugin update checks is added; current `SettingsState` only has `autoUpdate` for application updates at `src/store/Settings.ts:30` and defaults it to `true` at `src/store/Settings.ts:51`. |
| `src/components/Settings/PluginSettings.vue` | Likely relevant if plugin update status/manual update UI is added beside existing plugin cards and refresh/import actions. Existing plugin list shows version at `src/components/Settings/PluginSettings.vue:46-50`. |
| `src/types/window.d.ts` | Likely relevant if typed `window.api.plugins` declarations need to include new bridge methods. |

Build/config files that may be touched only if necessary:

- `src-tauri/tauri.conf.json` has CSP allowing `http:` and `https:` for images/media but default connect behavior is governed by Tauri/IPC and Rust `reqwest`; no plugin update endpoint currently exists (`src-tauri/tauri.conf.json:22-29` only configures the app updater).
- `package.json` scripts are generic (`dev`, `build`, `preview`, `tauri`) at `package.json:5-10`; no plugin-update build script exists.

### Code Patterns

#### Plugin store patterns

- Store shape is setup-style Pinia (`defineStore('plugin', () => { ... })`) with `ref` state and returned actions (`src/store/plugin.ts:36-257`).
- Initialization loads all plugins from backend, stores them in `plugins`, then syncs selected plugin source data into `LocalUserDetailStore` (`src/store/plugin.ts:60-121`). Errors during initialize/refresh are logged internally and swallowed (`src/store/plugin.ts:116-132`), so UI code that expects thrown errors should verify behavior before relying on `catch`.
- Mutating operations throw user-facing errors when bridge/backend returns `success: false` (`src/store/plugin.ts:165-178`, `src/store/plugin.ts:193-204`, `src/store/plugin.ts:226-233`).
- Add/uninstall clear plugin runtime cache via `PluginRunner.clearCache` (`src/store/plugin.ts:160-173`). `downloadAndAdd` currently refreshes the plugin list but does not explicitly clear `PluginRunner` cache in the store (`src/store/plugin.ts:188-194`); backend replacement still rewrites the file.

#### Plugin backend patterns

- Commands use a JSON response wrapper: `ok(...)` returns `{ success: true, data }`; `err(...)` returns `{ success: false, error }` (`src-tauri/src/plugin/commands.rs:9-19`). Some validation/download errors are returned by `Err(...)`, which bridge `ipcInvoke` logs and rethrows (`src/bridge/index.ts:62-69`).
- `payload(args)` unwraps the bridge’s `{ args: { ... } }` nesting (`src-tauri/src/plugin/commands.rs:21-38`). Existing plugin commands should be mirrored if adding new parameters.
- `PluginManager::add_plugin` uses plugin name/version to prevent exact duplicates and supports replacement by target plugin ID (`src-tauri/src/plugin/manager.rs:93-108`).
- Plugin files are stored under app data `plugins/` and named `{plugin_id}-{safe_name}` (`src-tauri/src/plugin/manager.rs:16-23`, `src-tauri/src/plugin/manager.rs:109-112`). No current metadata field stores the original download URL/update URL.

#### Home lifecycle patterns

- `/home` route is defined with redirect to `/home/find` and component `@/views/home/index.vue` (`src/router/index.ts:27-30`).
- `src/views/home/index.vue` wraps child routes in `KeepAlive` and renders `PlayMusic` (`src/views/home/index.vue:3-14`). The route-level component is a direct place to run a one-time home-open side effect.
- `HomeLayout` already uses `onMounted`/`onUnmounted` for a `watchEffect` (`src/components/layout/HomeLayout.vue:12-24`) and watches `route.path` with `{ immediate: true }` to sync nav state (`src/components/layout/HomeLayout.vue:61-68`).
- `App.vue` root `onMounted` initializes local user/auth and may navigate to `/home` after auth callback (`src/App.vue:27-41`). It is global app startup, not home-specific.
- Router has route preloading that may import route components after load (`src/router/index.ts:89-108`). Because preloading calls route component import functions, update-check side effects should be inside component setup/lifecycle rather than module top-level.

#### Notification and error handling patterns

- TDesign `MessagePlugin` is the common toast-style pattern for success/warning/error in plugin settings (`src/components/Settings/PluginSettings.vue:286`, `src/components/Settings/PluginSettings.vue:358-376`, `src/components/Settings/PluginSettings.vue:451-455`, `src/components/Settings/PluginSettings.vue:525-558`).
- TDesign `DialogPlugin.confirm` is used for destructive confirmation (`src/components/Settings/PluginSettings.vue:436-457`).
- Plugin settings page uses inline state blocks for loading/empty/error with `error` ref (`src/components/Settings/PluginSettings.vue:14-28`, `src/components/Settings/PluginSettings.vue:293-335`).
- About/app updater page uses inline update status cards rather than toasts for check results (`src/views/settings/sections/AboutSection.vue:20-27`, `src/views/settings/sections/AboutSection.vue:42-66`, `src/views/settings/sections/AboutSection.vue:159-214`). It logs check errors to console and stores a user-facing error message (`src/views/settings/sections/AboutSection.vue:59-63`).
- Bridge IPC failures are logged with `console.warn` and rethrown (`src/bridge/index.ts:62-69`). Store/component actions commonly convert errors to `MessagePlugin.error` messages.

#### Settings and persistence patterns

- `src/store/Settings.ts` manually loads settings from `localStorage.getItem('appSettings')`, merges saved data with defaults, and saves back using `localStorage.setItem` (`src/store/Settings.ts:63-113`).
- `updateSettings` merges partial settings and calls `saveSettings` (`src/store/Settings.ts:115-124`).
- Existing `autoUpdate` setting is named generically and displayed as “应用启动时检查更新” in About settings (`src/views/settings/sections/AboutSection.vue:145-147`), so it currently refers to app startup update checks, not plugin checks.

### Related Specs

- `.trellis/spec/frontend/state-management.md` — Pinia is used for shared plugin/settings state; manual server state uses services/API/`window.api` with loading/error state; persistence merges defaults from `localStorage` (`state-management.md:8-10`, `state-management.md:27-35`, `state-management.md:46-49`, `state-management.md:62-70`, `state-management.md:73-80`).
- `.trellis/spec/frontend/type-safety.md` — Store-specific plugin/settings interfaces can live in store files; plugin/backend boundaries may remain flexible but should be normalized/defaulted at boundaries (`type-safety.md:8-10`, `type-safety.md:16-20`, `type-safety.md:39-46`, `type-safety.md:71-89`).
- `.trellis/spec/frontend/directory-structure.md` — Confirms `src/bridge`, `src/store`, `src/utils/plugin`, `src/views`, and settings component locations (`directory-structure.md:16-31`, `directory-structure.md:43-52`).
- `.trellis/spec/frontend/hook-guidelines.md` — Lifecycle side effects should clean up listeners/timers; direct `window.api` calls are acceptable when no wrapper exists or in bridge-oriented code; async data fetching uses `try/catch/finally` (`hook-guidelines.md:29-35`, `hook-guidelines.md:44-58`, `hook-guidelines.md:76-81`).
- `.trellis/spec/frontend/component-guidelines.md` — Vue SFCs use Composition API, TDesign controls, local refs/computed/watch, lifecycle cleanup, and TDesign overlay patterns (`component-guidelines.md:6-11`, `component-guidelines.md:47-64`).

### External References

No external references were needed for this internal project research.

## Caveats / Not Found

- No installed plugin update source/manifest URL is stored in current `LoadedPlugin`, plugin file metadata, or settings. Existing install/update-by-target-ID APIs require the caller to already know a remote plugin URL or provide plugin code.
- No explicit “check all installed plugins for updates” API was found in frontend store, bridge, or Rust backend.
- No plugin-update notification UI was found outside manual plugin install/uninstall/config messages.
- The app updater setting `autoUpdate` exists, but no active startup auto-check usage was found in the inspected files; the About page exposes manual `handleCheckUpdate` and toggle UI. This setting should not be assumed to cover plugin update checks without confirming product intent.
- `window.api.plugins.onDeepLinkAdd` exists in the bridge, but no current emitter or component consumer for `plugin-add-link` was found by focused search.
- `downloadAndAdd` supports a `targetPluginId` replacement path, but current `PluginSettings.vue` online import calls `store.downloadAndAdd(onlineUrl, addPluginType)` without target ID (`src/components/Settings/PluginSettings.vue:369-371`), so the UI path is install-by-name/version rather than explicit update of a specific card.
- Version comparison helpers (semantic version compare, remote metadata compare) were not found. Current duplicate logic only checks exact string equality for plugin name/version in Rust manager.
- Build scripts/config contain Tauri app updater setup, but no plugin-update endpoint, manifest generation, or plugin registry config was found.
