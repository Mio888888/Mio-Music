# Mio Music 独立官网 — 设计文档

- 日期：2026-06-27
- 参考实现：`/Users/vant/Documents/开发/Mio/OpsBatch/website`
- 目标项目：Mio Music（Tauri 2 + Vue 3 跨平台桌面音乐播放器，仓库 `Mio888888/Mio-Music`，当前版本 `0.2.8`）

## 1. 目标与范围

为 Mio Music 构建一个**独立静态官网**，与现有 Tauri/Vue 桌面应用解耦：不引用 `src/`、`public/`、`dist/`，不影响桌面端构建。整体结构、技术形态与视觉骨架对齐 OpsBatch 官网，但内容、配色、文案专为 Mio Music 重写。

**已确认的范围决策（来自 brainstorming）：**

1. 截图轮播采用多图位结构，图片路径先指向 `home.png` 占位，用户后续替换为真实截图。
2. 只生成静态文件，**不**生成 GitHub Actions 发布 workflow，**不**改动现有 `build.yml` 或 release 产出。
3. 视觉：深色 `#0B0F19` 基底 + 紫青渐变强调（`#a855f7` 紫 → `#22d3ee` 青）。
4. 页面板块完整对齐 OpsBatch（Nav→Hero→轮播→痛点→能力→聆听流程→架构→一体化能力表→隐私/本地优先→下载→Footer）。
5. 实现路径 **Option A**：复用本地 vendor（Tailwind cdn js + Iconify 本地图标包 + JetBrains Mono / DotGothic16 字体），离线可用，与 OpsBatch 可靠性一致。

**非目标：** 不做 GitHub Actions 自动发布；不接入或修改 updater 的 `latest.json` 发布物；不提供除静态文件以外的部署。

## 2. 文件布局

```
website/
├── index.html              # SEO、结构化数据与静态页面标记
├── README.md               # 本地预览与下载/版本机制说明
├── .nojekyll               # 让 GitHub Pages 原样发布
├── robots.txt              # Allow: / + Sitemap
├── sitemap.xml             # 单 URL
├── site.webmanifest        # PWA manifest
└── assets/
    ├── app-icon.png        # 从音乐项目 public/icon.png 复制
    ├── site.css            # 字体、背景、入场与轮播样式
    ├── site-data.js        # i18n、下载源与页面状态数据
    ├── site-ui.js          # 轮播、语言切换与设备识别
    ├── site-downloads.js   # release 获取与安装包匹配
    ├── site-init.js        # 页面初始化入口
    ├── screenshots/
    │   └── home.png        # 占位图，从音乐项目 public/home.png 复制
    └── vendor/             # 从 OpsBatch/website/assets/vendor 复制
        ├── tailwind/tailwindcss.js
        ├── iconify/local-icons.js
        └── fonts/ (dotgothic16-regular.ttf, jetbrainsmono-{300,400,500,600,700}.ttf)
```

**隔离原则：** `website/` 内所有引用均为相对路径（`./assets/...`），不向 `../` 上溯。`index.html` 内硬编码保底版本 `v0.2.8`，不与桌面端版本自动耦合。

**仓库与 URL：**
- GitHub 仓库：`Mio888888/Mio-Music`
- canonical / og URL：`https://mio888888.github.io/Mio-Music/`
- 下载源：`https://github.com/Mio888888/Mio-Music/releases/latest`

**hero-poster 处理：** OpsBatch 用了一张 `hero-poster.jpg` 背景大图。为避免引入二进制生成步骤，本设计**不**生成 hero-poster，改为纯 CSS（grid-bg 网格 + 三个 ambient-orb 光晕）作为 Hero 背景。`<img>` hero-poster 节点删除。

## 3. 视觉系统

- **画布：** `body` `bg-[#0B0F19] text-white font-mono`，`selection:bg-purple-500/30`。
- **字体：** 正文 `font-mono`（JetBrains Mono），大标题与数字 `font-dot`（DotGothic16）—— 与 OpsBatch 字体习惯一致。
- **强调色（取代 OpsBatch 的 emerald 绿）：** 紫青渐变 `#a855f7` → `#22d3ee`。用于：
  - Hero 徽标脉冲点 `bg-purple-400`
  - 主 CTA 按钮 `bg-purple-500`（hover `bg-purple-400`），阴影 `shadow-[0_0_30px_rgba(168,85,247,0.35)]`
  - 次级 CTA / 边框 `border-purple-500/20`、`text-purple-300`
  - 轮播缩略图选中环 `border-cyan-400/55`、`bg-cyan-400/12`、圆点 `bg-cyan-400`
  - 下载「推荐」徽章 `text-cyan-300`
  - ambient-orb 三球：`bg-purple-500/10`、`bg-cyan-500/10`、`bg-pink-500/[0.07]`
- **`theme-color` / manifest：** `#0B0F19`。
- **动效：** 复用 `reveal` / `reveal-left` / `reveal-right` / `reveal-fade` 的 IntersectionObserver 入场与轮播切换过渡，全部在 `prefers-reduced-motion: reduce` 下禁用。

