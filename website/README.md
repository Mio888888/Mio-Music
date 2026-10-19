# Mio Music 官网与文档

Mio Music GPUI 的静态官网与文档站。它是纯静态文件，**不属于 Cargo 工作区，不参与软件打包**，只作为展示页面放在仓库里，通过 GitHub Pages 发布。

```text
website/
├─ index.html          官网首页（中英双语，CN / EN 切换）
├─ docs/               文档（仅中文）
│  ├─ index.html
│  ├─ getting-started.html   快速开始
│  ├─ user-guide.html        使用指南
│  ├─ architecture.html      架构概览
│  └─ plugins/               插件开发者文档
│     ├─ index.html          开发概览
│     ├─ quickstart.html     快速上手
│     ├─ list-plugin.html    列表插件参考
│     ├─ data-types.html     数据结构
│     ├─ runtime-api.html    运行时 API
│     ├─ playback-plugin.html 音源插件
│     └─ publishing.html     调试与发布
├─ assets/
│  ├─ site.css, site-*.js    首页样式、翻译数据、交互与下载逻辑
│  ├─ docs.css, docs.js      文档页的样式与外壳（页头、侧栏、目录、翻页）
│  ├─ vendor/                Tailwind 运行时、本地图标子集、字体
│  └─ screenshots/
├─ sitemap.xml, robots.txt, site.webmanifest, .nojekyll
```

## 本地预览

```bash
cd website
python3 -m http.server 4173
```

打开 `http://localhost:4173`。所有资源引用均为相对路径，不需要构建步骤。

## 发布

`.github/workflows/website.yml` 会在 `website/` 有改动并合入 `main` 后，把整个目录发布到 GitHub Pages。首次使用需要在仓库 `Settings → Pages → Build and deployment → Source` 选择 **GitHub Actions**。

发布地址（页面中的 canonical URL 与 sitemap 都使用它）：

```text
https://mio888888.github.io/Mio-Music/
```

> 如果换了仓库名或使用自定义域名，需要同步替换 `index.html`、`docs/**/*.html`、`sitemap.xml`、`robots.txt` 中的该地址。

## 下载区

首页会自动识别访问者的系统，并优先从 GitHub Releases 读取最新版本：

1. GitHub API（`api.github.com/repos/Mio888888/Mio-Music/releases/latest`，返回 CORS 头）
2. 默认分支上的 `latest.json` 镜像（jsDelivr / raw.githubusercontent，若已提交）

读取成功后会同步更新版本号、主下载按钮与 macOS / Windows / Linux 卡片；失败（例如仓库还没有发布 Release）时回退到 Releases 页面，并显示 `assets/site-data.js` 里的 `fallbackVersion`。**发布新版本时记得同步更新它**，以及 `index.html` 中 JSON-LD 的 `softwareVersion` 和英雄区的版本号。

## 编写文档

文档页由 `assets/docs.js` 统一包上外壳：每个页面只需要提供一个 `<article class="doc">`，页头、左侧目录、右侧「本页内容」、上一页 / 下一页、代码复制按钮和语法高亮都会自动生成。

- **新增页面**：复制一个现有页面，改 `<title>`、`<meta description>`、`<body data-root data-page>`，再把它加进 `assets/docs.js` 顶部的 `NAV`，并补进 `sitemap.xml`。`data-root` 是该页回到站点根目录的相对路径（`docs/` 下为 `../`，`docs/plugins/` 下为 `../../`），`data-page` 是它相对 `docs/` 的路径。
- **代码块**：`<pre class="language-js"><code>…</code></pre>`，代码里的 `<`、`>`、`&` 要转义。支持 `language-js`、`language-json`、`language-bash`；给 `<pre>` 加 `data-title="文件名"` 会显示标题栏。
- **提示框**：`<div class="callout note | warn | tip"><strong>标题</strong>正文</div>`。
- **步骤**：`<ol class="doc-steps">`。**卡片**：`<div class="doc-cards"><a class="doc-card">…`。
- 文档内容以代码和仓库 README 为准；插件相关页面的数值（限制、能力名、字段）来自 `crates/mio-plugin` 与 `crates/mio-music-sdk`，这两个 crate 改动契约时请同步修改文档。

## 截图替换

首页截图轮播的图片位于 `assets/screenshots/`，目前 6 个图位都用 `home.png` 占位。换成真实截图时，把图片放入该目录并更新 `index.html` 中对应的 `<img src="./assets/screenshots/...">` 即可。六个图位依次是：播放主界面、Apple Music 风格动态歌词、10 段均衡器与音效、频谱可视化、下载管理、插件管理。

首页的版本、功能范围与合规说明应随仓库根目录的 `README.md` 一起维护，避免官网与实际应用能力脱节。
