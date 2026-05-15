# Journal - Mio (Part 0)

> AI development session journal
> Started: 2026-05-12

---



## Session 1: 均衡器预设下载入口

**Date**: 2026-05-12
**Task**: 均衡器预设下载入口
**Branch**: `main`

### Summary

新增均衡器预设下载入口；清理误提交的 Trellis journal 后按规则重新收尾记录。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `bc40dbe` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 2: 修复搜索页首次不触发搜索及酷我 JSON 解析

**Date**: 2026-05-12
**Task**: 修复搜索页首次不触发搜索及酷我 JSON 解析
**Branch**: `main`

### Summary

修复两个 bug：1) 搜索页首次进入时因 watcher immediate + focus 时序问题导致搜索不触发，改用 onMounted 作为首次搜索入口，onActivated 处理 KeepAlive 重新激活，watcher 同时监听 value 和 focus 支持搜索页内换词。2) 酷我 API 返回的双引号 JSON 被单引号替换破坏，改为先尝试直接解析再 fallback。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `47c8ac6` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 3: 重构搜索页聚合搜索，修复 tab 切换与主题兼容

**Date**: 2026-05-12
**Task**: 重构搜索页聚合搜索，修复 tab 切换与主题兼容
**Branch**: `main`

### Summary

参考 lx-music-desktop 架构重构搜索页：提取 composable 和工具函数（normalize/similarity/deduplicate），新增 Levenshtein 相似度排序和聚合翻页能力。替换 NaiveUI n-tabs 为本地按钮式 tab 修复选中态不切换问题。搜索页样式改用项目主题变量兼容暗色/亮色自动切换。修复歌单详情页不传 source 导致歌曲不加载的 bug。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `ee2add3` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 4: 音源显示权限集中控制

**Date**: 2026-05-13
**Task**: 音源显示权限集中控制
**Branch**: `main`

### Summary

新建 useSourceAccess composable 统一管理音源名称解析、启用检查、数据过滤。替换所有硬编码 sourceNames 映射。插件卸载时 diff supportedSources 自动暂停播放并移除队列曲目。Subsonic 关闭同样处理。首页无音源时显示引导页跳过数据加载。搜索结果过滤未启用音源。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `a9e5e34` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 5: 新增 Android 自动构建 (v0.1.9)

**Date**: 2026-05-13
**Task**: 新增 Android 自动构建 (v0.1.9)
**Branch**: `main`

### Summary

为 Mio Music 新增 GitHub Actions Android APK 自动构建流水线。解决一系列交叉编译问题：crate 重命名避免 mio 库冲突、reqwest 切换 rustls-tls、桌面端专属 API 添加 #[cfg(desktop)] 条件编译（tray/menu/global_shortcut/always_on_top 等）。版本升级到 0.1.9。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `76c292b` | (see git log) |
| `fa1adfa` | (see git log) |
| `f47c714` | (see git log) |
| `547b41f` | (see git log) |
| `6b0958c` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 6: 手机端全屏按钮修复

**Date**: 2026-05-13
**Task**: 手机端全屏按钮修复
**Branch**: `main`

### Summary

修复手机端全屏播放页按钮显示问题，调整移动端样式并通过构建验证。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f328733` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 7: 优化手机端本地音乐库 UI

**Date**: 2026-05-13
**Task**: 优化手机端本地音乐库 UI
**Branch**: `main`

### Summary

重构 local.vue 移动端布局：卡片式歌曲行（歌名+歌手堆叠）、可折叠搜索栏、播放中均衡器动画、长按多选、触摸反馈、封面圆角阴影、优化空状态

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `83e7326` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 8: 优化移动端发现页 hot-tags

**Date**: 2026-05-13
**Task**: 优化移动端发现页 hot-tags
**Branch**: `main`

### Summary

移动端发现页 hot-tags 交互优化：选中标签自动滚动居中、右侧渐变遮罩动态显隐、分类栏 sticky 定位、激活标签视觉增强、scroll-snap mandatory

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `17f44a8` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 9: 优化音效设置移动端 UI

**Date**: 2026-05-13
**Task**: 优化音效设置移动端 UI
**Branch**: `main`

### Summary

优化设置页音效分类的移动端体验，新增均衡器手机端状态摘要、按钮网格、触控友好的频段/参数布局，并重做高级音效卡片的移动端层级。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `eebb4f9` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 10: 发布 v0.2.0

**Date**: 2026-05-13
**Task**: 发布 v0.2.0
**Branch**: `main`

### Summary

将项目版本号更新到 0.2.0，整理 0.1.9 之后的更新内容并完成 release 提交；构建校验通过。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `eda6b06` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 11: 修复 Android APK 签名发布与 Rust warning

**Date**: 2026-05-13
**Task**: 修复 Android APK 签名发布与 Rust warning
**Branch**: `main`

### Summary

修复 Android CI release APK 签名、校验和上传链路，避免发布 unsigned/debug 产物导致 packageInfo is null；同时修复 Rust unused variable 编译警告。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `1153aa9` | (see git log) |
| `d12356b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 12: 优化 Android APK 打包体积

