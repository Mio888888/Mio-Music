# Research: directory-structure

- **Query**: summarize the project's actual frontend directory/file organization for `.trellis/spec/frontend/directory-structure.md`
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `src/main.ts` | Frontend application entrypoint; imports global CSS, registers router/pinia, initializes bridge and runtime config. |
| `src/App.vue` | Root shell that mounts `Provider`, `GlobalBackground`, `GlobalContextMenu`, and the top-level router view. |
| `src/router/index.ts` | Central route map for welcome/home/settings plus special windows like desktop lyrics and recognition worker. |
| `src/components/layout/HomeLayout.vue` | Main in-app layout wrapper for the home area; provides sidebar, header, search, and mobile navigation. |
| `src/views/home/index.vue` | Route container for the home area; wraps nested routes and music player UI. |
| `src/views/settings/index.vue` | Settings page shell; organizes settings categories and dynamically loads section components. |
| `src/components/Settings/BackgroundRenderSettings.vue` | Example of a feature-specific settings component inside the `components/Settings` group. |
| `src/store/index.ts` | Barrel export for store modules. |
| `src/services/musicSdk.ts` | Service wrapper for music-related IPC/API calls. |
| `src/api/songList.ts` | API wrapper dedicated to song list operations. |

### Top-level Frontend Directories and Purpose

| Directory | Purpose |
|---|---|
| `src/` | Main frontend source tree. |
| `src/assets/` | Static assets, including CSS, icon fonts, SVG icons, images, and videos. |
| `src/api/` | Thin API wrappers around specific app domains. |
| `src/bridge/` | IPC compatibility layer that exposes `window.api` / `window.electron` style helpers. |
| `src/components/` | Reusable Vue components, grouped further by feature/domain. |
| `src/composables/` | Reusable Vue composables and hooks. |
| `src/config/` | Runtime configuration values for the frontend. |
| `src/router/` | Router configuration and route preloading behavior. |
| `src/services/` | Service-layer wrappers around IPC or platform APIs. |
| `src/store/` | Pinia stores and the store barrel export. |
| `src/types/` | Shared TypeScript type definitions. |
| `src/utils/` | Shared utility modules, grouped by concern. |
| `src/views/` | Route-level pages and feature screens. |

### Observed Code Patterns

#### Entry and shell layout
- `src/main.ts:0-45` shows the app entry importing global styles first, then `App.vue`, router, Pinia, persisted state, bridge setup, and cross-cutting runtime helpers.
- `src/App.vue:0-43` is the root composition layer rather than a page; it chooses between the normal app shell and the desktop-lyric window, and wraps routed content with global UI elements.
- `src/components/layout/HomeLayout.vue:0-297` is the primary in-app shell used by the home screen. It handles sidebar navigation, title bar controls, search, source selection, and mobile bottom navigation.

#### Route organization
- `src/router/index.ts:19-55` groups route-level pages by feature:
  - `/home/*` children point at `src/views/music/*`, `src/views/download/index.vue`, and `src/views/user/Profile.vue`.
  - `/settings` points at `src/views/settings/index.vue`.
  - special standalone windows use `src/views/DeskTopLyric/DeskTopLyric.vue` and `src/views/music/RecognitionWorker.vue`.
- `src/views/home/index.vue:0-23` and `src/views/settings/index.vue:0-274` both act as screen-level containers that orchestrate nested content rather than implementing only a single widget.

#### Feature grouping inside folders
- `src/components/Play/`, `src/components/Settings/`, `src/components/ContextMenu/`, `src/components/BackupRestore/`, `src/components/Auth/`, and `src/components/layout/` show feature-based component grouping.
- `src/views/settings/sections/` groups section-level components used only inside the settings page.
- `src/utils/audio/`, `src/utils/playlist/`, `src/utils/plugin/`, `src/utils/color/`, `src/utils/lyrics/`, and `src/utils/hotkeys/` show domain-based utility subfolders.
- `src/assets/icon_font/`, `src/assets/icons/`, `src/assets/images/`, and `src/assets/videos/` separate assets by media type.

#### Naming and placement conventions observed
- Vue component files are usually PascalCase when they are reusable components or standalone screens, for example `HomeLayout.vue`, `PlayMusic.vue`, `BackupRestore.vue`, `AISection.vue`.
- View route files are often placed in lowercase feature folders, with several lowercase filenames that match route segments, such as `src/views/music/find.vue`, `songlist.vue`, `local.vue`, `search.vue`, and `recent.vue`.
- Composables use the `use*` prefix, such as `useBackgroundRender.ts`, `useDynamicSongTheme.ts`, and `useEventListener.ts`.
- Store files live directly under `src/store/` and often use PascalCase names (`Auth.ts`, `Settings.ts`, `GlobalPlayStatus.ts`), but some are lower camel case or lowercase (`audioOutput.ts`, `playSetting.ts`, `search.ts`, `download.ts`, `dlna.ts`, `plugin.ts`).
- Utility files are grouped by feature and often use descriptive names that combine domain and action, such as `AudioManager.ts`, `playlistExportImport.ts`, `pluginWorker.ts`, and `cors-proxy.ts`.
- Generated support files sit at the `src/` root: `auto-imports.d.ts`, `components.d.ts`, and `vite-env.d.ts`.

### Concrete Examples

| Example Path | What it shows |
|---|---|
| `src/components/layout/HomeLayout.vue` | Shared app shell under a layout-specific component folder. |
| `src/views/settings/sections/AppearanceSection.vue` | Settings-only section component nested under the page it belongs to. |
| `src/router/index.ts` | Centralized route tree and route preload logic. |
| `src/utils/audio/AudioManager.ts` | Domain-specific utility placed under a feature subfolder. |

### Recommendations for the Spec File

- Document the frontend source tree starting from `src/`, then describe each top-level folder and its role.
- Include the route hierarchy from `src/router/index.ts`, especially how `views/` maps to route-level screens.
- Call out feature-folder patterns inside `components/`, `views/`, `utils/`, and `assets/`.
- Record the naming conventions actually used in this repo, including the mix of PascalCase and lowercase filenames.
- Mention support files that shape the frontend but are not pages, such as `main.ts`, `App.vue`, `bridge/index.ts`, and generated `.d.ts` files.
- Use a few representative file examples rather than listing every file.

## Caveats / Not Found

- The existing `.trellis/spec/frontend/directory-structure.md` is still a placeholder and does not yet contain project-specific content.
- I did not find a separate `public/`-specific organization rule in the inspected files; the main observed structure is under `src/`.
- External references were not needed for this task; the findings are based on the repository itself.
