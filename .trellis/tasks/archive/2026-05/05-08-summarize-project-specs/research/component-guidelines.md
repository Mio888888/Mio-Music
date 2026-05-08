# Research: frontend component guidelines

- **Query**: summarize the existing project's actual frontend component patterns into `.trellis/spec/frontend/component-guidelines.md`; inspect representative Vue/component files and styles; include component authoring style, props/emits/composition patterns, styling conventions, 2-4 concrete examples, and spec recommendations.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `src/components/SettingsSearch.vue` | Small reusable Vue SFC using `<script setup lang="ts">`, typed props/emits, local refs/computed, TDesign input, scoped CSS and `:deep()` overrides. |
| `src/components/TitleBarControls.vue` | Window-control component using an `interface Props` plus `withDefaults(defineProps<Props>(), ...)`, router/store composition, slots, Tauri dynamic imports, and scoped SCSS. |
| `src/components/Music/SongVirtualList.vue` | Larger data/list component using typed props with defaults, event emits, TanStack virtualizer, Pinia store access, lifecycle cleanup, `defineExpose`, Teleport context menu, and scoped style block. |
| `src/components/Play/PlaylistDrawer.vue` | Drawer-style playback component using typed tuple emits, `withDefaults`, Pinia `storeToRefs`, VueUse virtual list, watchers, manual document listeners, transitions, `defineExpose`, and glass/TDesign-token styling. |
| `src/components/Settings/BackgroundRenderSettings.vue` | Settings panel pattern: no parent props, derives data from Pinia store, computed options/config, mutation helper functions, TDesign controls, scoped SCSS with settings CSS variables. |
| `src/components/ContextMenu/ContextMenu.vue` | Overlay component using Teleport, typed external props from `./types`, `v-model`-style `update:visible` emit, computed inline `CSSProperties`, global listener cleanup, and mobile-specific responsive CSS. |
| `src/components/layout/Provider.vue` | App-level provider composition around Naive UI providers, slots, global components, and dynamic theme override computed from CSS variables. |
| `src/assets/base.css` | Global reset/layout/mobile/dialog/scrollbar styles and root sizing tokens. |
| `src/assets/main.css` | Global theme token definitions, including motion, glass, mobile, TDesign-compatible color variables, and feature-specific CSS custom properties. |
| `.trellis/spec/frontend/component-guidelines.md` | Existing target spec file is currently a placeholder. |

### Code Patterns

#### Component authoring style used

- Components are overwhelmingly Vue single-file components with Composition API and TypeScript: `<script setup lang="ts">` appears in representative components such as `src/components/SettingsSearch.vue:1`, `src/components/TitleBarControls.vue:1`, `src/components/Music/SongVirtualList.vue:1`, `src/components/Play/PlaylistDrawer.vue:1`, and `src/components/Settings/BackgroundRenderSettings.vue:1`.
- Both block orders are present:
  - script-first: `SettingsSearch.vue` starts script at `src/components/SettingsSearch.vue:1` and template at `src/components/SettingsSearch.vue:103`.
  - template-first: `ContextMenu.vue` has template first at `src/components/ContextMenu/ContextMenu.vue:0` and script at `src/components/ContextMenu/ContextMenu.vue:18`; `Provider.vue` has template first at `src/components/layout/Provider.vue:0` and script at `src/components/layout/Provider.vue:10`.
- Vue Composition API primitives (`ref`, `computed`, `watch`, lifecycle hooks) are imported explicitly from `vue` in representative files, e.g. `SettingsSearch.vue:1`, `SongVirtualList.vue:1`, `PlaylistDrawer.vue:1`, and `Provider.vue:11`.
- Project imports use the `@/` alias for app modules, e.g. `@/views/settings/searchIndex` in `src/components/SettingsSearch.vue:2`, `@/store/LocalUserDetail` in `src/components/Music/SongVirtualList.vue:14`, and `@/store/Settings` in `src/components/Settings/BackgroundRenderSettings.vue:2`.
- UI components are primarily TDesign Vue Next in templates (`<t-input>`, `<t-button>`, `<t-dialog>`, `<t-switch>`, `<t-slider>`), while app-level provider code also uses Naive UI providers. Examples:
  - `src/components/SettingsSearch.vue:105` uses `<t-input>`.
  - `src/components/TitleBarControls.vue:99` and `src/components/TitleBarControls.vue:182` use `<t-button>` and `<t-dialog>`.
  - `src/components/Settings/BackgroundRenderSettings.vue:82`, `src/components/Settings/BackgroundRenderSettings.vue:96`, and `src/components/Settings/BackgroundRenderSettings.vue:126` use TDesign form controls.
  - `src/components/layout/Provider.vue:1` wraps content in `<n-config-provider>`, `<n-message-provider>`, and `<n-dialog-provider>`.
