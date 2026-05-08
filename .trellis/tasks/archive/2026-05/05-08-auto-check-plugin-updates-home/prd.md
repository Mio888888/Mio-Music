# Auto Check Plugin Updates on Home

## Goal

Implement automatic checking for music/source plugin updates when the software home page opens, using `CeruMusic-main` as a read-only reference. The feature should help users notice available plugin updates without manually entering plugin settings.

## What I already know

* The user wants the software home page to automatically check whether source/music plugins have updates.
* `CeruMusic-main` uses a plugin-driven update model: CR plugins call `checkUpdate()` → `NoticeCenter('update', ...)`, LX plugins emit `updateAlert`. App only displays and installs.
* Current project `pluginWorker.ts` only logs `NoticeCenter(...)` and ignores LX `updateAlert` — plugin update notices never reach UI.
* Current project already has `store.downloadAndAdd(url, pluginType, targetPluginId)` to install/replace plugins.
* No existing "check all installed plugins for updates" API or endpoint exists in the current project.

## Research References

* [`research/current-project-plugin-update.md`](research/current-project-plugin-update.md) — Current project plugin store, bridge, worker, settings, home lifecycle, and gap analysis.
* [`research/cerumusic-plugin-update-reference.md`](research/cerumusic-plugin-update-reference.md) — CeruMusic plugin update check flow, notification UI, install path, and adaptation notes.

## Assumptions (temporary)

* The update check should run when the home page opens, not during app bootstrap before the UI is ready.
* Existing plugin management/update APIs should be reused where possible.
* The check should handle offline/update-check failures gracefully.
* No behavior in `CeruMusic-main` should be changed.

## Open Questions

* None.

## Decision (ADR-lite)

**Context**: Plugin update notifications need a clear, actionable UI that works on both PC and mobile.

**Decision**: Use TDesign Dialog 弹窗, following CeruMusic reference behavior (queue, version info, immediate update + postpone actions). Adapt dialog layout for mobile using the project's existing responsive patterns (`@media (max-width: 768px)`, safe-area, bottom-sheet on mobile).

**Consequences**: Notifications are prominent and consistent with the reference app; mobile users get an adapted layout rather than a desktop-only dialog.

## Requirements (evolving)

* Reference `CeruMusic-main` code read-only to identify plugin update-check behavior.
* Reuse current project plugin/store/service patterns where possible.
* Automatically check installed music/source plugins for updates when entering the software home page.
* Bridge plugin update notices from Worker to main thread so they reach the UI.
* Display update notifications using TDesign dialogs with "立即更新" and "稍后" actions.
* Use `downloadAndAdd(url, pluginType, targetPluginId)` to install updates, preserving current plugin identity.
* Handle update install failures with browser fallback.
* Avoid modifying `CeruMusic-main`.
* Avoid changing unrelated app behavior.

## Acceptance Criteria (evolving)

* [ ] Home-page entry triggers a plugin update check using existing plugin update mechanisms or a compatible implementation.
* [ ] Plugin update notices from CR (`NoticeCenter`) and LX (`updateAlert`) are bridged from Worker to main thread.
* [ ] Update notification dialog shows plugin name, current/new version, update notes, and action buttons.
* [ ] "立即更新" downloads and replaces the plugin using existing store API with target plugin ID.
* [ ] Install failure offers browser fallback.
* [ ] If no updates are available, the check does not interrupt normal startup/home usage.
* [ ] Network/check failures are handled without crashing or blocking the home page.
* [ ] `CeruMusic-main` files are not modified.
* [ ] Build/typecheck passes.

## Definition of Done

* Implementation follows frontend specs.
* Relevant research is persisted under `research/`.
* `implement.jsonl` and `check.jsonl` include relevant specs/research before implementation starts.
* `trellis-implement` and `trellis-check` complete successfully.

## Out of Scope

* Redesigning the plugin management UI.
* Adding a new plugin marketplace.
* Modifying `CeruMusic-main`.
* Changing plugin package format unless existing APIs require it.

## Technical Notes

* Task directory: `.trellis/tasks/05-08-auto-check-plugin-updates-home`.
* Plugin store: `src/store/plugin.ts` — Pinia setup-style, already has `downloadAndAdd(url, pluginType, targetPluginId)`.
* Plugin worker: `src/utils/plugin/pluginWorker.ts` — currently logs `NoticeCenter`, ignores `updateAlert`.
* Plugin runner: `src/utils/plugin/PluginRunner.ts` — Web Worker proxy, has `clearCache`.
* Bridge: `src/bridge/index.ts` — maps `plugins.*` to Tauri commands.
* Home entry: `src/views/home/index.vue` or `src/components/layout/HomeLayout.vue`.
* Backend: `src-tauri/src/plugin/commands.rs`, `manager.rs` — already support target plugin replacement.
* Reference notification dialog: `CeruMusic-main/src/renderer/src/components/PluginNoticeDialog.vue`.
* Reference LX converter: `CeruMusic-main/src/main/services/plugin/manager/converter-event-driven.ts`.
