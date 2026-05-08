# Research: setup guide UI patterns

- **Query**: Research UI/UX patterns in the active project for guiding the find page when no Subsonic config or source plugin exists.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `src/views/music/find.vue` | Discover page shell with header, segmented tabs, and responsive layout for songlists/leaderboards. |
| `src/components/Find/PlaylistCategory.vue` | Main find-page songlist content; representative loading, error, empty, grid, category chip, dropdown, and mobile bottom-sheet patterns. |
| `src/components/Find/LeaderBord.vue` | Find-page leaderboard content; compact loading skeleton and empty-with-retry pattern. |
| `src/views/settings/index.vue` | Settings route shell; supports `?category=` and `?section=` query navigation, category switching, mobile horizontal category nav. |
| `src/views/settings/sections/MusicSourceSection.vue` | Music source settings section; Subsonic built-in source config, plugin/source prompt, source cards, quality tags, mobile layout. |
| `src/components/Settings/PluginSettings.vue` | Plugin install/list/config UI; state blocks, add-plugin dialogs, plugin actions, service-plugin config dialog, responsive action grid. |
| `src/store/LocalUserDetail.ts` | User/source state; Subsonic validity helper, built-in source merge, persisted user info, current source computed. |
| `src/store/plugin.ts` | Plugin state; plugin list, music-source vs service type, add/import/select/config/test APIs. |
| `src/services/musicSdk.ts` | Music SDK facade; current source fallback and Subsonic config injection for SDK requests. |
| `src/components/layout/HomeLayout.vue` | App shell source selector, search bar, settings entry, sidebar/mobile navigation, source list derived from supported sources. |
| `src/components/TitleBarControls.vue` | Header controls; existing settings navigation pattern (`router.push('/settings')`). |
| `src/views/settings/searchIndex.ts` | Settings search metadata; existing category/section IDs for music source and plugin management. |
| `.trellis/spec/frontend/component-guidelines.md` | Frontend component/UI conventions: Vue 3 SFC, TDesign primary, scoped CSS, token styling, mobile rules. |
| `.trellis/spec/frontend/state-management.md` | State conventions: local refs for page UI, Pinia for shared source/plugin/user state, explicit loading/error flags. |
| `.trellis/spec/frontend/directory-structure.md` | Placement conventions for route pages, settings-only sections, components, stores, services. |

### Code Patterns

#### Discover/find page structure

- `src/views/music/find.vue:8-38` renders the page as:
  - `.find-container`
  - `.page-header` with title/subtitle
  - custom `.segment-tabs` buttons for `歌单` / `排行榜`
  - `.tab-content` containing `PlaylistCategory` and `LeaderBord`
- Styling uses project/TDesign tokens:
  - header title uses `border-left: 8px solid var(--td-brand-color-3)` and `color: var(--td-text-color-primary)` (`src/views/music/find.vue:51-69`).
  - segmented tab uses `var(--td-bg-color-secondarycontainer)`, active background `var(--td-bg-color-container)`, and `var(--td-brand-color)` (`src/views/music/find.vue:71-103`).
- Mobile rules at `src/views/music/find.vue:116-161` remove the title border, use large responsive typography (`clamp(2rem, 9vw, 2.6rem)`), `--mobile-page-gutter`, `--mobile-touch-target`, `--mobile-control-radius`, and glass tokens (`--mobile-glass-bg-strong`, `--mobile-glass-border`).

#### Existing empty/setup/error UI examples

- Find songlist empty/error/loading:
  - `src/components/Find/PlaylistCategory.vue:324-341`:
    - loading: centered `<t-loading size="large" text="正在加载歌单..." />`
    - error: `<t-alert theme="error" :message="error" />` plus primary retry button
    - empty: custom icon orb with `<t-icon name="music" />`, heading `暂无歌单`, helper text
  - Desktop styles:
    - loading/error centered with padding (`src/components/Find/PlaylistCategory.vue:486-496`)
    - `.empty-container` uses flex-column center, gap, min-height `260px`, text center, secondary text (`src/components/Find/PlaylistCategory.vue:541-574`)
    - `.empty-orb` is a 64px rounded brand-color light tile (`src/components/Find/PlaylistCategory.vue:553-563`)
  - Mobile styles convert loading/error/empty into a card-like block with `min-height: 220px`, border, radius, background, and touch-sized retry button (`src/components/Find/PlaylistCategory.vue:1003-1030`).

- Find leaderboard empty/loading:
  - `src/components/Find/LeaderBord.vue:50-68`:
    - loading skeleton grid uses local `.skeleton-card` blocks
    - empty state shows `暂无榜单数据` and text-variant retry `<t-button variant="text" @click="fetchBoards">重试</t-button>`
  - `.empty-state` centers content vertically with `min-height: 300px` and secondary text (`src/components/Find/LeaderBord.vue:113-124`).
  - Mobile grid switches to two columns and tighter gaps (`src/components/Find/LeaderBord.vue:131-144`).

- Settings music-source setup prompt:
  - `src/views/settings/sections/MusicSourceSection.vue:296-307` shows a setup prompt when no supported sources exist:
    - icon circle using `TreeRoundDotIcon`
    - heading `未检测到插件配置`
    - helper text
    - primary button `前往插件管理`
  - The prompt is styled as a dashed-border card with `var(--settings-plugin-prompt-bg, var(--td-bg-color-container))`, `var(--settings-plugin-prompt-border, var(--td-border-level-1-color))`, rounded corners, brand gradient icon, and primary/secondary text tokens (`src/views/settings/sections/MusicSourceSection.vue:417-429`).
  - Mobile prompt switches to vertical centered layout, smaller icon, tighter padding, and 13px helper text (`src/views/settings/sections/MusicSourceSection.vue:507-520`).

- Plugin management state blocks:
  - `src/components/Settings/PluginSettings.vue:14-35` has loading, error, and empty states:
    - loading: `.state-block.loading-state` + `<t-loading size="medium" />`
    - error: large `error-circle` icon, title, error message, retry button
    - empty: large app icon, `暂无已安装的插件`, helper text `点击"添加插件"按钮来安装新插件`
  - `.state-block` is reusable: flex-column centered, `padding: 60px 0`, `gap: 12px`, tokenized background/border/radius/text (`src/components/Settings/PluginSettings.vue:603-632`).
  - Mobile state block tightens to `padding: 36px 14px` and `border-radius: 10px` (`src/components/Settings/PluginSettings.vue:953-956`).

- Local playlist empty state:
  - `src/views/music/songlist.vue:565-575` shows a custom empty with icon, heading, helper text, and primary CTA `创建歌单`.
  - Styles use centered text, large placeholder icon, primary/secondary text, and CTA spacing (`src/views/music/songlist.vue:1002-1027`).

- Search empty state:
  - `src/views/music/search.vue:174-190` uses a minimal `.empty-state > .empty-content` with heading/helper text for no song/playlist results.
  - Styles center content at `min-height: 300px`; heading uses primary text, helper uses secondary (`src/views/music/search.vue:223-226`).

#### Setup/config actions already present

- Subsonic built-in source config:
  - `src/views/settings/sections/MusicSourceSection.vue:189-208` renders the Subsonic card:
    - `<t-switch v-model="subsonicConfig.enabled" />`
    - `<t-input>` fields for server (`baseUrl`), username, password, API version, client name
    - primary `测试连接` button with loading state
    - hint `连接成功后会在音乐源列表中显示 Subsonic。`
  - `src/views/settings/sections/MusicSourceSection.vue:24-41` lazily initializes `userInfo.subsonicConfig` with defaults: empty `baseUrl`, `username`, `password`, `apiVersion: '1.16.1'`, `clientName: 'Mio'`, `enabled: false`.
  - `src/views/settings/sections/MusicSourceSection.vue:43-55` calls `userStore.ensureBuiltInSources(userInfo.value)` and handles removing invalid Subsonic selection.
  - `src/views/settings/sections/MusicSourceSection.vue:57-74` tests connection via `musicSdk.request('ping', { source: 'subsonic' })`; success sets `enabled = true`, syncs source, and shows `MessagePlugin.success`.