- Icons are mixed between TDesign icon components and an iconfont class convention:
  - TDesign icon imports in `src/components/Music/SongVirtualList.vue:3`-`11` and `src/components/Play/PlaylistDrawer.vue:5`.
  - `iconfont` classes in `src/components/TitleBarControls.vue:108`, `src/components/Music/SongVirtualList.vue:533`, and `src/components/Play/PlaylistDrawer.vue:329`.
- Larger interaction-heavy components keep helper functions and state in the same SFC rather than moving all behavior into composables. Example: `src/components/Music/SongVirtualList.vue:87`-`337` contains sorting, virtual scrolling, multi-select, cache, favorite, click, context-menu, formatting, expose, and lifecycle sections.
- Components that expose imperative methods use `defineExpose`, e.g. `src/components/Play/PlaylistDrawer.vue:312` exposes `scrollToCurrentSong`, and `src/components/Music/SongVirtualList.vue:337` exposes `scrollToSong`, `sortedSongs`, `sortType`, and `resetSort`.
- Portal/overlay patterns use Vue `Teleport` to `body`, e.g. `src/components/Music/SongVirtualList.vue:641` for a song context menu and `src/components/ContextMenu/ContextMenu.vue:1` for the generic context menu.
- Transitions use Vue `<transition>` / `<Transition>` plus named CSS classes. Examples: `src/components/Play/PlaylistDrawer.vue:316` and `src/components/Music/SongVirtualList.vue:365`.

#### Props, emits, and composition patterns observed

- Simple props often use inline generic object types:
  - `src/components/SettingsSearch.vue:4` defines `hiddenCategories?: string[]`.
  - `src/components/BackupRestore/S3ConfigDialog.vue:305` defines `{ visible: boolean }`.
- More complex props use a local `interface Props` plus `withDefaults(defineProps<Props>(), defaults)`:
  - `src/components/TitleBarControls.vue:7`-`23` defines optional props and defaults.
  - `src/components/Music/SongVirtualList.vue:18`-`43` defines list display options and defaults.
  - `src/components/Play/PlaylistDrawer.vue:9`-`17` defines drawer props and defaults.
  - `src/components/Play/FullPlay.vue:75`-`90` defines full player props and defaults.
- Emit typing appears in three styles:
  - tuple-style typed object: `src/components/SettingsSearch.vue:8`-`10` has `select: [item: SearchItem]`; `src/components/Play/PlaylistDrawer.vue:19`-`22` has `close: []` and `playSong: [song: SongList]`.
  - call-signature style: `src/components/BackupRestore/S3ConfigDialog.vue:306` uses `(e: 'update:visible', val: boolean): void`.
  - untyped string-array style in some larger/older components: `src/components/Music/SongVirtualList.vue:45`-`57` and `src/components/Play/FullPlay.vue:92`.
- `v-model` component contracts use `update:*` emit names. Examples:
  - `src/components/BackupRestore/S3ConfigDialog.vue:306` emits `update:visible` and `src/components/BackupRestore/S3ConfigDialog.vue:321`-`323` closes by emitting false.
  - `src/components/ContextMenu/ContextMenu.vue:26` emits `update:visible` and `close`; `src/components/ContextMenu/ContextMenu.vue:44`-`47` emits both on close.
  - `src/components/Play/FullPlay.vue:92` includes `update:showComments`.
- Local state uses `ref` for mutable UI state and `computed` for derived state:
  - `src/components/SettingsSearch.vue:12`-`15` uses refs for search UI state and `src/components/SettingsSearch.vue:36`-`46` computes filtered items.
  - `src/components/Music/SongVirtualList.vue:95`-`106` uses refs/computed for sort state.
  - `src/components/Settings/BackgroundRenderSettings.vue:25`-`36` uses computed settings/options from the store.
- Pinia store composition is common inside components; refs are extracted with `storeToRefs` when store state needs reactivity:
  - `src/components/TitleBarControls.vue:26`-`29` uses `LocalUserDetailStore`, `storeToRefs`, and `useSettingsStore`.
  - `src/components/Play/PlaylistDrawer.vue:24`-`25` uses `LocalUserDetailStore` and `storeToRefs`.
  - `src/components/Settings/BackgroundRenderSettings.vue:7`-`8` uses `useSettingsStore` and `storeToRefs`.
- Router composition is used directly inside components when interactions navigate:
  - `src/components/TitleBarControls.vue:25` and `src/components/TitleBarControls.vue:86`-`92` use `useRouter()` for settings/back actions.
  - `src/components/Music/SongVirtualList.vue:59`-`64` navigates to singer pages.
- Watchers synchronize prop/store state or attach side effects:
  - `src/components/Play/PlaylistDrawer.vue:30` syncs a local virtual-list source from a Pinia list with `{ deep: true }`.
  - `src/components/Music/SongVirtualList.vue:233`-`236` clears caches when songs/sort change, and `src/components/Music/SongVirtualList.vue:354`-`358` reacts to multi-select and song changes.
  - `src/components/Play/FullPlay.vue:142`-`160` watches `props.show` and attaches/removes mousemove behavior.
- Lifecycle hooks are used for global listener setup/cleanup:
  - `src/components/Music/SongVirtualList.vue:342`-`351` adds/removes `document` and `window` listeners.
  - `src/components/Play/PlaylistDrawer.vue:274`-`279` clears timers, drag listeners, and auto-scroll on unmount.
  - `src/components/ContextMenu/ContextMenu.vue:61`-`65` watches visibility and removes global `mousedown` listener on unmount.
- Slots appear in layout/control extension points and UI-library slots:
  - `src/components/TitleBarControls.vue:117` provides a named `before-settings` slot.
  - `src/components/layout/Provider.vue:5` renders the default slot inside providers.
  - TDesign slots such as `#prefix-icon`, `#icon`, and `#footer` appear in `src/components/SettingsSearch.vue:114`, `src/components/Music/SongVirtualList.vue:453`, and `src/components/TitleBarControls.vue:187`.

#### Styling conventions observed

- Most Vue files use `<style scoped>` or `<style lang="scss" scoped>`:
  - `src/components/SettingsSearch.vue:151` uses plain scoped CSS.
  - `src/components/TitleBarControls.vue:199`, `src/components/Play/PlaylistDrawer.vue:397`, and `src/components/Settings/BackgroundRenderSettings.vue:204` use scoped SCSS.
  - Some components also include non-scoped style blocks for global overrides, e.g. `src/components/Play/FullPlay.vue:1934`, `src/components/Music/SongVirtualList.vue:1195`, and `src/components/Settings/PluginSettings.vue:1047`.
- Styling relies heavily on CSS custom properties rather than hard-coded color values, especially TDesign variables and project-level variables:
  - `src/components/SettingsSearch.vue:157`-`180` uses `var(--td-bg-color-*)`, `var(--td-border-*)`, `var(--td-brand-color)`, and `var(--td-text-color-*)`.
  - `src/components/Play/PlaylistDrawer.vue:406`-`412` uses `var(--td-bg-color-container)`, `var(--td-text-color-primary)`, and `color-mix(...)`.
  - `src/components/Settings/BackgroundRenderSettings.vue:210`-`258` uses `--settings-*` variables plus TDesign warning variables.
  - Global tokens are defined in `src/assets/main.css:1`-`27` for motion/glass/mobile variables and in `src/assets/main.css:29` onward for theme/TDesign-compatible variables.
- Component styles commonly customize TDesign internals with `:deep()` inside scoped blocks, e.g. `src/components/SettingsSearch.vue:157`-`187`, and nested `:deep(.t-icon)` inside `src/components/Music/SongVirtualList.vue:721`.
- SCSS nesting is used in many scoped style blocks, e.g. nested `.control-btn`, `.left`, `.title-box` under `.title-controls` in `src/components/TitleBarControls.vue:200`-`258`, and nested settings sections in `src/components/Settings/BackgroundRenderSettings.vue:205`-`248`.
- Responsive/mobile behavior is usually CSS media-query based using `@media (max-width: 768px)`, plus mobile CSS variables from global styles:
  - Global mobile app/dialog rules in `src/assets/base.css:67`-`89` and `src/assets/base.css:151`-`259`.
  - Context menu mobile bottom-sheet styling in `src/components/ContextMenu/ContextMenu.vue:99`-`119`.