## 4. 板块内容（对齐 OpsBatch 结构，专为 Mio Music 重写）

| # | 板块 | 内容 |
|---|------|------|
| Nav | 导航 | Logo（app-icon.png + 「Mio Music」）+ 锚点（能力/聆听流程/架构/隐私/下载）+ CN/EN 切换 + GitHub 链接 + 「免费下载」按钮 |
| Hero | 首屏 | 徽标「开源免费 · 本地优先 · 跨平台」；H1「原生高保真桌面音乐播放器」；副标：基于 CeruMusic 重构，Tauri 2 (Rust) 后端 + Vue 3 前端，插件架构接入多音乐源；CTA「免费下载桌面版 →」/「查看源码」；三数据：当前版本 `v0.2.8` / 支持格式 `8+` / 本地数据 `100%` |
| Carousel | 截图轮播 | 多图位结构（与 OpsBatch 同款主图 + 缩略图列 + 左右翻页 + 键盘焦点），图片路径先指向 `./assets/screenshots/home.png` 占位。图位标题/说明：①播放主界面 ②Apple Music 风格动态歌词 ③10 段均衡器与音效 ④频谱可视化 ⑤下载管理 ⑥AI 助手 |
| Pain | 痛点 | 标题「音乐播放器够多，但音质与体验总差一口气」。三组对照：①「网页播放器压缩音质、无均衡器」→「Rust 原生音频引擎 + 10 段参数均衡器 + 低音增强 / 环绕声」；②「歌词只能逐行、没有翻译」→「LRC / YRC / QRC / TTML 多格式 + 翻译合并 + Apple Music 风格动态歌词」；③「音源被锁死在单一平台」→「插件系统 + Subsonic 自托管 + 本地音乐库扫描」 |
| Features (能力) | 能力卡片网格 | 6 张卡片：①原生音频引擎（Rodio + cpal + Symphonia，AAC/MP3/FLAC/WAV/OGG）；②歌词系统（LRC/YRC/QRC/TTML、翻译合并、桌面悬浮歌词窗）；③均衡器与音效（10 段参数 EQ、8 预设、低音增强 / 环绕声 / 立体声平衡、无缝播放 / Crossfade）；④插件音源（澜音 / 洛雪插件 + Subsonic，动态安装 / 配置 / 测试）；⑤专辑封面主题（自动提取主色、生成 50+ CSS 变量、明暗模式适配、PixiJS 着色器背景 + Three.js 粒子欢迎页）；⑥下载与数据（下载管理器暂停 / 恢复 / 批量、S3 AES-GCM 加密备份、DLNA 投屏、SQLite 本地库） |
| Workflow (聆听流程) | 四步流程 | ①选择音源（插件 / Subsonic / 本地库）→ ②调校音质（10 段 EQ + 预设 + 低音 / 环绕）→ ③沉浸歌词（动态歌词 / 翻译 / 桌面悬浮）→ ④管理与同步（下载 / S3 备份 / DLNA 投屏） |
| Architecture | 技术架构 | 标题「本地优先高保真架构」。前端 Vue 3 + TypeScript + Vite 6；后端 Rust + Tauri 2；核心数据持久化在本地 SQLite，无需云服务即可运行。数据流：Rust 后端 → 音频引擎 / SQLite / 插件 / 网络。底部能力条：Rodio · cpal · Symphonia · rusqlite · rustfft · biquad |
| All-in-one | 一体化能力表 | 表格三列（能力 / 技术 / 状态），行：音频格式支持（AAC/MP3/FLAC/WAV/OGG）、均衡器预设（8 内置）、歌词格式（LRC/YRC/QRC/TTML）、插件音源（澜音/洛雪 + Subsonic）、专辑色主题（50+ CSS 变量）、下载管理（暂停/恢复/批量）、S3 加密备份（AES-GCM + 密码）、DLNA 投屏、自动更新（Ed25519 签名）、国际化（中 / English） |
| Privacy / Local | 隐私 · 本地优先 | 标题「本地优先，音乐库在你手里」。音乐库 / 播放列表 / 设置存本地 SQLite；S3 备份采用 AES-GCM 加密 + 密码保护，密钥由用户自管；插件数据合法性由插件提供者与用户负责。声明：本项目不直接获取 / 存储 / 传输任何音乐数据，仅提供插件运行框架。四个小徽标：无云依赖 / 加密备份 / 签名自动更新 / 开源免费 |
| Download | 下载 | 同 OpsBatch：识别系统（macOS / Windows / Linux）+ 读取 `latest.json`（jsDelivr + raw.githubusercontent + GitHub API 兜底，指向 `Mio888888/Mio-Music`）+ 主下载按钮 + 三平台卡片（.dmg / .app.tar.gz · .exe / .msi · .AppImage / .deb / .rpm）+ GitHub Star。保底版本 `v0.2.8` |
| Footer | 页脚 | 简介 + GitHub / Release / Issue 图标 + 产品 / 资源 / 源码 三列链接 + 版权「© 2026 Mio Music · 开源免费 · 由 Mio 维护」+ 技术栈条「Tauri 2 + Vue 3 + Rust」 |

## 5. SEO / 结构化数据 / i18n

- **`<head>` meta：** title、description、keywords（音乐播放器、桌面音乐、无损、均衡器、歌词、Tauri、Vue 3、跨平台）、author=Mio、theme-color、application-name、og:title/description/type/url/image/locale、twitter:card，canonical。
- **JSON-LD `@graph`：**
  - `SoftwareApplication`（name=Mio Music，applicationCategory=`MusicApplication`，operatingSystem=`Windows, macOS, Linux`，softwareVersion=`0.2.8`，price=0，featureList=6 项，inLanguage=[zh-CN, en]）
  - `Organization`（sameAs=GitHub 仓库）
  - `WebSite`
  - `FAQPage`（3 问：是否免费 / 支持哪些系统 / 包含哪些核心能力）
- **i18n：** 复用 OpsBatch 的 `i18n.en.text` + `i18n.en.attrs` 翻译图机制。所有第 4 节中文字符串均有对应英文条目（以 `README_EN.md` 为文案基线）。切换时重写 `document.title`、meta description、og 标签、轮播文案与全部可见文本。`data-lang-toggle="zh"/"en"` 按钮组。

## 6. 下载 / 版本检测逻辑（适配音乐仓库）

与 OpsBatch 脚本结构相同，重定向到音乐仓库：

- `releasesUrl = 'https://github.com/Mio888888/Mio-Music/releases/latest'`
- `latestJsonUrls = ['https://cdn.jsdelivr.net/gh/Mio888888/Mio-Music@main/latest.json', 'https://raw.githubusercontent.com/Mio888888/Mio-Music/main/latest.json']`
- `latestReleaseApi = 'https://api.github.com/repos/Mio888888/Mio-Music/releases/latest'`
- `fallbackVersion = 'v0.2.8'`
- `platformPriority`（macOS：darwin-aarch64 / darwin-x86_64 等；Windows：windows-x86_64 等；Linux：linux-x86_64 等）与 `assetMatchers`（mac：.dmg / .app.tar.gz；win：.exe / .msi；linux：.AppImage / .deb / .rpm）与 OpsBatch 一致。
- CORS 兜底行为一致：依次尝试 jsDelivr → raw → GitHub API（后者返回 CORS 头）；全部失败回退到 releases 页 + 保底版本。

> 注：`tauri.conf.json` 已将 updater endpoint 指向 `.../Mio-Music/releases/latest/download/latest.json`，即 `latest.json` 随 release 发布。官网脚本读取的 `@main/latest.json` 镜像为可选增强项，若未提交到默认分支则由 GitHub API 兜底，不影响下载卡片功能。

## 7. 配套文件

- **`README.md`** — 对齐 OpsBatch：本地预览（`python3 -m http.server 4173`）、目录用途、下载 / 版本机制说明、随 `README.md` / `README_EN.md` 同步内容的提醒。
- **`.nojekyll`** — 空文件。
- **`robots.txt`** — `User-agent: *` / `Allow: /` / `Sitemap: https://mio888888.github.io/Mio-Music/sitemap.xml`。
- **`sitemap.xml`** — 单 URL（`https://mio888888.github.io/Mio-Music/`），lastmod `2026-06-27`。
- **`site.webmanifest`** — name/short_name=`Mio Music`，description=`跨平台桌面音乐播放器`，background_color/theme_color=`#0B0F19`，icon=`./assets/app-icon.png`（512）。

## 8. 资产复制清单

| 目标 | 来源 | 说明 |
|------|------|------|
| `website/assets/app-icon.png` | 音乐项目 `public/icon.png` | 站点 favicon / logo |
| `website/assets/screenshots/home.png` | 音乐项目 `public/home.png` | 轮播占位图 |
| `website/assets/vendor/tailwind/tailwindcss.js` | `OpsBatch/website/assets/vendor/tailwind/tailwindcss.js` | 原样复制 |
| `website/assets/vendor/iconify/local-icons.js` | `OpsBatch/website/assets/vendor/iconify/local-icons.js` | 原样复制 |
| `website/assets/vendor/fonts/*.ttf` | `OpsBatch/website/assets/vendor/fonts/*.ttf` | 7 个字体文件原样复制 |

## 9. 验收标准

1. `website/` 目录存在且包含上列全部文件。
2. `index.html` 内 `<title>` 含「Mio Music」，所有 GitHub 链接指向 `Mio888888/Mio-Music`，保底版本为 `v0.2.8`。
3. `website/` 页面资源中无任何 `OpsBatch` / emerald 绿残留字符串；强调色为紫青。
4. CN/EN 切换可改写全部可见文本、title、meta、og 标签与轮播文案。
5. 下载脚本的 `releasesUrl` / `latestJsonUrls` / `latestReleaseApi` 全部指向 `Mio888888/Mio-Music`。
6. `python3 -m http.server 4173 -d website` 可在 `http://localhost:4173` 正常打开，控制台无 404（vendor / 字体 / 图标路径正确）。
7. 所有资源引用为相对路径，`website/` 不向 `../` 上溯。
8. 未生成 GitHub Actions workflow，未改动 `build.yml`。