**Date**: 2026-05-13
**Task**: 优化 Android APK 打包体积
**Branch**: `main`

### Summary

分析 153MB APK 体积来源，实施两项优化：1) Cargo profile.release 添加 strip/LTO/codegen-units/panic=abort/opt-level=s；2) CI 构建启用 ABI splits，不再打 universal APK，产出 arm64-v8a 和 x86_64 独立包。预计单架构 APK 降至 40-60MB。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `dcebaf8` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 13: 修复 Android APK 闪退 + CI Android 构建

**Date**: 2026-05-14
**Task**: 修复 Android APK 闪退 + CI Android 构建
**Branch**: `main`

### Summary

1) 修复 Android APK 启动闪退：音频引擎初始化失败不再 panic，PlayerEngine 支持 Option<OutputStreamHandle> 优雅降级。2) CI Android 构建调试：修复 gradle.properties 追加拼接问题、AndroidManifest 权限注入改用 Python、ndk abiFilters 与 splits abi 冲突待解决（ABI 分包已恢复但仍需与 Tauri 生成的 abiFilters 对齐）。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `3e4538c` | (see git log) |
| `a1f9e1b` | (see git log) |
| `29572aa` | (see git log) |
| `fab813a` | (see git log) |
| `657775e` | (see git log) |
| `81c1f5b` | (see git log) |
| `043a665` | (see git log) |
| `1174186` | (see git log) |
| `0b396c9` | (see git log) |
| `b1621b9` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 14: 优化移动端设置 UI、插件弹窗挂载、欢迎页边距、Android 图标修复

**Date**: 2026-05-14
**Task**: 优化移动端设置 UI、插件弹窗挂载、欢迎页边距、Android 图标修复
**Branch**: `main`

### Summary

移动端 A/B 对比模式 UI 适配；插件弹窗 attach=body 避免遮挡；设置页面去掉 t-card 消除三层框嵌套；欢迎页 Mio 文字边距调整；Android 自适应图标安全区修复

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `a370375` | (see git log) |
| `1d97c1d` | (see git log) |
| `ed07762` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 15: 接入 LiquidGlass 重构歌单导入/创建弹窗

**Date**: 2026-05-15
**Task**: 接入 LiquidGlass 重构歌单导入/创建弹窗
**Branch**: `main`

### Summary

将导入歌单、网络导入、创建歌单三个弹窗从 TDesign t-dialog 重构为 LiquidGlass 风格。合并导入方式选择与网络导入为单弹窗，平台选择改为卡片式 UI，导入流程在弹窗内完成。统一移动端底部弹出适配。移除 import-tips。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `9dc9e8f` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 16: fix: Android 发行版封面图片无法加载

**Date**: 2026-05-15
**Task**: fix: Android 发行版封面图片无法加载
**Branch**: `main`

### Summary

Android WebView 不支持自定义 URL scheme (imgproxy://)，导致所有封面图片返回 ERR_UNKNOWN_URL_SCHEME。通过 Chrome DevTools 远程调试定位问题，改用 WRY 的 HTTP workaround 格式 (http://imgproxy.localhost/) 解决。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `f4e0c2b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 17: 提交 Android gen 工程

**Date**: 2026-05-15
**Task**: 提交 Android gen 工程
**Branch**: `main`

### Summary

提交 src-tauri/gen Android 工程与 schemas，并调整 GitHub Actions Android 构建，确保 CI 使用已提交的 gen 内容且构建后校验 src-tauri/gen 未被修改。

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `d9354e6` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete
