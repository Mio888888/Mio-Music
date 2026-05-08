# Type Safety

> Type safety patterns in this project.

---

## Overview

The frontend uses TypeScript throughout `.ts` modules and Vue SFCs (`<script setup lang="ts">`). `tsconfig.json` enables strict mode. Types are pragmatic: core contracts are typed, while plugin/backend boundaries still allow flexible payloads with `any`, index signatures, and assertions where necessary.

Prefer improving local type clarity for new code without forcing broad refactors of legacy dynamic boundaries.

---

## Type Organization

- Shared domain/API contracts live in `src/types/`, e.g. `audio.ts`, `songList.ts`, `userInfo.ts`, `hotkeys.ts`, `background.ts`, and `window.d.ts`.
- Service-specific DTOs can live beside the service when not reused elsewhere, e.g. music/search/playlist/singer result interfaces in `src/services/musicSdk.ts`.
- Store-specific state/config interfaces can live in the store file, e.g. `SettingsState` in `src/store/Settings.ts`, download task types in `src/store/download.ts`, plugin types in `src/store/plugin.ts`.
- Component-specific props, emits, sort unions, and local UI types can live inside the SFC.
- Global bridge declarations live in `src/types/window.d.ts`.

---

## Common Patterns

- Use interfaces for object models, DTOs, options objects, and store state.
- Use type aliases for string-literal unions, function signatures, utility-shaped types, and re-exported external types.
- Use finite unions/enums for bounded states:
  - `HotkeyAction` in `src/types/hotkeys.ts`.
  - `BackgroundRenderPreset` in `src/types/background.ts`.
  - `PlayMode` in `src/types/audio.ts`.
  - `DownloadStatus` in `src/store/download.ts`.
- Use generic response wrappers for IPC/API boundaries, e.g. `IPCResponse<T>` and `SongListAPI` in `src/types/songList.ts`.
- Utility types such as `Partial`, `Omit`, `Record`, `Exclude`, and `readonly` arrays are common and preferred over hand-written duplicates.
- Use `import type` for type-only imports.

---

## Boundary Typing and Transformations

- Frontend services wrap IPC/Tauri calls and expose typed methods where possible.
- Normalize raw backend/plugin responses at boundaries with object mapping, URL rewriting, default values, array checks, and copied response objects.
- For localStorage/JSON state, parse in `try/catch` or guarded code, assert to the expected shape when needed, then merge with current defaults to repair missing fields.
- Keep DTO transformations plain: object spreading and field mapping are preferred over classes.
- When adapting legacy song/plugin shapes, local casts are acceptable at the boundary, but keep the receiving API typed.

Examples:

- `src/types/songList.ts` — `IPCResponse<T>`, playlist result DTOs, and `SongListAPI` method contracts.
- `src/services/musicSdk.ts` — service-local music/search/playlist DTOs and response normalization.
- `src/types/background.ts` — literal union preset types and typed preset records.
- `src/components/Music/SongVirtualList.vue` — component-local prop and sort types.

---

## Validation

No schema validation library is used in the active frontend code. Runtime validation is ad hoc and should stay consistent unless a task explicitly introduces a validator.

Use existing validation techniques:

- `Array.isArray` before assigning unknown arrays.
- `typeof` checks for primitive conversion.
- `Number.isFinite` for numeric parsing.
- Optional chaining and default fallbacks for partial backend/plugin payloads.
- `try/catch` around JSON parsing and non-critical cleanup.
- Defensive fallback objects when persisted or backend data is malformed.

---

## Flexible Boundary Payloads

Some boundaries intentionally remain flexible:

- Plugin metadata and music-source payloads may use `any`, `Record<string, any>`, or index signatures.
- `window.api` dynamic IPC arguments/results are globally typed but still broad.
- Tauri/listener payloads may be `any` when event schemas are dynamic or not centralized.

Contain this flexibility at the boundary. Inside new feature logic, prefer narrower local interfaces or typed mapping results.

---

## Forbidden Patterns

- Do not add broad `any` to internal feature logic when a simple local type would work.
- Do not trust parsed JSON/backend/plugin data without defaults or guards at the boundary.
- Do not duplicate shared API/domain contracts in multiple files; put reused contracts in `src/types/` or import the existing type.
- Do not replace existing typed service/API wrappers with scattered raw `(window as any)` calls unless the surrounding code already uses that bridge pattern and no wrapper exists.
- Do not convert pragmatic boundary assertions into large refactors unless the task explicitly requires it.
