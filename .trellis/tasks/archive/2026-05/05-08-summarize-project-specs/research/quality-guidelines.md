# Research: Frontend Quality Guidelines

- **Query**: Summarize actual frontend quality standards into `.trellis/spec/frontend/quality-guidelines.md`; inspect package scripts, lint/typecheck/test config, representative error handling and async patterns, CSS/theme conventions, and forbidden/undesirable patterns visible in the repo.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Scripts / Tools Used for Quality Checks

| Source | Evidence | Notes |
|---|---|---|
| `package.json:5-10` | `"build": "vue-tsc --noEmit && vite build"` | The only explicit quality gate in npm scripts is the production build, which runs Vue type-checking before Vite bundling. |
| `package.json:6-9` | `dev`, `build`, `preview`, `tauri` | No `lint`, `format`, `test`, `typecheck`, or `check` scripts are defined separately. |
| `tsconfig.json:13-16` | `strict: true`, `noFallthroughCasesInSwitch: true`, `noUnusedLocals: false`, `noUnusedParameters: false` | TypeScript strict mode is enabled, while unused locals/parameters are tolerated. |
| `tsconfig.json:18-26` and `vite.config.ts:37-46` | aliases `@`, `@renderer`, `@assets`, `@components`, `@services`, `@types`, `@store` | Path aliases are configured in both TS and Vite. |
| `.github/workflows/build.yml:49-53`, `86-90`, `128-132` | `yarn install`, then `tauri-apps/tauri-action@v0` | CI builds Tauri artifacts; there is no separate lint/test job visible in this workflow. |
| repo config search | no root `.eslintrc*`, `eslint.config.*`, `.prettierrc*`, `vitest.config.*`, `jest.config.*`, `playwright.config.*` | Lint/format/test tooling is not configured at the active repo root. Similar configs only appear under `CeruMusic-main/`, which looks like an upstream/reference directory, not the active app root. |
| `src/**/*.test.*`, `src/**/*.spec.*` search | no matching files | No frontend test files were found under `src/`. |

### Files Found / Concrete Examples

| File Path | Description |
|---|---|
| `package.json` | Defines the active scripts and dependencies; the `build` script is the practical type-check/build quality gate. |
| `tsconfig.json` | Shows strict TS settings and path aliases used by frontend code. |
| `src/main.ts` | App bootstrap, global CSS imports, Pinia/router setup, Logto initialization, dev-only performance sampling, and a narrow Vue warning filter. |
| `src/bridge/index.ts` | Tauri/Electron compatibility adapter; wraps IPC calls with normalization, caching, performance measurement, and warning/rethrow behavior. |
| `src/services/musicSdk.ts` | Centralized music SDK request wrapper; rewrites image URLs, logs contextual failures, and rethrows for callers. |
| `src/assets/main.css` | Central style/theme token source, including motion tokens, TDesign token overrides, light/dark theme variables, mobile safe-area tokens, and reduced-motion handling. |

### Observed Standards: Imports

- Alias imports are the dominant cross-module pattern:
  - `src/App.vue:17-20` imports `@/components/...`, `@/store/...`.
  - `src/services/musicSdk.ts:1-2` imports `@/store/LocalUserDetail` and `@/utils/imageProxy`.
  - `src/composables/useDynamicSongTheme.ts:2-4` imports from `@/store/...` and `@/utils/...`.
- Type-only imports are used when the import is type-only:
  - `src/api/songList.ts:1-11` uses `import type` for API/type contracts.
  - `src/components/Settings/BackgroundRenderSettings.vue:4-5` uses `import type { BackgroundRenderPreset }` plus a value import for `BACKGROUND_PRESETS`.
  - `src/composables/useEventListener.ts:1-2` imports `UnlistenFn` as type-only.
- Relative imports still appear for local/same-area type files, but aliases are preferred for cross-directory app imports:
  - `src/components/ContextMenu/ContextMenu.vue:21` imports local `./types`.
  - `src/store/LocalUserDetail.ts:4-5` uses `../types/...`, showing existing legacy/local relative imports are present.
- `vite.config.ts:13-35` configures `unplugin-auto-import` and `unplugin-vue-components` for TDesign and Naive UI. In practice, many TDesign components are used directly in templates, while explicit imports are still used for stores, utilities, and icons.

### Observed Standards: Async Handling and Error Handling