- Glass/blur surface styling is common for overlays/drawers/dialog-like surfaces:
  - `src/components/Play/PlaylistDrawer.vue:406`-`408` uses translucent `color-mix` background, `backdrop-filter`, and shadow.
  - `src/components/BackupRestore/S3ConfigDialog.vue:353`-`368` labels and implements Liquid Glass overlay styles.
  - `src/assets/base.css:147`-`149` applies blur to `.t-dialog__mask` globally.
- Global reset and baseline styling live in CSS assets:
  - `src/assets/base.css:23`-`29` applies box-sizing/margin/font-weight reset.
  - `src/assets/base.css:99`-`121` defines body typography and anti-aliasing.
  - `src/assets/base.css:123`-`146` defines global scrollbar styling.
- Inline styles are used for small dynamic values or component-specific widths/margins in templates, e.g. `src/components/Settings/BackgroundRenderSettings.vue:86`, `src/components/Settings/BackgroundRenderSettings.vue:133`, `src/components/Music/SongVirtualList.vue:501`-`510`, and `src/components/TitleBarControls.vue:133`.

### Concrete File Path Examples

1. `src/components/SettingsSearch.vue`
   - Shows the compact reusable component pattern: `<script setup lang="ts">`, inline typed optional prop, typed tuple emit, local search state with `ref`, derived filtering with `computed`, TDesign input and slots, scoped CSS, and `:deep()` overrides of TDesign internals.
   - Key lines: props/emits at `src/components/SettingsSearch.vue:4`-`10`, computed filtering at `src/components/SettingsSearch.vue:36`-`46`, template at `src/components/SettingsSearch.vue:103`-`148`, scoped styles at `src/components/SettingsSearch.vue:151` onward.

2. `src/components/TitleBarControls.vue`
   - Shows typed props with defaults, router/store composition, named slots, Tauri window APIs through dynamic imports, TDesign dialog/footer slot, and scoped SCSS nesting.
   - Key lines: `interface Props` and defaults at `src/components/TitleBarControls.vue:7`-`23`, store/router setup at `src/components/TitleBarControls.vue:25`-`29`, slot at `src/components/TitleBarControls.vue:117`, scoped SCSS at `src/components/TitleBarControls.vue:199` onward.

3. `src/components/Music/SongVirtualList.vue`
   - Shows the high-complexity list component pattern: typed props/defaults, event API, TanStack virtualizer, Pinia store, local caches, multi-select state, context menu via Teleport, lifecycle cleanup, and imperative `defineExpose`.
   - Key lines: props at `src/components/Music/SongVirtualList.vue:18`-`43`, emits at `src/components/Music/SongVirtualList.vue:45`-`57`, virtualizer setup at `src/components/Music/SongVirtualList.vue:172`-`186`, lifecycle at `src/components/Music/SongVirtualList.vue:342`-`351`, Teleport at `src/components/Music/SongVirtualList.vue:641`-`672`, styles at `src/components/Music/SongVirtualList.vue:676` onward.

4. `src/components/Play/PlaylistDrawer.vue`
   - Shows overlay/drawer interaction conventions: `withDefaults`, tuple emits, storeToRefs, VueUse virtual list, watcher synchronization, timers/document listeners with cleanup, transitions, `defineExpose`, and glass styling via TDesign tokens and `color-mix`.
   - Key lines: props/emits at `src/components/Play/PlaylistDrawer.vue:9`-`22`, virtual list at `src/components/Play/PlaylistDrawer.vue:27`-`35`, cleanup at `src/components/Play/PlaylistDrawer.vue:115`-`126` and `src/components/Play/PlaylistDrawer.vue:274`-`279`, transitions/template at `src/components/Play/PlaylistDrawer.vue:316`-`394`, styling at `src/components/Play/PlaylistDrawer.vue:397` onward.

### Recommendations for `.trellis/spec/frontend/component-guidelines.md`

