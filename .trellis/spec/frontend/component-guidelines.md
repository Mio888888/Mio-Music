# Component Guidelines

> How components are built in this project.

---

## Overview

Frontend components are Vue 3 single-file components using the Composition API and TypeScript. The dominant pattern is `<script setup lang="ts">`, TDesign Vue Next controls in templates, scoped CSS/SCSS for component styling, and Pinia/router/composables where needed.

Both script-first and template-first SFC block order exist. Follow the surrounding file/folder style when editing existing components.

---

## Component Structure

A typical component keeps related UI behavior in one SFC:

1. Imports from Vue, stores, router, services, types, and icons.
2. Typed props and emits.
3. Store/router/composable setup.
4. Local `ref` state for mutable UI state.
5. `computed` values for derived lists/options/classes/styles.
6. `watch` for prop/store synchronization and side effects.
7. Handler functions and async actions.
8. Lifecycle cleanup for document/window/Tauri/IPC listeners, timers, drag/scroll handlers, and render resources.
9. Optional `defineExpose` for parent-driven imperative methods.
10. Template using TDesign/Naive UI components, slots, `Teleport`, and transitions as needed.
11. Scoped styles, often SCSS.

Keep interaction-heavy component behavior local when it is specific to the component. Extract a composable only when the logic is reused or encapsulates a distinct lifecycle/resource concern.

---

## Props and Emits

- Use inline `defineProps<{ ... }>()` for small prop sets.
- Use `interface Props` plus `withDefaults(defineProps<Props>(), defaults)` for larger/defaulted APIs.
- Prefer typed emits for new code:
  - tuple style: `defineEmits<{ close: []; playSong: [song: SongList] }>()`
  - call-signature style is also present for `update:*` events.
- Use `update:*` event names for `v-model` contracts, such as `update:visible` and `update:showComments`.
- Semantic events should describe user intent: `select`, `close`, `playSong`, `play`, etc.
- Untyped string-array emits exist in older/larger components; avoid adding new untyped emits unless matching a local legacy pattern.

---

## Composition and State

- Use `ref` for local mutable UI state such as loading flags, selected rows, sort state, modal visibility, tabs, and pagination.
- Use `computed` for derived options, lists, display state, inline style objects, and token-driven theme values.
- Use Pinia stores for shared app/playback/user/settings state. Use `storeToRefs` when destructuring reactive store state.
- Use `useRouter()` directly in components when user interactions navigate.
- Watchers are acceptable for synchronizing props/store state, triggering fetches, or binding side effects. Pair side-effect watchers with cleanup when they register external resources.
- Use `defineExpose` sparingly for imperative APIs already used in this codebase, such as virtual-list scrolling.

---

## UI Libraries and Overlays

- TDesign Vue Next is the primary component/control library (`t-input`, `t-button`, `t-dialog`, `t-switch`, `t-slider`, etc.).
- TDesign icons and `iconfont` classes are both used. Follow nearby code for icon source.
- Naive UI providers are used at the app/provider layer (`src/components/layout/Provider.vue`), not as the default choice for feature controls.
- Overlay/context-menu/drawer patterns may use `Teleport` to `body`, fixed positioning, scrims/blur, named transitions, and mobile-specific bottom-sheet styles.

---

## Styling Patterns

- Prefer `<style scoped>` or `<style lang="scss" scoped>` in components.
- Use `:deep()` only when scoped styles must override third-party component internals.
- Use non-scoped style blocks only for intentional global overrides.
- Prefer CSS custom properties from TDesign and project tokens: `--td-*`, `--theme-*`, and feature tokens such as `--settings-*` or `--song-list-*`.
- Use `color-mix()` for translucent/adaptive states instead of hard-coded theme colors.
- Mobile responsive rules commonly use `@media (max-width: 768px)` plus safe-area/touch-target tokens from global CSS.
- Respect existing motion tokens and `prefers-reduced-motion` behavior when adding transitions/animations.

---

## Accessibility

Accessibility is not fully standardized, but new components should preserve observed good patterns:

- Add `aria-label` for icon-only controls when the visible text is absent.
- Use `role="status"` and `aria-live="polite"` for loading/status regions when appropriate.
- Use `aria-busy` for long-running central actions.
- Keep keyboard and pointer behavior intact when adding custom overlays or document listeners.

---

## Examples

- `src/components/SettingsSearch.vue` — compact reusable component with typed props/emits, local search refs, computed filtering, TDesign input slots, scoped CSS, and `:deep()` overrides.
- `src/components/TitleBarControls.vue` — typed props with defaults, store/router composition, named slot, Tauri window APIs, TDesign dialog, and scoped SCSS.
- `src/components/Music/SongVirtualList.vue` — high-complexity list component with typed display options, virtualizer, Pinia state, multi-select, context-menu `Teleport`, lifecycle cleanup, and `defineExpose`.
- `src/components/Play/PlaylistDrawer.vue` — drawer/overlay with tuple emits, `storeToRefs`, VueUse virtual list, cleanup, transitions, exposed scroll method, and token-based glass styling.

---

## Common Mistakes

- Adding global listeners, timers, drag handlers, or render loops without unmount cleanup.
- Adding hard-coded colors for themeable UI instead of TDesign/project tokens.
- Promoting settings/page-only widgets into global components prematurely.
- Adding untyped emits or broad props when a small typed contract would match the project style.
