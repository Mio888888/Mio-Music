# Hook Guidelines

> How composables are used in this project.

---

## Overview

This project uses Vue 3 Composition API composables, not React hooks. Shared composables live in `src/composables/` and are named `useXxx`.

Composables should encapsulate reusable stateful logic, lifecycle cleanup, or external resources. Do not extract one-off component code into a composable only for abstraction.

---

## Custom Hook Patterns

Use these patterns for new composables:

- Name files and functions with the `use*` prefix, e.g. `useEventListener`, `useBackgroundRender`, `useDynamicSongTheme`.
- Define a local options interface when accepting structured input.
- Use `Ref<T>` options when callers pass reactive inputs that the composable should watch.
- Keep internal state in `ref`, `computed`, or private module/local variables as appropriate.
- Return a small explicit public API: state refs plus methods such as `init`, `pause`, `resume`, `dispose`.
- It is acceptable to return nothing when the composable only binds a global side effect to the component lifecycle.
- Store Tauri `UnlistenFn` handles and call them during cleanup.
- Provide an explicit `dispose` method when the composable owns external resources such as renderers, canvases, timers, or listeners.

---

## Cleanup and Watchers

- Register DOM listeners, Tauri listeners, Electron-like IPC listeners, intervals, timeouts, and arbitrary cleanup callbacks with a cleanup path.
- Prefer `onUnmounted` / `onBeforeUnmount` cleanup. Use visibility watchers when a resource should only exist while visible/enabled.
- Use `watch` for reactive inputs and side effects. `immediate: true` is common for initial apply/fetch behavior; `deep: true` is used for nested settings/config objects where required.
- When async work can be invalidated by a later reactive change, use watcher cleanup and cancellation patterns where practical.

Representative examples:

- `src/composables/useEventListener.ts` — lifecycle registration helpers plus defensive disposal.
- `src/composables/useBackgroundRender.ts` — AMLL background renderer initialization, watched config, Tauri event listener, canvas removal, and explicit `dispose`.
- `src/composables/useDynamicSongTheme.ts` — applies CSS theme variables from playback store state and clears them before unmount.

---

## Data Fetching

There is no TanStack Query/SWR-style query cache in this project. Data fetching is performed directly in services, stores, and components.

Use the existing shape:

- Call typed service/API wrappers such as `src/services/musicSdk.ts` or domain APIs under `src/api/` when available.
- Use `window.api` / Tauri bridge calls directly only when no wrapper exists or when editing existing bridge-oriented code.
- Keep local `loading`, `page`, `total`, `hasMore`, and result refs in the component when the data is page-local.
- Guard duplicate loads and invalid inputs before requesting.
- Use `try/catch/finally`; set loading flags before the request and reset them in `finally`.
- Normalize DTOs at service/component/store boundaries with object mapping, URL rewriting, defaults, and array checks.
- Manual caches are local and keyed by source/tag/route when needed; invalidate on selected source or relevant input changes.
- For backend event streams, store the synchronized state in Pinia when it is shared across components.

Examples:

- `src/views/music/search.vue` — local refs for search results/loading/pagination/tabs, watchers that trigger fetches, and direct `musicSdk` calls.
- `src/components/Find/PlaylistCategory.vue` — module-local `Record` cache keyed by tag/source and invalidated when the selected source changes.
- `src/store/GlobalPlayStatus.ts` — watcher cleanup and `AbortController` cancellation for long-running lyric/comment-related fetches.

---

## Naming Conventions

- Shared composable files live in `src/composables/`.
- File and exported function names should start with `use`.
- Keep composable names domain-specific and action-oriented: `useBackgroundRender`, `useDynamicSongTheme`, `useEventListener`.
- If a helper has no Vue lifecycle/reactivity concern, put it under `src/utils/` instead of `src/composables/`.

---

## Common Mistakes

- Forgetting to clean up listeners, intervals/timeouts, canvas/renderers, or IPC/Tauri subscriptions.
- Hiding page-specific fetch state in a composable when simple component-local refs are clearer.
- Adding a query library or global cache for one feature without an explicit task requirement.
- Returning a large implicit API from a composable instead of a small explicit set of refs/methods.
