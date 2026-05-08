# State Management

> How state is managed in this project.

---

## Overview

Global state uses Pinia 3. `src/main.ts` creates Pinia, installs `pinia-plugin-persistedstate`, and registers it with the Vue app. Both setup-style and option-style stores exist; prefer matching the style of the store you are editing.

Local component state is still the default for transient UI concerns. Promote state to Pinia only when it is shared, persisted, backend/event-synchronized, or part of global playback/control flows.

---

## State Categories

### Component-local state

Use component-local `ref`/`computed` for:

- Tabs, forms, search query, modal/drawer visibility.
- Loading/saving flags and one-page pagination.
- Scroll position, hover state, selection sets, sort state.
- Page-local fetched lists that do not need cross-page reuse.

Examples: `src/views/music/search.vue`, `src/components/Music/SongVirtualList.vue`, `src/components/Find/PlaylistCategory.vue`.

### Global app/user/playback state

Use Pinia for state shared across pages/components or needed by global flows:

- User/source details and local playlists.
- App settings and feature toggles.
- Current playback/player state and lyrics/comments.
- Auth, plugins, equalizer/audio effects, audio output.

Examples: `src/store/Settings.ts`, `src/store/LocalUserDetail.ts`, `src/store/GlobalPlayStatus.ts`, `src/store/ControlAudio.ts`.

### Backend/event-synchronized state

Use stores when frontend state mirrors backend commands or events:

- Download tasks synchronized through backend events (`src/store/download.ts`).
- Rust player state mirrored into frontend audio state (`src/store/ControlAudio.ts`).
- SQLite-backed playlist CRUD via `window.api.songList` (`src/store/LocalUserDetail.ts`).

### Manual server state

There is no central query-cache library. Components/stores fetch via `musicSdk`, `src/api/*`, or `window.api`, then manage loading flags, manual caches, and refresh/invalidation locally.

---

## Store Patterns

- Export store factories with `useXStore` when adding new stores where possible. Existing legacy names (`LocalUserDetailStore`, `ControlAudioStore`, `playSetting`, `searchValue`) should be preserved unless refactoring is required.
- Setup-style stores use `ref`, `reactive`, `computed`, watchers, and methods returned from `defineStore`.
- Option-style stores are still present and use `state`, `getters`, and `actions`.
- Use `storeToRefs` when destructuring reactive state in components/composables.
- Keep actions responsible for backend calls or multi-step state transitions when the behavior belongs to the shared state domain.

---

## Persistence

Two persistence styles are used:

1. `pinia-plugin-persistedstate` with `persist: true` or path-limited persistence for simple store state.
2. Manual `localStorage` persistence for nested, legacy, per-key, or partially persisted state.

Manual persistence commonly parses saved JSON, merges with typed defaults, repairs missing nested defaults, and saves through explicit methods or debounced/deep watchers. Examples include `src/store/Settings.ts`, `src/store/LocalUserDetail.ts`, `src/store/search.ts`, `src/store/plugin.ts`, and `src/store/S3Backup.ts`.

---

## Server State

- Prefer existing service/API wrappers for backend calls.
- Store backend/server state globally only when it is reused across components, synchronized by backend events, or needed by global playback/control flows.
- For page-specific music lists and playlist categories, keep data local and use manual keyed caches when useful.
- Invalidate caches on source changes and other domain-specific input changes.
- Use `try/catch/finally` and explicit loading/error state for user-visible async work.

---

## Common Mistakes

- Moving temporary UI state into Pinia when local refs are enough.
- Destructuring Pinia state without `storeToRefs`, losing reactivity.
- Persisting complex nested state without merging saved data with current defaults.
- Treating manual server caches as global truth without invalidating on source/route/input changes.
- Adding a new state/query library without an explicit requirement.
