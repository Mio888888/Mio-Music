# 调整 Sub 音源基础分类展示

## Goal

将 Subsonic 音源的「最近添加、最新专辑、随机专辑、收藏专辑、按名称排序」从「更多分类」面板移出，作为基础分类与「热门」并列展示在顶部分类栏中，使这些 Subsonic 专属分类可以直接一键切换，无需展开二级面板。

## Requirements

1. Subsonic 音源下，`getPlaylistTags` 返回的 5 个分类（最近添加、最新专辑、随机专辑、收藏专辑、按名称排序）作为基础分类 chip 直接显示在顶部 `.hot-tags` 栏，与硬编码的「热门」按钮并列
2. 当 Subsonic 用户点击某个基础分类 chip 时，该分类的 `id` 应作为 `sortId`（而非 `tagId`）传递给 `getCategoryPlaylists`，使 Subsonic 后端正确映射到 `getAlbumList2.view?type=xxx`
3. Subsonic 音源下隐藏「更多分类」按钮（因为没有额外的分组分类需要展示）
4. 保持非 Subsonic 音源（kw 等其他音源）的原有行为不变

## Acceptance Criteria

- [ ] Subsonic 音源：顶部栏显示「热门 / 最近添加 / 最新专辑 / 随机专辑 / 收藏专辑 / 按名称排序」共 6 个 chip
- [ ] 点击任一 chip 能正确加载对应类型的专辑列表
- [ ] Subsonic 音源下不显示「更多分类」按钮
- [ ] 非 Subsonic 音源行为不受影响
- [ ] `cargo test` 和 `npm run build` / lint 均通过

## Technical Approach

### 核心问题

1. **参数传递不匹配**：前端 `fetchCategoryPlaylists` 将 `sortId` 硬编码为 `'hot'`、用户选择放到 `tagId`；但 Subsonic 后端只读取 `sortId`
2. **tags 结构不兼容**：Subsonic 返回扁平 `[{id, name}]`，前端「更多分类」面板期望分组 `[{name, list: [{id, name}]}]`

### 修改方案

**文件 1: `src/components/Find/PlaylistCategory.vue`**

- 在 `fetchTags` 中检测 Subsonic 音源：将 Subsonic 扁平 `tags` 数组存入 `hotTag`（而非 `tags`），使它们渲染在顶部 `.hot-tags` 栏中
- 在 `fetchCategoryPlaylists` 中检测 Subsonic 音源：将 `activeTagId` 作为 `sortId` 传递（`getCategoryPlaylists(activeTagId.value || 'recent', '', ...)`），而非固定传 `'hot'`
- 在模板中，Subsonic 音源下隐藏「更多分类」按钮（`v-if` 判断）

**文件 2: `src-tauri/src/music_sdk/sources/subsonic/mod.rs`**

- 无需修改后端，当前 `getPlaylistTags` 返回的扁平结构和 `album_list_type` 映射已经正确

**文件 3: `src/services/musicSdk.ts`**

- 无需修改，保持通用传参

### Subsonic 判断方式

通过 `userSource.value.source === 'subsonic'` 判断当前是否为 Subsonic 音源。

## Out of Scope

- 不修改非 Subsonic 音源的行为
- 不修改 Subsonic 后端 Rust 代码
- 不修改「热门」的映射逻辑（仍映射为 `recent`）
- 不处理「随机专辑」缓存刷新问题

## Technical Notes

- 关键文件：`src/components/Find/PlaylistCategory.vue`（主要修改点）
- Subsonic 分类 id 映射：recent / newest / random / starred / alphabeticalByName
- 前端 `userSource.value.source` 可区分当前音源
- 缓存 key 使用 `(activeTagId || 'hot') + ':' + source`，改用 sortId 后需同步调整