- Async operations that touch IPC, filesystem/dialog APIs, network-like SDK calls, plugins, or browser storage are usually wrapped in `try/catch` with either user feedback, logged context, fallback return values, or cleanup in `finally`.
- UI-facing async errors commonly use `MessagePlugin` from TDesign:
  - `src/components/Settings/DirectorySettings.vue:106-119` loads directory settings, logs `加载目录配置失败`, and shows `MessagePlugin.error('加载目录配置失败')`.
  - `src/components/Settings/DirectorySettings.vue:201-214` sets `isSaving`, invokes `save_directories`, shows success/error messages, emits `directory-changed`, and resets `isSaving` in `finally`.
  - `src/utils/downloadHelper.ts:3-26` returns early for missing URLs, shows success on queued downloads, and logs/shows an error on failure.
- Service-layer wrappers either normalize errors into response objects or rethrow after adding context:
  - `src/api/songList.ts:22-35` catches Electron API failures and returns `{ success: false, error }` rather than throwing to callers.
  - `src/api/songList.ts:319-353` uses `Promise.all` for playlist metadata and songs, then returns a normalized success/error shape.
  - `src/services/musicSdk.ts:132-145` wraps Tauri `invoke`, rewrites image URLs on success, logs `[musicSdk] request('<method>') failed`, then rethrows.
- Fire-and-forget / cleanup operations sometimes intentionally swallow errors where failure is non-critical:
  - `src/composables/useEventListener.ts:12-17`, `33-36`, `39-44` catches dispose/listener registration cleanup errors and continues.
  - `src/router/index.ts:68-76`, `95-100` swallows localStorage parsing/preload errors and keeps route preloading best-effort.
  - `src/views/music/recognize.vue:415-429` stops capture and unregisters listeners defensively, swallowing non-critical stop/unlisten failures.
- Loading state cleanup is often handled with `finally`:
  - `src/components/Settings/DirectorySettings.vue:201-214` clears `isSaving` in `finally`.
  - `src/store/S3Backup.ts:80-94`, `97-129`, `132-172` sets connection/backup/restore flags before async work and resets them in `finally`.
  - `src/views/music/recognize.vue:446-499` sets processing state, closes `AudioContext` in nested `finally`, then clears `running` in outer `finally`.
- App-wide error-boundary style handling is minimal. There is no global `onErrorCaptured`/`errorCaptured` boundary found in app code. The only global Vue config hook found is `src/main.ts:34-38`, a narrow `warnHandler` that filters a known TDesign `TPopup` slot warning and forwards all other warnings to `console.warn`.

### Observed Standards: Theme / Styles

- Global CSS token layers are centralized:
  - `src/assets/main.css:1-18` defines app/mobile layout tokens such as `--play-bottom-height`, safe-area tokens, mobile nav height, and mobile touch target.
  - `src/assets/base.css:1-27` defines motion/glass/mobile layer tokens.
  - `src/assets/main.css:123-146` defines global scrollbar styling using TDesign token variables.
- TDesign and project theme variables are the primary styling surface:
  - `src/assets/main.css:29-433` defines light theme variables, including `--td-*`, `--theme-*`, page-specific variables, settings variables, plugin variables, and player variables.
  - `src/assets/main.css:435-579` starts the dark theme token block via `:root[data-theme='dark']` and redefines the corresponding tokens.
  - `src/assets/main.css:945-947` adjusts `#app` background for dark mode.
- Recent project convention strongly favors `color-mix()` plus TDesign variables instead of fixed colors for adaptive UI:
  - Recent commit evidence: `a691112 feat: 用 color-mix + TDesign 变量替代硬编码颜色，自动适配暗色模式`.
  - `src/components/layout/HomeLayout.vue:679-815` uses `color-mix(in srgb, var(--td-...), transparent)` and `:global([data-theme="dark"])` overrides for mobile bottom navigation.
  - `src/components/Playlist/AddToPlaylistDialog.vue:246-259` builds glass-panel backgrounds/borders/shadows with `color-mix()` and `var(--td-...)` tokens.
- Motion/accessibility conventions are visible:
  - `src/App.vue:51-83` uses motion tokens for route transitions and switches to transform-free reduced motion under `@media (prefers-reduced-motion: reduce)`.
  - `src/assets/main.css:949-958` globally reduces animation/transition duration under `prefers-reduced-motion`.
  - `src/components/Find/PlaylistCategory.vue:325-329` uses `role="status"` and `aria-live="polite"` for loading status.
  - `src/views/music/recognize.vue:586-595` uses `aria-label` and `aria-busy` on the recognition center button.
- Responsive/mobile conventions use safe-area and touch-target tokens:
  - `src/assets/main.css:151-259` applies mobile dialog/drawer z-index, safe-area padding, glass styling, and mobile overlay behavior.
  - `src/components/layout/HomeLayout.vue:681-724` positions mobile bottom navigation with safe-area variables and sets min touch target sizing.
