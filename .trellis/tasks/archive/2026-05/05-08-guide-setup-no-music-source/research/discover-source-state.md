# Research: discover source state

- **Query**: Modify discover/find page so that if the user has not configured Subsonic API and has no installed music-source plugins, the page does not show default source data and instead guides setup.
- **Scope**: internal
- **Date**: 2026-05-08

## Findings

### Files Found

| File Path | Description |
|---|---|
| `src/views/music/find.vue` | 发现页容器，只维护 `activeTab`，直接渲染歌单/排行榜两个子组件。 |
| `src/components/Find/PlaylistCategory.vue` | 发现页歌单面板；监听 `LocalUserDetailStore().userSource`，调用 `musicSdk.getPlaylistTags()` 与 `musicSdk.getCategoryPlaylists()` 拉取数据。 |
| `src/components/Find/LeaderBord.vue` | 发现页排行榜面板；监听 `localUserStore.userSource.source`，调用 `musicSdk.getLeaderboards()` 拉取数据。 |
| `src/components/Find/LeaderBordCard.vue` | 排行榜卡片展示组件。 |
| `src/services/musicSdk.ts` | 前端音乐 SDK 包装；`getSource()` 在未选源时回退到内置默认源 `kw`，所有发现页请求最终走这里。 |
| `src/store/LocalUserDetail.ts` | 用户/音源选择状态；新用户默认 `selectSources: 'wy'`，`userSource` 计算属性暴露当前 `pluginId/source/quality`。 |
| `src/types/userInfo.ts` | `UserInfo` 与 `MusicSource` 类型定义，包含 `selectSources`、`pluginId`、`supportedSources`、`sourceQualityMap`。 |
| `src/store/plugin.ts` | Pinia 插件 store；维护已安装插件列表、当前选择插件，并从后端初始化插件。 |
| `src/components/Settings/PluginSettings.vue` | 插件管理 UI；初始化插件列表，选择 music-source 插件时写入 `userInfo.pluginId/supportedSources/selectSources/selectQuality`；可配置 service 插件。 |
| `src/views/settings/sections/MusicSourceSection.vue` | 音乐源配置 UI；已有“未检测到插件配置”引导，并通过 `switch-category` 跳转插件管理分类。 |
| `src/components/layout/HomeLayout.vue` | 顶栏搜索区和音源切换器；仅在 `userInfo.pluginId + supportedSources` 存在时允许切换插件音源。 |
| `src/bridge/index.ts` | `window.api` 桥接；`music.requestSdk` 调 `service-music-sdk-request`，`plugins.*` 调插件后端/worker。 |
| `src-tauri/src/music_sdk/client.rs` | 后端 SDK 请求入口；`handle_request()` 未传 `source` 时默认 `kw`。 |
| `src-tauri/src/music_sdk/sources/mod.rs` | 内置音源分发；支持 `kw/bd/wy/kg/tx/mg/xm/git`，未知源返回空 stub。 |
| `src-tauri/src/plugin/types.rs` | 后端插件类型；`plugin_type` 默认为 `music-source`，可为 `service`。 |
| `src-tauri/src/plugin/manager.rs` | 插件加载/配置持久化；`initialize()` 从 app data 的 `plugins` 目录加载全部插件；`get_config()` 读 `<pluginId>.config.json`。 |
| `src/utils/plugin/PluginRunner.ts` | 前端插件 worker 代理；`testConnection()`、`callMethod()` 会读取插件配置并执行插件导出方法。 |
| `src/utils/plugin/pluginWorker.ts` | 插件 worker；`testConnection()` 调插件的 `testConnection` 或 `ping`，并注入保存配置。 |
| `src/views/settings/index.vue` | 设置页分类路由；支持通过 `/settings?category=plugins` 或 `category=music&section=music-source` 打开目标设置分类/定位分区。 |
| `src/router/index.ts` | 路由定义；设置页路径为 `/settings`，发现页路径为 `/home/find`。 |
| `.trellis/spec/frontend/state-management.md` | 状态管理规范；页面级服务端数据保留本地状态和手动缓存，源变化需失效缓存。 |
| `.trellis/spec/frontend/component-guidelines.md` | 组件规范；Vue 3 `<script setup lang="ts">`、Composition API、TDesign、scoped CSS、`storeToRefs` 等约定。 |