- Subsonic validity/source state:
  - `src/store/LocalUserDetail.ts:6-10` defines the built-in source as `subsonic` with name `Subsonic`, type `内置音源`, and qualities `['128k', '320k', 'flac']`.
  - `src/store/LocalUserDetail.ts:12-20` defines valid Subsonic config as `enabled && baseUrl.trim() && username.trim() && password`.
  - `src/store/LocalUserDetail.ts:22-34` merges/removes `subsonic` in `supportedSources` based on validity.
  - `src/store/LocalUserDetail.ts:77-90` calls `ensureBuiltInSources` during user info initialization.
  - `src/store/LocalUserDetail.ts:163-167` exposes `userSource` with `pluginId`, selected source, and selected quality.

- Music SDK source behavior:
  - `src/services/musicSdk.ts:90-93` falls back to source `kw` if `store.userSource.source` is empty.
  - `src/services/musicSdk.ts:95-110` injects `subsonicConfig` into SDK args when `source === 'subsonic'`.
  - `src-tauri/src/music_sdk/sources/mod.rs:8` registers `subsonic`; dispatch maps source `subsonic` to `subsonic::handle` at `src-tauri/src/music_sdk/sources/mod.rs:13-24`.

- Plugin install and selection:
  - Header actions in plugin settings include primary `添加插件` and default `刷新` (`src/components/Settings/PluginSettings.vue:2-11`).
  - Add flow uses two dialogs:
    - type selection with `t-radio-group` `cr` / `lx` (`src/components/Settings/PluginSettings.vue:105-118`)
    - import method with `local` / `online`, online URL input, local import tip (`src/components/Settings/PluginSettings.vue:120-146`)
  - Import handler validates URL for online imports and calls `store.selectAndAdd` or `store.downloadAndAdd`, with `MessagePlugin` success/warning/error feedback (`src/components/Settings/PluginSettings.vue:351-376`).
  - Plugin cards show source tags and actions (`src/components/Settings/PluginSettings.vue:38-103`): logs, configure/import for service plugins, `使用` for non-service music-source plugins, uninstall.
  - Music-source plugin selection updates `userInfo.pluginId`, `pluginName`, `supportedSources`, `selectSources`, and `selectQuality` (`src/components/Settings/PluginSettings.vue:381-432`).
  - Store plugin type is explicit: `LoadedPlugin.plugin_type: 'music-source' | 'service'` (`src/store/plugin.ts:18-24`); `isServicePlugin` checks `plugin.plugin_type === 'service'` (`src/store/plugin.ts:153-155`).

#### Routing/navigation targets

- Central route map:
  - Find page is `/home/find`, name `find` (`src/router/index.ts:26-32`).
  - Settings page is `/settings`, name `settings` (`src/router/index.ts:44-48`).
- Settings page supports direct category/section query handling:
  - `src/views/settings/index.vue:166-180` watches `route.query.category` and `route.query.section`; it switches category and optionally scrolls/highlights an element by ID.
  - `src/views/settings/index.vue:132-144` implements `switchCategory(categoryKey)`.
- Existing settings categories:
  - plugin category key is `plugins`, label `插件管理`, description `插件安装、配置和管理` (`src/views/settings/index.vue:57-62`).
  - music category key is `music`, label `音乐源`, description `音乐源选择和音质配置` (`src/views/settings/index.vue:63-68`).
- Existing search index IDs:
  - `music-source` / category `music` / title `音乐源选择` (`src/views/settings/searchIndex.ts:26-28`).
  - `music-quality` / category `music` / title `音质选择` (`src/views/settings/searchIndex.ts:28`).
  - `plugin-settings` / category `plugins` / title `插件管理` (`src/views/settings/searchIndex.ts:40-42`).
- Existing navigation patterns:
  - `TitleBarControls` goes to settings with `router.push('/settings')` (`src/components/TitleBarControls.vue:86-88`).
  - Within settings, `MusicSourceSection` emits `switch-category` and its `goPlugin` action emits `plugins` (`src/views/settings/sections/MusicSourceSection.vue:9`, `src/views/settings/sections/MusicSourceSection.vue:180`).
- Practical targets for a find-page setup guide:
  - Configure Subsonic: route to `/settings?category=music` (Subsonic card is at the top of the music section).
  - Install source plugin: route to `/settings?category=plugins` (plugin management section contains the add button and plugin list).
  - `?section=music-source` scrolls to the music source card, not the Subsonic config card. No dedicated Subsonic element ID was found.
  - `?section=plugin-settings` is present in search metadata, but no matching `id="plugin-settings"` was found in `PluginSettings.vue`; category navigation is the reliable target.

#### TDesign components/patterns to reuse

Observed TDesign usage relevant to setup guide:

- `t-button`: primary CTA, default/outline/text buttons, loading button, icon slots.
- `t-icon` and `tdesign-icons-vue-next`: large illustrative icons (`music`, `app`, `error-circle`, `TreeRoundDotIcon`).
- `t-loading`: central loading states in find and plugin pages.
- `t-alert`: error message block in find songlist error state.
- `t-dialog`: plugin install/config/import flows.
- `t-radio-group` / `t-radio-button`: plugin type/import method choices.
- `t-input`, `t-input-number`, `t-select`, `t-option`, `t-switch`: Subsonic/plugin config forms.
- `t-tag`: current plugin/source/quality tags.
- `MessagePlugin`: operation success/warning/error feedback.

Style conventions:

- Prefer tokenized colors such as `--td-text-color-primary`, `--td-text-color-secondary`, `--td-text-color-placeholder`, `--td-bg-color-container`, `--td-bg-color-secondarycontainer`, `--td-border-level-1-color`, `--td-brand-color`, `--td-brand-color-light`, and `--td-brand-color-1..6`.
- Settings contexts often use `--settings-*` token fallbacks (`src/views/settings/sections/MusicSourceSection.vue:313-429`, `src/components/Settings/PluginSettings.vue:603-648`).
- Feature cards generally use rounded corners (`12px`, `0.75rem`, `1rem`), token backgrounds, token borders, and subtle hover/active transforms.
- Project specs confirm TDesign Vue Next as the primary control library, `iconfont`/TDesign icons are both used, scoped styles are preferred, and mobile uses `@media (max-width: 768px)` plus global mobile tokens (`.trellis/spec/frontend/component-guidelines.md:58-75`).

#### Mobile responsive considerations

- Find page:
  - uses `--mobile-page-top-gutter`, `--mobile-page-gutter`, `--mobile-touch-target`, and mobile glass tokens (`src/views/music/find.vue:116-161`).
  - mobile header typography is large, borderless, and compact.
- PlaylistCategory:
  - category chips become a horizontal scroll strip with hidden scrollbars, scroll snap, and `touch-action: pan-x` (`src/components/Find/PlaylistCategory.vue:726-847`).
  - “more categories” becomes a bottom sheet with fixed mask, safe-bottom padding, handle, header, close button, scrollable body, and 3-column chips (`src/components/Find/PlaylistCategory.vue:850-991`).
  - empty/error/loading blocks become card-like bordered containers (`src/components/Find/PlaylistCategory.vue:1003-1030`).
  - playlist grid becomes single-column list cards (`src/components/Find/PlaylistCategory.vue:1032-1126`).
- Settings page:
  - settings sidebar becomes a horizontal chip nav; content panel becomes a bordered, scrollable glass card (`src/views/settings/index.vue:459-608`).
  - all controls/buttons/inputs are given `min-height: var(--mobile-touch-target)` (`src/views/settings/index.vue:659-665`).
  - setting groups/cards/items are tightened with smaller radii/padding (`src/views/settings/index.vue:610-684`).
- Plugin settings:
  - header actions become full-width/flex buttons (`src/components/Settings/PluginSettings.vue:930-951`).
  - plugin cards stack vertically; action buttons use a 2-column grid (`src/components/Settings/PluginSettings.vue:958-986`).
- Music source settings:
  - source cards become one column, quality tags become two-column flexible tags, config status becomes one column, prompt becomes centered vertical (`src/views/settings/sections/MusicSourceSection.vue:431-520`).
- Home layout:
  - desktop sidebar is hidden on mobile; bottom navigation is a fixed glass pill (`src/components/layout/HomeLayout.vue:605-720`).
  - search input/source selector controls meet `--mobile-touch-target` (`src/components/layout/HomeLayout.vue:644-666`).

### Related Specs

- `.trellis/spec/frontend/component-guidelines.md` — Vue 3 SFC/TDesign/scoped CSS/token/mobile/accessibility conventions.
- `.trellis/spec/frontend/state-management.md` — local UI state vs Pinia shared state, manual server state, explicit loading/error handling.
- `.trellis/spec/frontend/directory-structure.md` — route pages under `src/views`, settings-only sections under `src/views/settings/sections`, feature components under `src/components`.

## Recommended UI Shape for Find Page Setup Guide

- Place the no-source gate in `src/views/music/find.vue` before rendering the existing segmented tabs and content, so the find page does not mount `PlaylistCategory` or `LeaderBord` and therefore does not request default source data.
- Keep the existing page header (`发现音乐`) for visual continuity, but replace tabs/content with one centered setup panel.
- Use a custom card/prompt pattern rather than introducing a new `t-empty`, because existing empty/setup states are custom divs with `t-icon`, headings, helper text, and TDesign buttons.
- Suggested visual composition based on existing patterns:
  - large brand-colored icon orb similar to `PlaylistCategory.empty-orb` or `MusicSourceSection.prompt-icon`.
  - title such as `配置音乐源以开始发现音乐`.
  - helper text explaining that Mio needs either Subsonic or a music-source plugin before showing recommendations/playlists/rankings.
  - two action cards or CTA rows:
    - `配置 Subsonic` with description `连接 Navidrome/Subsonic 兼容服务` and primary button routing to `/settings?category=music`.
    - `安装音源插件` with description `导入本地或在线插件` and default/outline button routing to `/settings?category=plugins`.
  - Optional small hint line that configuration stays local, matching settings copy tone.
- Desktop layout can use a max-width centered card (`min-height` similar to 260–300px empty states), token background/border/radius, and horizontal/two-column action cards.
- Mobile layout should stack action cards vertically, use `--mobile-page-gutter`, `--mobile-touch-target`, `--mobile-card-radius*`, avoid hover-only affordances, and keep buttons full width or near full width.
- Detection sources to inspect/compose from existing state:
  - valid Subsonic config: `LocalUserDetailStore().hasValidSubsonicConfig(userInfo)` (`src/store/LocalUserDetail.ts:12-20`).
  - supported built-in/plugin sources: `userInfo.supportedSources` after `ensureBuiltInSources` (`src/store/LocalUserDetail.ts:22-34`).
  - installed music-source plugins: `usePluginStore().plugins` entries where `plugin_type === 'music-source'` (`src/store/plugin.ts:18-24`).

## Caveats / Not Found

- No dedicated `id` for the Subsonic config group was found; `/settings?category=music` is the reliable existing navigation target. If deep scrolling is required, a future app-source change would need an element ID on the Subsonic group.
- `src/views/settings/searchIndex.ts` references `plugin-settings`, but no matching `id="plugin-settings"` was found in `PluginSettings.vue`; `/settings?category=plugins` is the reliable existing target.
- The active worktree initially lacked recent Subsonic files, while the primary repo path contains Subsonic support. Subsonic findings above are from the primary repo at `/Users/vant/Documents/开发/Mio/音乐` as requested.
- No external UI references were used; the task was satisfied by internal project patterns.