- Scoped component styles are common, with both plain CSS and SCSS:
  - `src/components/Settings/DirectorySettings.vue:259-338` uses `<style lang="scss" scoped>` and TDesign token colors.
  - `src/components/Find/PlaylistCategory.vue:384-1197` uses scoped CSS with token-based colors and custom responsive rules.

### Observed Standards: Performance / Async UI

- Expensive or repeated renderer work is guarded by performance controls:
  - `src/types/background.ts:40-76` defines `performance` and `quality` presets plus defaults for render scale, FPS, static mode, and audio response.
  - `src/components/Play/FullPlay.vue:226-323` creates a `PerformanceDegrader` that lowers render scale/FPS and disables audio response when low FPS is detected.
  - `src/utils/performanceMonitor.ts:4-69` tracks FPS and triggers degradation after sustained low FPS.
  - `src/utils/performanceMonitor.ts:154-235` records IPC/render/FPS/memory telemetry; `src/main.ts:28-30` starts memory sampling only in dev.
- Route preloading is idle/best-effort and user-configurable:
  - `src/router/index.ts:68-107` reads `routePreloadEnabled`, schedules lazy route preload via `requestIdleCallback` or timeout fallback, and swallows preload errors.
- Lists and images use performance-aware patterns:
  - `src/components/Find/PlaylistCategory.vue:644-645` uses `content-visibility: auto` and `contain-intrinsic-size` for playlist cards.
  - `src/components/Find/PlaylistCategory.vue:355` and `src/views/music/local.vue:426-432` use `loading="lazy"` for images.

### Forbidden / Undesirable Patterns Inferred Where Evidence Exists

These are inferred from visible repo conventions and recent commits, not from an existing filled spec.

| Pattern | Evidence | Spec implication |
|---|---|---|
| Adding app-wide/global animated background behavior without scoping/performance controls | Recent commit `1d87ae0` says global background was removed; current code scopes `backgroundRender` to `fullPlay` only in `src/types/background.ts:32-35`, `src/store/Settings.ts:30-49`, and `src/components/Play/FullPlay.vue:223-288`. | If documenting as forbidden, phrase as: background effects must be scoped to the feature surface and guarded by enable/FPS/render-scale/static-mode controls; avoid unbounded global background renderers. |
| Hard-coded colors for normal themeable UI | Recent commit `a691112` explicitly replaced hard-coded colors with `color-mix + TDesign variables`; active styles use `var(--td-*)`, `var(--theme-*)`, and `color-mix()` in files such as `src/components/layout/HomeLayout.vue:692-815` and `src/components/Playlist/AddToPlaylistDialog.vue:246-259`. | Prefer tokens and `color-mix()` for adaptive light/dark UI. Fixed colors should be limited to semantic platform colors, media-derived colors, SVG/icon assets, or special visual effects where tokenization is not practical. |
| Using TDesign `t-loading` as the only loading visual where custom CSS spinner is already the local convention | Recent commit `015829b` says custom CSS rotation replaced TDesign Loading; current custom spinner examples exist in `src/components/Find/PlaylistCategory.vue:325-329`, `514-522`, `1191-1197`, and `src/components/Play/PlayMusic.vue:926-933`. However `t-loading` still appears in some components (`src/views/music/recognize.vue:599`, `src/components/Settings/PluginSettings.vue:17`, etc.). | Document cautiously: for app/player/list surfaces that need consistent lightweight loading states, prefer local CSS spinner/status markup; do not state a blanket ban because `t-loading` remains present. |
| Unhandled async calls for user-visible work | Representative patterns show try/catch plus `MessagePlugin`, response normalization, or logged context (`src/components/Settings/DirectorySettings.vue:106-119`, `201-214`; `src/api/songList.ts:22-35`; `src/services/musicSdk.ts:132-145`). | User-visible async actions should catch errors and either show feedback, return a normalized failure object, or rethrow with contextual logging at service boundaries. |
| Leaving timers/listeners/render loops uncleared | Lifecycle utilities and components consistently clean up listeners/timers/RAF (`src/composables/useEventListener.ts:9-72`; `src/views/music/recognize.vue:415-429`, `562-564`; `src/components/Play/FullPlay.vue:449-458`). | Document cleanup as required for `listen`, DOM events, intervals/timeouts, `requestAnimationFrame`, audio contexts, and background renderers. |
| Unqualified `console.log` in production-facing code | `src/utils/logger.ts:9-18` provides namespaced logger with dev-only `debug`; grep still finds raw `console.log` in places such as `src/components/Play/FullPlay.vue:364-389`, `src/components/Play/PlayMusic.vue:807`, `src/components/Settings/PlaylistSettings.vue:91`. | Treat raw debug `console.log` as undesirable for new code; prefer contextual `console.warn/error` for failures or `createLogger(...).debug` for dev-only diagnostics. |