### Code Patterns

#### 发现页当前结构

`src/views/music/find.vue:1-5` 只导入两个子组件并维护 tab：

```ts
import { ref } from 'vue'
import LeaderBord from '@/components/Find/LeaderBord.vue'
import PlaylistCategory from '@/components/Find/PlaylistCategory.vue'

const activeTab = ref<'songlist' | 'leaderboard'>('songlist')
```

模板在 `src/views/music/find.vue:32-36` 无条件挂载两个内容组件，其中歌单组件用 `v-show`，排行榜外层也用 `v-show`：

```vue
<PlaylistCategory v-show="activeTab === 'songlist'" />
<div v-show="activeTab === 'leaderboard'" class="leaderboard-pane">
  <LeaderBord />
</div>
```

由于 `v-show` 不阻止组件挂载，两个子组件的 `onMounted()`/watcher 都会在发现页进入时运行。

#### 歌单数据 fetch 流程

`src/components/Find/PlaylistCategory.vue:8-10` 读取 `LocalUserDetailStore()` 并通过 `storeToRefs` 获取 `userSource`：

```ts
const localUserStore = LocalUserDetailStore()
const { userSource } = storeToRefs(localUserStore)
```

`fetchTags()` 在 `src/components/Find/PlaylistCategory.vue:34-43` 调 `musicSdk.getPlaylistTags()`，失败时仅打印错误。

`fetchCategoryPlaylists()` 在 `src/components/Find/PlaylistCategory.vue:46-102`：

- reset 时设置 `page=1/noMore=false/loading=true`。
- 缓存 key 使用 `(activeTagId.value || 'hot') + ':' + (userSource.value.source || 'kw')`（第 52、57 行）。
- 实际请求在 `src/components/Find/PlaylistCategory.vue:70`：

```ts
const res = await musicSdk.getCategoryPlaylists('hot', activeTagId.value, page.value, limit.value)
```

- 成功后将 `res.list` 映射为本地 card 数据并写入 `playlists`（第 74-85 行）。
- 失败时设置 `error.value = '获取分类歌单失败，请稍后重试'`（第 95-98 行）。

挂载逻辑在 `src/components/Find/PlaylistCategory.vue:158-172`：

```ts
onMounted(() => {
  watch(
    userSource,
    () => {
      loading.value = true
      error.value = ''
      Object.keys(categoryCache).forEach(k => delete categoryCache[k])
      fetchTags().then(() => {
        activeTagId.value = ''
        activeCategoryName.value = '热门'
        fetchCategoryPlaylists(true)
      })
    },
    { deep: true, immediate: true }
  )
  ...
})
```

这意味着首次挂载和任意 `userSource` 深层变化都会清空缓存并立即请求标签与歌单。

#### 排行榜数据 fetch 流程

`src/components/Find/LeaderBord.vue:10-18` 同样读取 `LocalUserDetailStore()`，并通过 `musicSdk.getLeaderboards()` 请求：

```ts
const localUserStore = LocalUserDetailStore()
const currentSource = computed(() => localUserStore.userSource.source)

const fetchBoards = async () => {
  loading.value = true
  try {
    const res = await musicSdk.getLeaderboards()
    boards.value = Array.isArray(res?.list) ? res.list : Array.isArray(res) ? res : []
  } catch (e) {
    console.error('获取排行榜失败:', e)
    boards.value = []
  } finally {
    loading.value = false
  }
}
```

