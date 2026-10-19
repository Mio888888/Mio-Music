# GPUI 渲染扩展

本目录保留 crates.io 的 `gpui-fast` 0.1.6、`gpui-fast-apple` 0.1.6 和 `gpui-component` 0.7.2 源包及 Apache-2.0 许可证。根 Cargo.toml 通过 `[patch.crates-io]` 固定使用本地副本；依赖版本保持原样，没有修改 Cargo 全局缓存。上游已有测试代码保持原样，本项目没有新增或运行这些测试。

渲染核心为 [gpui-fast](https://github.com/longbridge/gpui-fast)（Zed GPUI 的渲染与布局优化发行版）：由 `gpui-kit` 0.7.2 的 `gpui-fast` 特性选择，滚动容器内容缓存为图层贴图、保留视图、文本整形缓存和路径批合并均来自它。`gpui-kit` 仍把 `gpui-pre` 0.3.8 作为不可选依赖编译进来，但不再被调用，也不再有本地改动。

本地改动（均为 gpui-pre 时期补丁在 gpui-fast 上的移植）：

- `gpui-fast/src/scene.rs`、`src/fast/scene.rs`：Apple backdrop primitive、场景绘制屏障及 `backdrops` 的排序与批次。
- `gpui-fast/src/window.rs`：Apple `Window::paint_backdrop`，先饱和度后高斯模糊；逻辑半径范围 0–128px。
- `gpui-fast/src/fast/layers/*`：滚动图层只缓存不依赖帧缓冲的内容。含 backdrop 的内容按含路径内容的方式直接画进帧（`paint::reads_frame`），并在下一帧降级，不会被栅格化进贴图。
- `gpui-fast/src/text_system/line_layout.rs`：`set_text_advance_adjustment(wide, narrow)`，在文字排版后给每个字形的步进额外加一个固定像素值（全角字符用 `wide`，其他用 `narrow`），并同步修正行宽；未设置时不改变任何排版。用来对齐原版 WebView 的字距（见 tools/golden）。gpui-fast 的数字整形与近期整形缓存在其上游，调整仍在缓存之后应用。
- `gpui-fast/src/assets.rs`、`src/elements/img.rs`、`src/svg_renderer.rs`、`src/platform.rs`、`Cargo.toml`：动图播放次数、`image_scale`、`on_frame`、APNG/AVIF（dav1d、mp4parse）及 svgz 支持。
- `gpui-fast-apple/src/backdrop.rs`：区域 GPU 快照、纹理缓存、MPS 高斯模糊和透明合成；纹理分配检查空结果，全部成功后替换缓存。
- `gpui-fast-apple/src/metal_renderer.rs`、`gpui_apple.rs`、`shaders.metal`：分批接入、模块及着色器；backdrop 窗口在获取 drawable 前开启读取能力，其他窗口沿用原配置。含 backdrop 的帧不走 `fast/paths.rs` 的路径分组编码，使用渲染器自己的批循环（需要在批之间结束并重开编码器）；图层贴图栅格化不处理 backdrop。
- `gpui-fast-apple/Cargo.toml`：启用现有 metal 0.33 的 `mps` feature，没有添加新依赖版本。
- `gpui-component/src/dialog/dialog.rs`：对话框可选遮罩/底板滤镜、遮罩底色、底部定位及原生关闭按钮样式；显式阴影配置参与原有动画。底板与滤镜共用透明度、可见性及裁剪，默认参数保留原版 Kit 路径。焦点陷阱、取消/关闭/OK 回调和 Base 事件分发未替换。底部定位保持静态，滤镜渐入仍待校准。
- `gpui-component/src/sizing.rs`：`input_h` 的中号为固定 32px（原为 `h_8`，随响应式 rem 变成 33.5px），与 TDesign 输入框一致。
- `gpui-component/src/select.rs`、`searchable_list.rs`、`searchable_list/item.rs`、`searchable_list/adapter.rs`：`DropdownStyle` 与 `Select::menu_style`，可覆盖下拉面板、选项行、间距和选中装饰；自定义尾部图标保留调用方尺寸和颜色，默认 Caret 路径沿用原配置。
- `gpui-component/src/list/list.rs`、`lib.rs`：列表在无选中项时按“上”键选第一项（`select_first_on_initial_up`）；`lib.rs` 的 `init` 追加 `select::init`。

目前调用方为 macOS 播放队列、音量弹层、播放器样式面板及其圆形入口、顶部播放控件、底栏、窄窗队列遮罩、队列操作按钮和清空确认弹窗；Windows/Linux 未接入此渲染扩展。MPS 不支持或纹理分配失败时保留普通面板，不将回退视为玻璃验收通过。升级 gpui-kit 或 GPUI Fast 前应先复核本地改动及绘制顺序。当前视觉证据与验证边界见 `docs/parity/progress.md`。