### Related Specs

| Spec Path | Status / Relevance |
|---|---|
| `.trellis/spec/frontend/quality-guidelines.md` | Existing target file is a placeholder with sections for overview, forbidden patterns, required patterns, testing requirements, and code review checklist. |
| `.trellis/spec/frontend/index.md` | Says guideline files should document actual conventions, include codebase examples, list forbidden patterns, and be written in English. |
| `.trellis/spec/frontend/type-safety.md` | Placeholder; relevant because quality guidelines should reference strict TypeScript/build gate but leave deeper type rules to this file. |
| `.trellis/spec/frontend/component-guidelines.md` | Placeholder; relevant for style/component examples but detailed component structure belongs there. |
| `.trellis/spec/frontend/hook-guidelines.md` | Placeholder; lifecycle cleanup and async composables may be cross-referenced. |
| `.trellis/spec/frontend/state-management.md` | Placeholder; Pinia/loading/error-state conventions may be cross-referenced. |

### Recommendations for `.trellis/spec/frontend/quality-guidelines.md`

The spec file should likely include these sections and points, written as project rules with examples:

1. **Quality gate / scripts**
   - State that the current quality gate is `npm run build` / `yarn build`, which runs `vue-tsc --noEmit && vite build`.
   - State that no root lint/test/format scripts are currently configured; do not invent lint/test requirements.
   - Mention CI builds Tauri artifacts via GitHub Actions but does not run a separate frontend lint/test job.

2. **TypeScript and imports**
   - Require cross-directory imports to use configured aliases (`@/...`, `@store`, `@types`, etc.) when practical.
   - Require `import type` for type-only imports.
   - Note that strict TypeScript is enabled, while unused locals/parameters are not compiler failures.

3. **Async/error handling**
   - User-facing async actions should wrap IPC/network/storage/dialog operations in `try/catch` and use `MessagePlugin` or component state for visible feedback.
   - Service/API wrappers should either return normalized `{ success, error }` shapes or log contextual information and rethrow.
   - Use `finally` for loading/saving flags and resource cleanup.
   - Fire-and-forget cleanup may swallow errors only when failure is non-critical.

4. **Lifecycle cleanup**
   - Any DOM/Tauri/Electron listeners, timers, RAF loops, AudioContext instances, and background renderers must be cleaned up on unmount or stop paths.
   - Reference `useLifecycle` as the reusable pattern for component-level registrations.

5. **Theme/styles**
   - Prefer `var(--td-*)`, `var(--theme-*)`, component/page tokens, and `color-mix()` for themeable UI.
   - Avoid new hard-coded colors in normal app UI unless they are platform-specific controls, media-derived colors, semantic one-off effects, or token definitions themselves.
   - Keep dark-mode behavior token-driven via `:root[data-theme='dark']` / `:global([data-theme="dark"])` patterns.
   - Respect `prefers-reduced-motion` for new animations/transitions.
   - Preserve mobile safe-area and touch-target tokens for mobile layouts.

6. **Performance-sensitive UI**
   - Gate expensive rendering behind settings/visibility checks.
   - Include FPS/render-scale/static-mode style controls for dynamic background/effect code.
   - Prefer idle/best-effort preloading and lazy images/virtualized or contained list rendering for large lists.

7. **Forbidden / undesirable patterns**
   - Do not add unscoped global visual renderers/backgrounds.
   - Do not add hard-coded colors for themeable UI when TDesign/project tokens are available.
   - Do not leave user-visible async work without error handling/user feedback.
   - Do not leave listeners/timers/RAF/audio/render resources uncleared.
   - Avoid raw debug `console.log` in new production-facing code; use contextual warnings/errors or the namespaced logger for diagnostics.
   - Be cautious with a blanket `t-loading` ban: evidence shows a recent preference for CSS spinners, but `t-loading` still exists in the codebase.

## Caveats / Not Found

- No external research was needed; findings are internal codebase observations.
- No root ESLint, Prettier, Vitest, Jest, or Playwright config was found in the active repository root. Configs under `CeruMusic-main/` were not treated as active project standards.
- No frontend test files matching `src/**/*.test.*` or `src/**/*.spec.*` were found.
- No global Vue error boundary / `onErrorCaptured` app-level pattern was found; only a targeted `app.config.warnHandler` exists in `src/main.ts`.
- `console.log` and `t-loading` both still exist in current source, so any spec language should call them “undesirable/prefer alternatives” only where supported by project convention, not absolute repo-wide bans unless the team explicitly decides so.
