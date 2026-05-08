# Guide Setup When No Music Source

## Goal

Modify the discover/find page so that when the user has neither configured Subsonic API nor installed any music-source plugin, the app stops showing default source data and instead guides the user to configure Subsonic or install a source plugin.

## What I already know

* The target UI is the discover/find page.
* The current app recently added built-in Subsonic source support.
* The user wants to avoid showing default source data when there is no usable music source.
* The replacement state should guide users to either configure Subsonic API or install a source plugin.

## Assumptions (temporary)

* “Subsonic API configured” means the app has enough Subsonic connection settings saved to fetch data successfully.
* “Installed music-source plugin” means at least one installed plugin with plugin type/source capability usable by music discovery.
* The change should only affect the no-source state, not users who already have Subsonic configured or plugins installed.

## Open Questions

* None yet; inspect code first.

## Research References

* [`research/discover-source-state.md`](research/discover-source-state.md) — find page fetch flow, default source fallback, plugin/Subsonic state detection, likely files.
* [`research/setup-guide-ui-patterns.md`](research/setup-guide-ui-patterns.md) — existing empty/setup UI patterns, settings navigation targets, mobile styling.

## Requirements (evolving)

* Detect the no-source state: no configured Subsonic API and no installed music-source plugins.
* In that state, the discover/find page must not request or show default source data.
* Show a clear empty/setup guide with actions to configure Subsonic or install a source plugin.
* Use existing settings route targets: `/settings?category=music` for Subsonic/music source setup and `/settings?category=plugins` for plugin management.
* Preserve existing discover page behavior for users with a configured Subsonic source or installed plugins.
* Follow existing UI, state, token styling, and responsive conventions.

## Acceptance Criteria (evolving)

* [ ] With no Subsonic config and no installed source plugins, discover/find shows setup guidance instead of default source content.
* [ ] In no-source state, `PlaylistCategory` and `LeaderBord` are not mounted and do not request default `wy/kw` data.
* [ ] Guidance includes an action/path to configure Subsonic/music source settings.
* [ ] Guidance includes an action/path to install a source plugin.
* [ ] With Subsonic configured, existing discovery behavior still works.
* [ ] With a music-source plugin installed, existing plugin-based discovery behavior still works.
* [ ] Build/typecheck passes.

## Technical Approach

* Implement the gate in `src/views/music/find.vue`, before rendering segmented tabs and child content.
* Initialize plugin store on page entry before deciding no-source state, so an unloaded plugin list is not mistaken for no plugins.
* Determine installed music-source plugins with `plugin.plugin_type === 'music-source'`.
* Determine Subsonic configured by checking service plugin saved config for non-empty `serverUrl`, because there is no dedicated Subsonic store/flag in current code.
* Show a setup card using existing TDesign button/icon patterns and mobile responsive tokens.
* Use `v-if` around the existing tabs/content branch so no-source mode prevents `PlaylistCategory` and `LeaderBord` from mounting.

## Decision (ADR-lite)

**Context**: Default source fallback exists in both frontend and backend music SDK paths. Changing global fallback behavior would affect unrelated pages.

**Decision**: Gate only the discover/find page. When no configured Subsonic API and no installed music-source plugin exist, render setup guidance instead of mounting discovery child components.

**Consequences**: The default source behavior remains available elsewhere, but the discover page no longer shows misleading default data for unconfigured users.


## Definition of Done

* Research is persisted under `research/`.
* `implement.jsonl` / `check.jsonl` are curated before implementation.
* Implementation is reviewed by `trellis-check`.
* Work is committed before `/trellis:finish-work`.

## Out of Scope

* Changing Subsonic setup internals unless required for detecting configuration.
* Redesigning the full discovery page.
* Adding a new plugin marketplace.
* Removing existing supported source behavior.

## Technical Notes

* Task directory: `.trellis/tasks/05-08-guide-setup-no-music-source`.
* Need to inspect `src/views/music/find.vue`, Subsonic store/config, plugin store, and source selection logic.
