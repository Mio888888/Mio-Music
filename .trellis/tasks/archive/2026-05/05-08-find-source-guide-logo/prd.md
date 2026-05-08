# 发现页音源引导使用应用 Logo

## Goal

将发现页无音源配置引导卡片顶部的图标，改为与左上角应用 Logo 一致，提升品牌一致性。

## What I already know

* 用户要求“发现页配置音源引导上面的 logo 改为和左上角 logo 一致”。
* 左上角 Logo 位于 `src/components/layout/HomeLayout.vue`，使用 `<img src="/icon.png" alt="Mio Music" />`。
* 发现页引导位于 `src/views/music/find.vue`，当前使用 `.setup-orb` 内的 `<t-icon name="music" />`。
* 这是小范围 UI 调整，应尽量只改发现页引导图标与对应样式。

## Assumptions

* “和左上角 logo 一致”指使用相同图片资源 `/icon.png`。
* 保留现有引导卡片结构、文案、按钮、无源 gate 行为不变。

## Requirements

* 发现页无音源配置引导顶部使用与左上角一致的 `/icon.png`。
* 不改变无源判断逻辑、设置/插件跳转逻辑、歌单/排行榜挂载 gate。
* 保持桌面和移动端布局美观。

## Acceptance Criteria

* [ ] `src/views/music/find.vue` 的引导顶部显示 `/icon.png`，不再显示 TDesign music 图标。
* [ ] 左上角 Logo 资源路径保持不变。
* [ ] 无源引导的布局在桌面与移动端仍正常。
* [ ] Typecheck/build 通过。

## Definition of Done

* 代码改动完成并通过类型检查或构建。
* Trellis check 复核完成。
* 工作提交。

## Out of Scope

* 更换应用全局 Logo 资源。
* 调整发现页无源判断逻辑。
* 改动设置页或插件页。

## Technical Notes

* Task directory: `.trellis/tasks/05-08-find-source-guide-logo`。
* Left sidebar logo pattern: `src/components/layout/HomeLayout.vue:168-171`。
* Find setup guide icon: `src/views/music/find.vue:69-72` and styles around `.setup-orb`.