`src/components/Find/LeaderBord.vue:40-41` 在源变化和挂载时请求：

```ts
watch(() => localUserStore.userSource.source, () => fetchBoards())
onMounted(() => fetchBoards())
```

点击榜单进入歌单详情时，`src/components/Find/LeaderBord.vue:27-38` 将 `source: board.source || currentSource.value` 带入 query。

#### 默认源行为（导致未配置时仍显示默认数据）

前端 `musicSdk` 有两层默认源：

1. `src/services/musicSdk.ts:90-93`：

```ts
function getSource(): string {
  const store = LocalUserDetailStore()
  return store.userSource.source || 'kw'
}
```

2. `src/services/musicSdk.ts:115-121` 在请求时使用 `args.source || getSource()`：

```ts
const source = args.source || getSource()
const result = await invoke('service_music_sdk_request', {
  method,
  args: { ...args, source }
})
```

同时 `LocalUserDetailStore.init()` 在没有本地 `userInfo` 时写入默认 `selectSources: 'wy'`，见 `src/store/LocalUserDetail.ts:53-58`：

```ts
userInfo.value = {
  lastPlaySongId: null, topBarStyle: false, mainColor: '#00DAC0',
  volume: 80, currentTime: 0, selectSources: 'wy', sourceQualityMap: {}, hasGuide: false
}
```

后端还有第三层默认：`src-tauri/src/music_sdk/client.rs:230-232`：

```rust
let source = args.get("source").and_then(|v| v.as_str()).unwrap_or("kw").to_string();
crate::music_sdk::sources::dispatch(&source, method, args).await
```

内置源分发在 `src-tauri/src/music_sdk/sources/mod.rs:12-23`，`kw/bd/wy/kg/tx/mg/xm/git` 都会进入内置实现。因此在无插件、无 Subsonic 配置时，发现页仍可能通过默认 `wy`（新用户）或 `kw`（空 source）显示内置源数据。

#### 当前源选择状态

`src/store/LocalUserDetail.ts:131-135` 暴露 `userSource`：

```ts
const userSource = computed(() => ({
  pluginId: userInfo.value.pluginId,
  source: userInfo.value.selectSources,
  quality: (userInfo.value.sourceQualityMap || {})[userInfo.value.selectSources as string] || userInfo.value.selectQuality
}))
```

`src/types/userInfo.ts:6-20` 定义对应字段：

```ts
export interface UserInfo {
  selectSources?: string
  selectQuality?: string
  pluginId?: string
  pluginName?: string
  supportedSources?: Record<string, MusicSource>
  sourceQualityMap?: Record<string, string>
  hasGuide?: boolean
  [key: string]: any
}
```

顶栏音源下拉在 `src/components/layout/HomeLayout.vue:70-77` 用 `pluginId + supportedSources` 判断是否存在插件音源数据：

```ts
const hasPluginData = computed(() => {
  const LocalUserDetail = LocalUserDetailStore()
  return !!(
    LocalUserDetail.userInfo.pluginId &&
    LocalUserDetail.userInfo.supportedSources &&
    Object.keys(LocalUserDetail.userInfo.supportedSources).length > 0
  )
})
```

`src/components/layout/HomeLayout.vue:105-126` 的 `selectSource()` 若 `!hasPluginData.value` 直接返回，说明当前 UI 上的插件音源切换依赖此判断。

#### 插件 store/state

`src/store/plugin.ts:36-41` setup-style Pinia store 维护：

```ts
const plugins = ref<LoadedPlugin[]>([])
const loading = ref(false)
const currentPluginId = ref('')
const currentPluginName = ref('')
```

插件类型接口见 `src/store/plugin.ts:18-24`：

```ts
export interface LoadedPlugin {
  plugin_id: string
  plugin_name: string
  plugin_info: PluginInfo
  supported_sources: PluginSource[]
  plugin_type: 'music-source' | 'service'
}
```

