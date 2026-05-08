# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

The active frontend quality gate is the production build: `npm run build` / `yarn build`, which runs `vue-tsc --noEmit && vite build`. There are no root lint, format, or test scripts currently configured in the active repo, and no frontend test files were found under `src/`.

Do not invent tooling requirements for a task. For docs-only changes, a git diff / placeholder check is usually sufficient. For app source changes, run the build unless the task or environment makes it impractical, and report the result.

---

## Required Patterns

### TypeScript and imports

- Use TypeScript in Vue SFCs (`<script setup lang="ts">`) and `.ts` modules.
- Use `import type` for type-only imports.
- Prefer configured aliases for cross-directory app imports (`@/...`, `@store`, `@types`, etc.) when practical.
- Local sibling imports such as `./types` are fine.
- Strict TypeScript is enabled; unused locals/parameters are currently tolerated by `tsconfig` and should not be treated as a configured failure by themselves.

### Async and error handling

- Wrap user-visible IPC, filesystem/dialog, storage, and network-like SDK operations in `try/catch`.
- Use `MessagePlugin` or component state for user-facing success/error feedback where appropriate.
- Service/API wrappers should either return normalized `{ success, data, error }`-style results or log contextual information and rethrow.
- Use `finally` to reset loading/saving flags and release resources.
- Fire-and-forget cleanup may swallow errors only when failure is non-critical and the code remains safe.

### Lifecycle cleanup

Clean up every external resource that a component/composable creates:

- DOM/window/document listeners.
- Tauri `listen` / Electron-like IPC subscriptions.
- Intervals, timeouts, `requestAnimationFrame` loops.
- Audio contexts/capture resources.
- Background renderers, canvases, and visualization loops.

Use `src/composables/useEventListener.ts` as the reusable cleanup pattern when appropriate.

### Theme and styles

- Prefer TDesign/project CSS tokens (`--td-*`, `--theme-*`, page/component tokens) and `color-mix()` for themeable UI.
- Keep light/dark behavior token-driven via global `:root[data-theme='dark']` and local `:global([data-theme="dark"])` patterns.
- Avoid new fixed colors for normal app UI unless they are semantic platform colors, media-derived colors, SVG/icon assets, special effects, or token definitions themselves.
- Respect `prefers-reduced-motion` for new animations/transitions.
- Preserve mobile safe-area and touch-target variables for mobile layouts.

### Performance-sensitive UI

- Gate expensive rendering behind feature settings, visibility checks, or enabled flags.
- For visual effects/background renderers, keep FPS/render-scale/static-mode/degradation controls where applicable.
- Prefer idle/best-effort preloading, lazy images, virtualized lists, or CSS containment for large lists and expensive screens.

---

## Forbidden and Undesirable Patterns

Hard requirements for new code:

- Do not leave user-visible async work without error handling or feedback.
- Do not leave listeners, timers, RAF loops, audio resources, or render resources uncleared.
- Do not add unscoped global visual renderers/background effects.
- Do not add hard-coded colors for themeable UI when TDesign/project tokens are available.

Undesirable for new production-facing code, but still present in legacy areas:

- Raw debug `console.log`; prefer contextual `console.warn/error` for failures or the namespaced logger for dev diagnostics.
- Blanket reliance on `t-loading` for app/player/list loading visuals where local CSS spinner/status markup is already the local convention. `t-loading` still exists in the repo, so do not treat this as a global ban.
- Broad `any` at internal call sites when a local interface or shared type is practical. Boundary/plugin payloads are more flexible; see `type-safety.md`.

---

## Testing Requirements

Current configured checks:

- Build/type check: `npm run build` or `yarn build` (`vue-tsc --noEmit && vite build`).
- No configured root lint/test/format scripts.
- CI builds Tauri artifacts but does not show a separate frontend lint/test job.

For docs-only spec changes, verify by reviewing the diff and checking the updated files are no longer placeholders. Do not add test or lint tooling as part of ordinary feature work unless explicitly requested.

---

## Code Review Checklist

- Scope: app behavior changed only if the task required it; docs-only tasks do not modify `src/` runtime behavior.
- Types: type-only imports use `import type`; shared/local types are placed according to `type-safety.md`.
- Async: user-visible operations handle errors and reset loading state in `finally`.
- Cleanup: all external listeners/timers/render/audio resources are disposed.
- State: local vs Pinia state follows `state-management.md`; no unnecessary global state/query library added.
- Styling: themeable UI uses tokens/`color-mix()` and remains dark-mode/mobile/reduced-motion aware.
- Performance: large lists/effects are gated, lazy, virtualized, contained, or degraded as appropriate.
- Diagnostics: no new unqualified debug `console.log` in production-facing paths.
