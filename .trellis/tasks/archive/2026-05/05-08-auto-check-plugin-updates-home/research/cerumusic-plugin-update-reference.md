# Research: CeruMusic 插件更新检查参考

- **Query**: Research the read-only reference project `CeruMusic-main` for automatic checking for music/source plugin updates on home page; inspect plugin update check behavior, stores/services/components involved, auto-check timing, notification UI, throttling/caching, and error handling.
- **Scope**: mixed internal reference research + active-project compatibility notes
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts` | CeruMusic CR plugin host; exposes `cerumusic.NoticeCenter`, forwards update notices to main-process notification sender. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts` | LX plugin converter; handles `lx.EVENT_NAMES.updateAlert` and converts it into `NoticeCenter('update', ...)`; includes one-run duplicate guard. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts` | Main-process plugin notification normalizer; validates update URLs, builds dialog payloads, sends `plugin-notice` IPC event. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/preload/index.ts` | Exposes renderer APIs for `plugins.downloadAndAddPlugin(...)` and `pluginNotice.onPluginNotice(...)`. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue` | Renderer notification dialog; queues notices, delays display on welcome route, installs plugin update via plugin API, falls back to browser on update failure. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/layout/Provider.vue` | Global provider mounts `PluginNoticeDialog`; handles route/interactive timing for other app prompts. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts` | Plugin installation/update service; `downloadAndAddPlugin` downloads update code and `addPlugin` replaces existing plugin by `targetPluginId` or same plugin name. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/plugins.ts` | IPC handlers for plugin add/download/list/uninstall operations. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/index.ts` | Main-process startup; initializes plugins before creating the window and initializes plugin notice with the main window. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/router/routes.ts` | Deeplink route supports direct plugin update/install paths and uses the same download/update service. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/plugin-notice-usage.md` | Documents plugin notification payloads, especially `type: 'update'`, `url`, `version`, and `pluginInfo.type`. |
| `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md` | Documents CR plugin self-update example with `checkUpdate()` and LX `updateAlert` event. |
| `/Users/vant/Documents/开发/Mio/音乐/src/store/plugin.ts` | Active project plugin Pinia store; initializes/list/add/download/update APIs and stores current plugin selection. |
| `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts` | Active project plugin executor; currently `NoticeCenter(...)` only logs and LX `send` ignores `updateAlert`. |
| `/Users/vant/Documents/开发/Mio/音乐/src/bridge/index.ts` | Active project frontend bridge; exposes Tauri plugin APIs and HTTP proxy cache. |
| `/Users/vant/Documents/开发/Mio/音乐/src/components/Settings/PluginSettings.vue` | Active project plugin management UI; has manual install/refresh and existing update-capable `downloadAndAdd` store path. |
| `/Users/vant/Documents/开发/Mio/音乐/src/views/home/index.vue` | Active project home wrapper where home-page auto-check behavior can be triggered after layout/store initialization. |

### Reference Behavior Summary

CeruMusic-main does not have a central app-owned “check all plugins for updates” scheduler. Its update check behavior is plugin-driven:

1. Installed plugins are initialized at app startup.
   - `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/index.ts:547-551` calls `pluginService.initializePlugins()` before `createWindow()`.
   - `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/index.ts:502-503` registers plugin IPC services and passes the created main window into `initPluginNotice(mainWindow)`.
2. CR plugins can perform their own top-level `checkUpdate()` while their plugin code is loaded.
   - The docs example defines `pluginInfo.updateMd5`, `apiKey`, and `type` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:511-520`.
   - It defines `checkUpdate` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:624-658` and calls it immediately at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:659-661`.
3. CR plugins notify the app through `cerumusic.NoticeCenter('update', payload)`.
   - `CeruMusicPluginHost` exposes `NoticeCenter` via `_getCerumusicApi()` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts:580-587`.
   - `_createNoticeCenter()` sends the plugin notice with current version and plugin id at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts:782-801`.
4. LX plugins use `lx.send(lx.EVENT_NAMES.updateAlert, { log, updateUrl })`; CeruMusic’s converter maps that to a normal update notification.
   - The LX event names include `updateAlert` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:169-173`.
   - The converter catches update events at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:200-203`.
   - The docs show the LX update event at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:807-811`.

### Update Check Flow and UI Behavior

#### CR plugin flow

1. App starts and loads installed plugins from disk.
2. Plugin code runs in the host sandbox.
3. If the plugin author included a top-level `checkUpdate().then(...)`, that check runs during plugin initialization.
4. If remote data indicates a new version, plugin calls `NoticeCenter('update', { title, content, url, version, pluginInfo })`.
5. `CeruMusicPluginHost` wraps the event with the installed plugin’s `currentVersion` and `pluginId`.
6. Main process normalizes the payload and emits `plugin-notice` to renderer.
7. Global renderer dialog queues and displays the notice; user can update immediately or postpone.
8. Immediate update calls `window.api.plugins.downloadAndAddPlugin(updateUrl, pluginType, pluginId)`, which replaces the existing plugin.

Key reference lines:

- CR update example request endpoint and notification: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:624-650`.
- Main-process update payload creation: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts:125-143`.
- Renderer update action: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:152-166`.

#### LX plugin flow

1. LX plugin emits `updateAlert` through `lx.send(...)`.
2. Converter validates `log` and `updateUrl`, truncates long values, and rejects non-HTTP(S) URLs.
3. Converter sends `NoticeCenter('update', ...)` with `pluginInfo.type = 'lx'`.
4. Remaining UI/install flow is identical to CR update notices.

Key reference lines:

- Duplicate guard and missing-log handling: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:98-110`.
- URL truncation and protocol validation: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:114-133`.
- Converted update notice: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:138-154`.

#### Notification UI behavior

- `PluginNoticeDialog.vue` listens globally on mount through `window.api.pluginNotice.onPluginNotice(...)` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:265-268`.
- Notices are queued in `noticeQueue`; if no dialog is visible, the next notice is shown immediately: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:106-115`.
- Dialog display is suppressed while the current route is welcome (`/` or route name `welcome`): `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:119-124`.
- When route changes away from welcome, queued notices are displayed after 1 second: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:244-256`.
- Dialog title includes the remaining queue count: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:94-104`.
- Update dialogs are wider and show current/new version plus plugin type tag: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:18-32` and `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:89-92`.
- Update failure handling asks whether to open the update URL in the browser: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:167-183`.
- The dialog is mounted globally by Provider at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/layout/Provider.vue:663-666`.

### Stores / Services / Components Involved

#### CeruMusic-main reference

- Plugin service:
  - `addPlugin` parses plugin metadata, detects same-name updates, deletes old plugin file, reloads the plugin, and returns `pluginId`, `pluginInfo`, `supportedSources`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts:63-143`.
  - `downloadAndAddPlugin` validates URL, downloads code, checks CR/LX format, converts LX plugins, and delegates to `addPlugin`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts:268-300`.
  - Download timeout and network error mapping are handled in `downloadFile`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts:303-331`.
- IPC/preload:
  - Main IPC handler returns `{ error: error.message }` on plugin download errors: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/plugins.ts:17-26`.
  - Preload exposes `plugins.downloadAndAddPlugin(url, type, targetPluginId)` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/preload/index.ts:41-49`.
  - Preload exposes `pluginNotice.onPluginNotice(...)` at `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/preload/index.ts:370-378`.
- Main-process notification:
  - `sendPluginNotice` validates update URLs and maps update notices to an action list of “稍后更新” and “立即更新”: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts:125-139`.
- Renderer notification UI:
  - `PluginNoticeDialog.vue` handles queueing, route suppression, update install, fallback browser open, and normal install confirmation.

#### Active project counterpart files

- `src/store/plugin.ts` already exposes `initialize`, `refresh`, `downloadAndAdd(url, pluginType, targetPluginId)`, and persists current plugin selection: `/Users/vant/Documents/开发/Mio/音乐/src/store/plugin.ts:60-68`, `/Users/vant/Documents/开发/Mio/音乐/src/store/plugin.ts:188-195`.
- `src/bridge/index.ts` maps `plugins.downloadAndAdd(...)` to Tauri command `plugin__download_and_add`: `/Users/vant/Documents/开发/Mio/音乐/src/bridge/index.ts:155-187`.
- `src/components/Settings/PluginSettings.vue` already uses `store.downloadAndAdd(...)` for online plugin install and TDesign messages: `/Users/vant/Documents/开发/Mio/音乐/src/components/Settings/PluginSettings.vue:352-377`.
- `src/views/home/index.vue` wraps `HomeLayout`, route view, and player; this is the requested home-page entry point: `/Users/vant/Documents/开发/Mio/音乐/src/views/home/index.vue:1-22`.
- `src/utils/plugin/pluginWorker.ts` currently logs `NoticeCenter(...)` only and does not propagate update notices: `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts:440-443`.
- Active LX mock ignores `send(...)`; therefore `updateAlert` is not converted to a UI notice in the active project: `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts:520-525`.
- Active Rust LX converter also has `updateAlert` in event names but its `send` handler only handles `inited`, not `updateAlert`: `/Users/vant/Documents/开发/Mio/音乐/src-tauri/src/plugin/converter.rs:152-176`.

### Code Patterns

#### Plugin-owned check, app-owned display/install

CeruMusic-main separates responsibilities:

- Plugin owns update detection and remote version semantics.
- App owns notification display and installation/update action.

CR example from docs:

```javascript
const checkUpdate = async () => {
  const { body } = await request(
    `${apiUrl}/script?checkUpdate=${pluginInfo.updateMd5}&key=${pluginInfo.apiKey}&type=${pluginInfo.type}`,
    { method: 'GET', headers: { 'Content-Type': 'application/json' } }
  )
  if (body.data != null) {
    NoticeCenter('update', {
      title: `${pluginInfo.name} 有新的版本 ${body.data.version}`,
      content: body.data.updateMsg,
      url: `${body.data.updateUrl}`,
      version: body.data.version,
      pluginInfo: { name: pluginInfo.name, type: 'cr' }
    })
  }
}
checkUpdate().then(() => console.log('版本更新检测完成'))
```

Source: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:624-661`.

#### NoticeCenter normalization

`sendPluginNotice` only treats a notice as an update dialog if `type === 'update'`, `data.url` exists, and URL protocol is HTTP(S). Invalid or missing URL falls through to normal notice behavior.

Source: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts:62-68`, `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts:125-143`.

#### Update install by target plugin id

The update button passes the original `pluginId` to `downloadAndAddPlugin`, preserving current plugin identity where possible:

- Renderer action includes `notice.value.pluginId`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:155-159`.
- Service treats `targetPluginId` as explicit update and deletes old plugin file before writing the new plugin: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts:78-116`.

#### Queue and route suppression

The UI keeps notification state local to the dialog component, not in a store. It queues notices, suppresses display on the welcome route, and replays queue after route change.

Source: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:83-141`, `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:244-256`.

### Auto-check Timing

- CeruMusic-main’s app-level timing is plugin initialization at desktop app startup, before window creation: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/index.ts:547-556`.
- CR plugin timing is author-controlled because the sample plugin calls `checkUpdate()` at top level immediately after defining it: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:659-661`.
- LX plugin timing is author-controlled by when it emits `updateAlert`; CeruMusic only converts/display the event.
- Renderer display timing is global-provider mounted but delayed on welcome route until the user leaves welcome: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:119-124`, `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:244-256`.

### Throttling / Caching

What exists in CeruMusic-main:

- LX converted plugins have an in-script duplicate guard: `updateAlertSent` prevents repeated `updateAlert` notifications during one converted plugin runtime: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:98-104`, `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:135-137`.
- Update logs and URLs are truncated to 1024 characters in the LX converter: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts:116-127`.
- Renderer dialog has queueing and a 300ms delay between notices to avoid fast dialog switching: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:232-241`.

Not found in CeruMusic-main:

- No persisted `lastCheck` / last update prompt timestamp for plugin update checks was found in the reference plugin update code.
- No app-owned “once per day” throttle for plugin update checks was found.
- No global store for plugin update notification state was found.
- No central endpoint for checking all installed plugins was found.

Active project note:

- The active project has HTTP proxy caching at the bridge level for 5 minutes: `/Users/vant/Documents/开发/Mio/音乐/src/bridge/index.ts:147-149`. This is generic HTTP proxy caching, not plugin update-specific throttling.

### Error Handling

#### CeruMusic-main

- Plugin host request timeouts return a structured result with status `408` instead of throwing directly inside `_makeHttpRequest`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts:715-763`.
- `sendPluginNotice` catches and logs notification-send failures: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/pluginNotice.ts:175-177`.
- Plugin download maps HTTP/network failures to human-readable messages: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/index.ts:303-331`.
- IPC plugin download handler catches service errors and returns `{ error }`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/events/plugins.ts:17-26`.
- Renderer update action catches internal update failure and offers manual browser fallback: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue:167-183`.
- Plugin author sample catches `checkUpdate` errors and logs them without surfacing an app-level dialog: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/docs/guide/CeruMusicPluginDev.md:655-657`.

#### Active project compatibility

- Active `store.downloadAndAdd(...)` throws `res.error || '下载插件失败'` when Tauri command fails: `/Users/vant/Documents/开发/Mio/音乐/src/store/plugin.ts:188-195`.
- Active `PluginSettings.vue` already shows install errors through `MessagePlugin.error(...)`: `/Users/vant/Documents/开发/Mio/音乐/src/components/Settings/PluginSettings.vue:374-377`.
- Active `PluginWorker` request bridge maps HTTP proxy failures into `网络请求失败: ...`: `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts:424-432`.

### Transferable Patterns vs Incompatible Differences

#### Transferable patterns

| Pattern | Reference | Active-project adaptation surface |
|---|---|---|
| Plugin-driven update semantics | CR plugins call `NoticeCenter('update', ...)`; LX plugins emit `updateAlert`. | Keep update detection in plugin runtime where possible; app displays and installs updates. |
| Global notification listener/dialog | `PluginNoticeDialog.vue` mounted by Provider. | Active project can use a global/home-mounted component or composable with TDesign dialog/message. |
| Queue update notices | Renderer queues multiple plugin notices. | Useful if multiple installed plugins emit updates at startup/home entry. |
| Delay/suppress notices on welcome route | Reference does not show update prompts on welcome. | User goal says home page; trigger/display after entering `/home` rather than welcome. |
| Preserve plugin id during update | Update action passes target plugin id to download/install. | Active store already supports `downloadAndAdd(url, pluginType, targetPluginId)`. |
| Fallback to browser after install failure | Reference asks user to open URL manually. | Can reuse same UX with `window.open(updateUrl)` after Tauri install fails. |
| Local component state for dialog queue | Reference keeps queue in component refs. | Matches active frontend spec guidance to keep transient UI local. |

#### Incompatible differences

| Difference | Evidence | Impact |
|---|---|---|
| Reference is Electron main/preload; active project is Tauri + frontend bridge. | Ceru uses `ipcMain`, `ipcRenderer`, `BrowserWindow`; active uses `src/bridge/index.ts` and Tauri commands. | Do not copy Electron IPC code directly; route through active `window.api` bridge/store. |
| Reference runs plugins in main-process VM; active runs plugins in Web Worker. | Ceru host uses `vm.runInNewContext`: `/Users/vant/Documents/开发/Mio/音乐/CeruMusic-main/src/main/services/plugin/manager/CeruMusicPluginHost.ts:378-384`; active uses Worker: `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/PluginRunner.ts:12-16`. | Notification events from plugin code must cross Worker → main thread before UI can show them. |
| Active `NoticeCenter` currently only logs. | `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts:440-443`. | CR plugin update checks may run but will not surface to UI. |
| Active LX mock ignores `updateAlert`. | `/Users/vant/Documents/开发/Mio/音乐/src/utils/plugin/pluginWorker.ts:520-525`; Rust converter also ignores updateAlert at `/Users/vant/Documents/开发/Mio/音乐/src-tauri/src/plugin/converter.rs:163-176`. | LX update prompts from plugins will be lost unless this event is bridged/converted. |
| Active plugin metadata uses snake_case arrays; Ceru uses camelCase objects. | Active `LoadedPlugin` fields: `/Users/vant/Documents/开发/Mio/音乐/src/store/plugin.ts:18-24`; Ceru settings UI uses `plugin.pluginInfo` / `supportedSources`. | UI payloads should use active field names (`plugin_id`, `plugin_info`, `supported_sources`). |
| Ceru has no persistent update-check throttle. | No `lastCheck` / update cache found in reference update files. | Any daily/home-entry throttle in active project would be a new adaptation, not copied from Ceru behavior. |

### Recommended Adaptation for Active Project

Given the goal “automatic checking for music/source plugin updates on home page,” the closest adaptation from CeruMusic-main is:

1. Use home-page entry as the app-owned trigger instead of Electron startup.
   - Active target: `/Users/vant/Documents/开发/Mio/音乐/src/views/home/index.vue`.
   - Existing plugin store can initialize and list installed plugins through `usePluginStore().initialize()`.
2. Reuse the existing active plugin update/install path.
   - `src/store/plugin.ts:188-195` already supports `downloadAndAdd(url, pluginType, targetPluginId)`.
   - Use `targetPluginId` to preserve the existing installed plugin identity, mirroring Ceru’s update action.
3. Add an active-project equivalent of Ceru’s plugin notice bridge before relying on plugin self-check output.
   - CR update path requires `NoticeCenter('update', data)` to emit a Worker message instead of only logging.
   - LX path requires `lx.send('updateAlert', data)` to convert into the same update notice payload, mirroring Ceru’s converter.
4. Use a TDesign dialog/queue UI modeled after `PluginNoticeDialog.vue`.
   - Fields to preserve: `pluginId`, `pluginName`, `pluginType`, `currentVersion`, `newVersion`, `updateUrl`, `actions`.
   - Button behavior: “稍后更新” closes; “立即更新” calls `store.downloadAndAdd(updateUrl, pluginType, pluginId)`; on failure, offer browser fallback.
5. Add home-entry throttling only as active-project-specific behavior.
   - CeruMusic-main does not persist plugin update-check timestamps.
   - If implemented, store a simple per-plugin timestamp in localStorage or Pinia/manual persistence and document that it is an active-project addition, not reference behavior.
6. Keep transient queue/dialog state local unless multiple pages need shared access.
   - This follows active specs: component-local state for modal visibility/queues; Pinia for shared plugin state.

A minimal active-project flow consistent with Ceru would be:

1. Home mounts.
2. `usePluginStore().initialize()` loads installed plugins.
3. For each installed music-source plugin, ensure its code is executed or call an update-check method if present/available.
4. Plugin emits an update notice through Worker bridge (`NoticeCenter('update')` or LX `updateAlert`).
5. Home/global dialog queues notices.
6. User clicks immediate update.
7. `usePluginStore().downloadAndAdd(updateUrl, pluginType, pluginId)` installs replacement and refreshes plugin list.
8. If updated plugin is current, reconcile `LocalUserDetailStore` source/quality selection using the refreshed plugin metadata, following the existing selection logic in `src/store/plugin.ts:69-115` and `src/components/Settings/PluginSettings.vue:397-433`.

### Related Specs

| Spec Path | Description |
|---|---|
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/index.md` | Frontend guide index; load component/state/type guidelines before implementation. |
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/component-guidelines.md` | Vue 3 + Composition API + TDesign patterns; local state for modal/dialog queues; cleanup for listeners/timers. |
| `/Users/vant/Documents/开发/Mio/音乐/.trellis/spec/frontend/state-management.md` | Pinia/manual persistence guidance; global plugin state belongs in store, transient dialog state should remain local. |

### External References

None. This research was internal/reference-project only.

## Caveats / Not Found

- No central CeruMusic-main service that periodically checks every installed plugin for updates was found.
- No CeruMusic-main persisted plugin update throttle/cache (`lastCheck`, daily check, dismissed-version cache) was found.
- CeruMusic-main’s documented CR update check is plugin-authored sample code, not app-enforced behavior.
- Active project currently has the install/update backend path but does not yet surface plugin `NoticeCenter('update')` or LX `updateAlert` to UI.
- This file describes reference behavior and adaptation points only; no CeruMusic-main, app source, or spec files were modified.