`initialize()` 在 `src/store/plugin.ts:60-121` 调 `window.api.plugins.initialize()`，成功后写入 `plugins.value = res.data || []`，再加载 localStorage 中的 `pluginId/pluginName` 选择；如果已有当前插件并且有 `supported_sources`，会同步 `LocalUserDetailStore().userInfo.supportedSources/selectSources/selectQuality`。

`refresh()` 在 `src/store/plugin.ts:123-135` 调 `window.api.plugins.getList()` 并写入 `plugins.value`。

判断 service 插件的方法在 `src/store/plugin.ts:153-155`：

```ts
function isServicePlugin(plugin: LoadedPlugin): boolean {
  return plugin.plugin_type === 'service'
}
```

因此“已安装 music-source 插件”可由 `plugins.value.filter(p => p.plugin_type === 'music-source')` 或 `!isServicePlugin(p)` 判断；只看 `userInfo.pluginId` 不等价于“已安装”，因为用户可能安装但未选择插件，或选择了无可用音源插件。

当前插件 store 只在 `src/components/Settings/PluginSettings.vue` 初始化：`onMounted(() => { doRefresh() })`（第 326-328 行），`doRefresh()` 内调用 `await store.initialize()`（第 330-334 行）。全局 `App.vue` 只初始化 `LocalUserDetailStore` 与 auth，没有初始化插件 store（`src/App.vue:27-41`）。因此发现页若要依赖 `usePluginStore().plugins` 判断已安装插件，需要确保插件列表已初始化/刷新，否则首次进入发现页时 store 可能仍为空。

#### 插件管理选择行为

`src/components/Settings/PluginSettings.vue:382-434` 的 `doSelect(plugin)`：

- 调 `store.selectPlugin(plugin)` 持久化 `pluginId/pluginName` 到 localStorage。
- 若 `supported_sources` 空，则写入 `userInfo.pluginId/pluginName`，但 `supportedSources = {}`、`selectSources = ''`、`selectQuality = ''`（第 387-394 行）。
- 若存在 `supported_sources`，则转为 `supportedSourcesForStore`，key 为 `src.source_id || src.name`，并选择前一个有效源或第一个源、前一个有效音质或最后一个音质，最后写入 `userInfo.pluginId/pluginName/supportedSources/selectSources/selectQuality`（第 397-431 行）。

卸载当前插件时，`src/components/Settings/PluginSettings.vue:443-451` 会清空 `userInfo.pluginId/pluginName/supportedSources/selectSources/selectQuality`。

#### Subsonic 配置/状态

代码库内没有独立的 `subsonic` store、内置 Subsonic source 文件或显式 `Subsonic` 前端组件命名；全文搜索仅发现通用插件配置逻辑和后端连接测试中对 `serverUrl` 与 `/rest/ping?f=json` 的处理。

现有 Subsonic 相关路径表现为“service 插件配置”：

- `src/components/Settings/PluginSettings.vue:70-84`：service 插件显示“配置”和“导入歌单”按钮，music-source 插件显示“使用”按钮。
- `src/components/Settings/PluginSettings.vue:511-523`：`openConfig(plugin)` 通过 `store.getConfigSchema(plugin.plugin_id)` 获取 schema，再通过 `store.getConfig(plugin.plugin_id)` 读取保存配置，并填入 `configValues`。
- `src/components/Settings/PluginSettings.vue:529-542`：`doSaveConfig()` 校验 required 字段非空后调用 `store.saveConfig(configPluginId.value, JSON.parse(JSON.stringify(configValues.value)))`。
- `src/components/Settings/PluginSettings.vue:545-550`：`doTestConnection()` 先保存配置，再调用 `store.testConnection(configPluginId.value)`。
- `src-tauri/src/plugin/manager.rs:178-187`：后端 `get_config(plugin_id)` 读取 `<pluginId>.config.json`，不存在则返回 `{}`。
- `src-tauri/src/plugin/manager.rs:209-222`：后端通用 `test_connection()` 从配置里取 `serverUrl`，为空则返回“未配置服务器地址”，否则请求 `{serverUrl}/rest/ping?f=json`。
- 前端 worker 的实际测试路径在 `src/utils/plugin/pluginWorker.ts:784-816`：优先调用插件导出的 `testConnection`，没有则调用 `ping`，调用时通过 `options.injectConfig` 注入保存配置。