- **Overview / component model**: document that frontend components are Vue 3 SFCs using Composition API with TypeScript and usually `<script setup lang="ts">`; note that both script-first and template-first order exists, but representative reusable components commonly use script/template/style blocks in one SFC.
- **Component structure**: include standard sections for imports, typed props/defaults, emits, store/router/composable setup, local refs/computed/watchers, handlers, lifecycle cleanup, optional `defineExpose`, template, and scoped styles.
- **Props conventions**: specify observed forms: inline `defineProps<{ ... }>()` for small props and `interface Props` + `withDefaults(defineProps<Props>(), ...)` for larger/defaulted APIs. Include optional prop defaults from `TitleBarControls.vue` and `SongVirtualList.vue` as examples.
- **Emits conventions**: capture all observed emit styles but recommend documenting typed emits as the spec example because many components already use tuple/call-signature typing. Include `update:*` events for `v-model`-style bindings (`update:visible`, `update:showComments`) and ordinary semantic events (`select`, `close`, `playSong`, `play`).
- **Composition/state conventions**: document use of `ref` for mutable local UI state, `computed` for derived state/options/classes, `watch` for prop/store synchronization and side effects, Pinia stores for shared app state, `storeToRefs` for reactive store state extraction, and direct router use for navigation actions.
- **Lifecycle/side effects**: document that global `window`/`document` listeners, timers, and manual drag/scroll handlers should be paired with cleanup in `onUnmounted` or visibility watchers; existing examples include `SongVirtualList.vue`, `PlaylistDrawer.vue`, and `ContextMenu.vue`.
- **Slots and expose**: document named slots for extension points (`before-settings`) and provider default slots; document `defineExpose` for parent-driven imperative methods such as virtual-list scrolling.
- **UI library usage**: document TDesign Vue Next as the primary component/control library and `tdesign-icons-vue-next` plus iconfont as observed icon sources; note Naive UI provider usage at the app/provider layer.
- **Styling conventions**: document preference for `<style scoped>` / `<style lang="scss" scoped>`, use of SCSS nesting where helpful, `:deep()` for scoped overrides of third-party components, and limited non-scoped style blocks only for global overrides.
- **Theme tokens**: document that component styles should use CSS variables from TDesign/project theme tokens (`--td-*`, `--theme-*`, feature-specific variables like `--settings-*`, `--song-list-*`) and `color-mix()` for translucent states; global tokens live in `src/assets/main.css` and baseline styles in `src/assets/base.css`.
- **Responsive/overlay styling**: include the `@media (max-width: 768px)` mobile breakpoint, mobile safe-area/glass variables, and overlay patterns using Teleport, fixed positioning, scrim, blur, z-index, and named transitions.
- **Examples section**: link the spec examples to the four concrete files above: `SettingsSearch.vue`, `TitleBarControls.vue`, `SongVirtualList.vue`, and `PlaylistDrawer.vue`.

### Related Specs

| Spec Path | Description |
|---|---|
| `.trellis/spec/frontend/component-guidelines.md` | Target spec file; currently contains placeholder sections for overview, structure, props, styling, accessibility, and common mistakes. |
| `.trellis/spec/frontend/directory-structure.md` | Placeholder for directory conventions; relevant for where reusable components, views, stores, assets, and composables live. |
| `.trellis/spec/frontend/state-management.md` | Placeholder for state conventions; relevant because components frequently use Pinia stores and `storeToRefs`. |
| `.trellis/spec/frontend/hook-guidelines.md` | Placeholder for composable/hook conventions; relevant because components use project composables such as `useDynamicSongTheme` and VueUse/TanStack hooks. |

### External References

Not used. This task was internal codebase research only.

## Caveats / Not Found

- The existing `.trellis/spec/frontend/component-guidelines.md` is a placeholder, so the recommendations above are derived from source inspection rather than prior written conventions.
- No `.tsx` component files were found under `src`; the component layer appears to be Vue SFC-based for this research scope.
- Emit typing is not fully uniform: typed tuple/call-signature emits and string-array emits both exist.
- Some components include non-scoped style blocks for global overrides, but the dominant style pattern is scoped CSS/SCSS.
- This report describes existing patterns and spec content candidates only; no application source or `.trellis/spec` files were modified.
