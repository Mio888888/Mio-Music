# Directory Structure

> How frontend code is organized in this project.

---

## Overview

This is a Vue 3 / TypeScript frontend. Most application code lives under `src/`; `.trellis/spec/frontend/` documents conventions only and must not be treated as runtime code.

Use the existing feature-oriented layout. Do not reorganize the source tree for a small feature unless the task explicitly requires it.

---

## Directory Layout

```text
src/
├── main.ts                 # App entry; imports global CSS, installs router/Pinia, initializes bridge/runtime helpers
├── App.vue                 # Root shell with global providers, background/context menu, and top-level router view
├── assets/                 # Global CSS plus icon fonts, SVG icons, images, videos
├── api/                    # Thin API wrappers around app domains, often normalizing bridge responses
├── bridge/                 # Tauri/Electron-compatible IPC bridge exposed on window.api/window.electron
├── components/             # Reusable and feature-grouped Vue SFCs
├── composables/            # Shared Vue composables named useXxx
├── config/                 # Runtime/frontend configuration values
├── router/                 # Central route map and route preload behavior
├── services/               # Service facades over IPC/platform APIs, e.g. musicSdk
├── store/                  # Pinia stores and store barrel exports
├── types/                  # Shared TypeScript contracts and global declarations
├── utils/                  # Shared utilities grouped by domain
└── views/                  # Route-level pages and standalone windows
```

Important support files:

- `src/main.ts` bootstraps global styles, router, Pinia persisted state, bridge setup, and runtime helpers.
- `src/App.vue` is a root composition layer, not a feature page.
- `src/router/index.ts` is the centralized route tree.
- Generated typing support files (`src/auto-imports.d.ts`, `src/components.d.ts`, `src/vite-env.d.ts`) stay at `src/` root.

---

## Module Organization

- Put route-level screens under `src/views/<feature>/`. The router maps directly to these files, e.g. `/settings` uses `src/views/settings/index.vue`; `/home/*` children use `src/views/music/*`, `src/views/download/index.vue`, and `src/views/user/Profile.vue`.
- Put reusable or cross-page UI under `src/components/`. Group by feature/domain when there is an established folder, e.g. `components/Play/`, `components/Settings/`, `components/ContextMenu/`, `components/BackupRestore/`, `components/Auth/`, `components/layout/`.
- Put settings-only section components under `src/views/settings/sections/` rather than promoting them to global components.
- Put shared reactive logic under `src/composables/` only when it is reused or owns a lifecycle/resource concern.
- Put domain utilities in subfolders under `src/utils/`, e.g. `utils/audio/`, `utils/playlist/`, `utils/plugin/`, `utils/color/`, `utils/lyrics/`, `utils/hotkeys/`.
- Put typed IPC/API/service contracts in `src/types/` when shared; keep service-specific DTOs next to the service when they are not reused.
- Keep platform/IPC compatibility behavior in `src/bridge/` or service/API wrappers; do not spread raw bridge adaptation across unrelated components.

---

## Naming Conventions

- Reusable Vue components and standalone screens are usually PascalCase: `HomeLayout.vue`, `PlayMusic.vue`, `BackupRestore.vue`, `AISection.vue`.
- Route files may be lowercase when they match route segments: `views/music/find.vue`, `songlist.vue`, `local.vue`, `search.vue`, `recent.vue`.
- Composables use the `use*` prefix: `useBackgroundRender.ts`, `useDynamicSongTheme.ts`, `useEventListener.ts`.
- Store files are mixed in the current repo. Prefer the exported factory naming pattern `useXStore` for new stores, while preserving legacy names such as `LocalUserDetailStore`, `playSetting`, or `searchValue` when editing existing stores.
- Use the `@/` alias for cross-directory app imports when practical. Local sibling imports such as `./types` are fine.

---

## Examples

- `src/components/layout/HomeLayout.vue` — primary in-app shell with sidebar, header/search/source controls, and mobile navigation.
- `src/views/settings/index.vue` and `src/views/settings/sections/AppearanceSection.vue` — page shell plus settings-only section components.
- `src/router/index.ts` — route hierarchy for welcome/home/settings and standalone windows such as desktop lyrics and recognition worker.
- `src/services/musicSdk.ts` — service wrapper around music-source IPC/API calls.
- `src/api/songList.ts` — domain API wrapper with typed response normalization.
- `src/utils/audio/AudioManager.ts` — domain utility in a feature subfolder.