据此，当前代码可确定“Subsonic 已配置”的通用条件只能从 service 插件及其保存配置判断：存在 `plugin_type === 'service'` 的插件，并且其 `getConfig(plugin_id)` 返回的配置至少包含非空 `serverUrl`；如果要严格确认可用，还需调用 `store.testConnection(plugin_id)` 或插件自己的 `testConnection/ping`。当前代码没有单独字段记录“Subsonic API configured”。

#### 已安装 music-source 插件判断

后端插件类型默认值为 music-source：`src-tauri/src/plugin/types.rs:44-56`：

```rust
pub struct LoadedPlugin {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_info: PluginInfo,
    pub supported_sources: Vec<PluginSource>,
    #[serde(default = "default_plugin_type")]
    pub plugin_type: String,
}

fn default_plugin_type() -> String {
    "music-source".to_string()
}
```

后端初始化从插件目录加载所有插件并返回 `LoadedPlugin`：`src-tauri/src/plugin/manager.rs:27-78`。前端通过 `src/store/plugin.ts:60-66` 接收列表。因此判断“已安装 music-source 插件”的直接数据源是：

```ts
const musicSourcePlugins = pluginStore.plugins.filter(p => p.plugin_type === 'music-source')
const hasInstalledMusicSourcePlugins = musicSourcePlugins.length > 0
```

如果要排除无可用音源的 music-source 插件，可进一步要求 `p.supported_sources?.length > 0`；但用户需求文字是“no installed music-source plugins”，不是“no selected/usable source”，所以基础判断应以 `plugin_type === 'music-source'` 为准，并明确是否需要 `supported_sources.length` 由实现阶段确认。

#### 现有空/引导 UI 与导航/action 模式

可复用/对齐的模式：

- `src/components/Find/PlaylistCategory.vue:335-342` 已有发现页空状态，包含 `.empty-container`、`.empty-orb`、标题和说明。
- `src/components/Find/LeaderBord.vue:65-68` 排行榜空状态是 `.empty-state` + 文本 + `t-button variant="text"` 重试。
- `src/views/settings/sections/MusicSourceSection.vue:215-227` 已有插件配置引导：图标、标题“未检测到插件配置”、说明和 `t-button theme="primary"`，点击 `goPlugin()` emit `switch-category`。
- `src/components/Settings/PluginSettings.vue:31-35` 插件列表为空状态：`t-icon name="app"`、提示“暂无已安装的插件”、hint。
- `src/views/settings/index.vue:166-180` 支持 query 驱动设置页：`category=plugins` 可切到插件管理，`category=music&section=music-source` 可定位音乐源分区。
- `src/router/index.ts:45-48` 设置页路由为 `/settings`。

因此发现页引导按钮可使用现有路由模式，例如跳转到：

```ts
router.push({ path: '/settings', query: { category: 'plugins' } })
```

若要直接去音乐源配置，则可用：

```ts
router.push({ path: '/settings', query: { category: 'music', section: 'music-source' } })
```

### Exact Current Source Detection/Data Fetch Behavior

1. 发现页本身没有源可用性检测。
2. 发现页两个子组件进入页面即挂载，因此歌单和排行榜都会尝试请求。
3. 歌单请求链路：`PlaylistCategory.vue` watcher（immediate）→ `fetchTags()` → `musicSdk.getPlaylistTags()` → `musicSdk.request('getPlaylistTags')` → Tauri `service_music_sdk_request` → Rust `client::handle_request()` → `sources::dispatch(source, method, args)`。
4. 歌单列表请求链路：`fetchCategoryPlaylists(true)` → `musicSdk.getCategoryPlaylists('hot', activeTagId, page, limit)` → 同一后端分发链路。
5. 排行榜请求链路：`LeaderBord.vue` `onMounted()` + `watch(source)` → `musicSdk.getLeaderboards()` → 同一后端分发链路。
6. 源值优先级：显式 `args.source` → `LocalUserDetailStore().userSource.source` → 前端 fallback `'kw'` → 后端 fallback `'kw'`。
7. 新用户 `LocalUserDetailStore.init()` 默认写入 `selectSources: 'wy'`，因此通常无需触发 `kw` fallback 也会请求内置网易云源。
8. `PlaylistCategory.vue` 缓存 key 也使用 `userSource.value.source || 'kw'`，空源时会把数据缓存到 `hot:kw` 这类 key。

### How to Determine Subsonic Configured

当前代码没有专用 Subsonic store/flag。可从现有插件体系中按以下数据路径判断：

1. 确保插件列表已初始化：`await usePluginStore().initialize()` 或已有列表已加载。
2. 找到 service 插件：`plugin.plugin_type === 'service'`。
3. 对候选 service 插件调用 `await pluginStore.getConfig(plugin.plugin_id)`。
4. 以保存配置中存在非空 `serverUrl` 作为“已填写 Subsonic API 地址”的最低判断；这是后端 `test_connection()` 使用的字段名。
5. 若任务语义要求“可连接/有效配置”，再调用 `await pluginStore.testConnection(plugin.plugin_id)`，成功条件为 `{ success: true }`。注意这会产生网络请求，适合作为显式测试或异步状态刷新，不适合每次渲染同步计算。

不能仅凭 `plugin_type === 'service'` 判断已配置，因为 service 插件可存在但配置为空；后端 `get_config()` 在无配置文件时返回 `{}`。

### How to Determine Installed Music-Source Plugins

1. 数据源：`usePluginStore().plugins`。
2. 初始化：当前只有插件管理页面会调用 `store.initialize()`；发现页若使用该 store，需在发现页或新的可复用逻辑中初始化/刷新，避免把“未加载”误判为“未安装”。
3. 判断表达式：

```ts
const hasInstalledMusicSourcePlugins = pluginStore.plugins.some(
  plugin => plugin.plugin_type === 'music-source'
)
```

4. 如果实现选择“可用音源插件”而非“已安装 music-source 插件”，则可改为同时要求：

```ts
plugin.plugin_type === 'music-source' && plugin.supported_sources?.length > 0
```

但这与需求措辞存在差异，需要主实现者确认。

### Likely Files to Modify

| File Path | Why likely |
|---|---|
| `src/views/music/find.vue` | 最适合在父级统一判断“无 Subsonic 配置且无 music-source 插件”并在此时阻止两个子组件挂载/请求，改为显示引导 UI。 |
| `src/store/plugin.ts` | 可能需要新增“是否已初始化/已加载”状态或便捷 getter（例如 music-source 插件、service 插件配置检查），因为当前只有 `loading/plugins/currentPluginId`，没有 initialized 标志。 |
| `src/components/Find/PlaylistCategory.vue` | 如果选择在子组件级防请求，需要在 watcher/fetch 前增加 guard；但父级用 `v-if` 不挂载更直接。 |
| `src/components/Find/LeaderBord.vue` | 同上，父级不挂载可避免排行榜请求；子级 guard 是备选。 |
| `src/views/settings/index.vue` / `src/router/index.ts` | 通常不需要改；已有 `/settings?category=plugins` query 模式可直接复用。 |

### Recommended Implementation Approach

1. 在 `src/views/music/find.vue` 引入 `computed/onMounted/ref`、`useRouter`、`usePluginStore`。
2. 进入发现页时初始化插件列表（如 `await pluginStore.initialize()`），并维护一个发现页本地 `sourceStateLoading`，避免插件列表未加载前闪现“无源引导”。若在 `src/store/plugin.ts` 增加 initialized 标志，则可复用 store 状态。
3. 基于插件列表计算：
   - `hasInstalledMusicSourcePlugins = plugins.some(p => p.plugin_type === 'music-source')`
   - `servicePlugins = plugins.filter(p => p.plugin_type === 'service')`
4. 异步读取 service 插件配置：对每个 service 插件调用 `pluginStore.getConfig(plugin_id)`；若任一配置有非空 `serverUrl`，则 `hasSubsonicConfig = true`。若只想识别 Subsonic 服务插件，可结合 `plugin_info.name/plugin_name` 包含 `subsonic` 或配置 schema 字段，但当前代码没有稳定专用标识，`serverUrl` 是现有连接测试使用的字段。
5. 计算 `shouldShowSetupGuide = !sourceStateLoading && !hasSubsonicConfig && !hasInstalledMusicSourcePlugins`。
6. 在 `find.vue` 模板中：
   - `sourceStateLoading` 时显示轻量 loading/骨架。
   - `shouldShowSetupGuide` 时显示设置引导（标题说明 + 前往插件管理/配置 Subsonic 的按钮）。
   - 否则才渲染 tab 和 `<PlaylistCategory>`/`<LeaderBord>`。
7. 关键点：将现有子组件包在 `v-if="!shouldShowSetupGuide"` 分支内，而不是只在子组件上加 `v-show`。这样无源状态不会挂载 `PlaylistCategory` 和 `LeaderBord`，也就不会触发默认 `wy/kw` 请求。
8. 引导按钮导航可复用现有设置页 query：

```ts
router.push({ path: '/settings', query: { category: 'plugins' } })
```

也可提供“音乐源设置”按钮：

```ts
router.push({ path: '/settings', query: { category: 'music', section: 'music-source' } })
```

9. 如果用户安装/配置后返回发现页，需要在 `onActivated` 或 route 回到页面时重新初始化/刷新插件状态，或监听插件 store 变化后重新检查配置。当前发现页不是 keep-alive 明确包裹，但子组件有 `onActivated/onDeactivated`，所以实现时应注意页面缓存场景。

### Related Specs

- `.trellis/spec/frontend/component-guidelines.md` — Vue 3 SFC、Composition API、TDesign、scoped CSS、`storeToRefs`、导航/监听 cleanup 约定。
- `.trellis/spec/frontend/state-management.md` — 页面级 fetched data 保持本地状态；共享插件/用户源状态用 Pinia；源变化需失效缓存；异步工作使用 loading/error。
- `.trellis/spec/frontend/directory-structure.md` — 前端目录职责（views/components/store/services 等）。

## Caveats / Not Found

- 未找到独立的 `Subsonic` store、`subsonic` 组件、内置 Subsonic source 文件或专用配置 flag；当前只能通过 service 插件配置（特别是 `serverUrl`）推断 Subsonic API 是否已配置。
- `usePluginStore().plugins` 在应用启动时不会全局初始化；发现页直接读取可能得到空数组，必须先初始化/刷新或增加 initialized 标志。
- 需求中的“has not configured Subsonic API”和“has no installed music-source plugins”是 AND 条件；若存在未配置的 service 插件但没有 music-source 插件，应显示引导。
- “installed music-source plugins”是否要求 `supported_sources.length > 0` 未在现有代码或需求中明确；现有插件管理允许选择无可用音源插件，但会清空 `selectSources`。
- 内置源 fallback 同时存在于前端 `musicSdk.ts` 和后端 `client.rs`；本任务目标是发现页不显示默认源数据，最小影响做法是在发现页无源状态阻止请求，而不是改全局 fallback。
