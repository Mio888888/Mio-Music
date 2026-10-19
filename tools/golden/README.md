# 原生对照

参考应用为相邻 Mio-Music 快照 `48f551125211eaa0665f37071cdc4df20aaece7d`。`docs/parity/baselines` 保存实际参考窗口 PNG 与通过 Web Inspector 执行 `tools/spec-extract/parity.js` 取得的布局 JSON。采集器只监听 127.0.0.1，接受原版开发 WebView 的指定 Origin；不采集输入框值或用户配置。

已采集 1200×800 浅色、768×800 浅色，以及 1200×800 CSS 暗色探针。暗色探针仅将参考 DOM 的 `data-theme` 改为 `dark` 后采集，不能证明原应用存在完整主题切换流程。窄窗通过原生边缘缩放，并核实实际 innerWidth/innerHeight；Tauri 未允许脚本调整窗口尺寸。

GPUI 采集使用实际窗口、原生文本整形与 Metal 渲染；不会读取用户设置或系统凭据。

```sh
MIO_GPUI_CAPTURE_DIR="$PWD/tools/golden/captures" cargo run --locked -p mio-app --features parity
MIO_GPUI_CAPTURE_DIR="$PWD/tools/golden/captures" MIO_GPUI_CAPTURE_THEME=dark cargo run --locked -p mio-app --features parity
MIO_GPUI_CAPTURE_DIR="$PWD/tools/golden/captures" MIO_GPUI_CAPTURE_WIDTH=768 cargo run --locked -p mio-app --features parity
```

程序等候嵌入图像解码、刷新窗口，保存 PNG 与关键元素 bounds 后退出。采集结果不代表全部元素都已被标记；样式逐值和文字行数门禁仍待补齐。新增或修改状态需要重新采集对应基准。

使用安装了 Pillow 与 numpy 的 Python 运行对比：

```sh
python3 tools/golden/test_compare.py
python3 tools/golden/compare.py \
  --reference docs/parity/baselines/reference-light-1200x800.json \
  --actual tools/golden/captures/gpui-light-1200x800.json \
  --report docs/parity/results/light-1200x800.json
```

对比检查窗口尺寸、DPR、主题、所有采集到的可见参考元素 bounds（≤1px），以及未遮罩、未缩放的整屏 RGB SSIM（11×11、σ=1.5，阈值 0.97）。原版自绘鼠标指针仍在截图中，未排除其区域。图片尺寸不一致、元素缺失、阈值不足均返回退出码 1。报告会保留失败原因，不会降低阈值来使其通过。

一条命令跑一屏（采集、对比、给出 SSIM 损失分布）：

```sh
tools/golden/run.sh                  # 浅色 1200×800
THEME=dark tools/golden/run.sh       # 暗色 1200×800
WIDTH=768 tools/golden/run.sh        # 浅色 768×800
```

`regions.py` 把整屏 SSIM 的损失按区域（侧栏、顶栏、引导区、卡片、播放条各段）统计出“占总损失的份额”和“区域本身的损失”，用于决定先改哪里。

当前三组整体验收均未通过。窄窗 768×800：12 个元素包围盒全部通过（最大 0.016px），整屏 SSIM 0.945（含文字字距补偿后）。暗色 1200×800：包围盒最大偏差 0.39px（通过），整屏 SSIM 0.9530。浅色 1200×800：元素包围盒最大偏差 0.39px（通过），整屏 SSIM 0.9554（门槛 0.97，未通过；含文字字距补偿）；损失集中在两张卡片、播放条左段和侧栏/顶栏里的小段文字（栅格化半像素差与拉丁字母字体宽度）。详见 `docs/parity/progress.md` 和 `docs/parity/results`。普通三平台 CI 尚不运行这些需要真实窗口的黄金检查。

## 其它页面

`tools/golden/page.sh settings|downloads|playlists` 用 WKWebView（`tools/spec-extract/wk_capture.swift`，原版 Tauri 窗口在 macOS 上的同一引擎）渲染原版前端的对应页面作参考，再采集 GPUI 同一页面并对比。需要先在原版仓库启动开发服务器：

```sh
cd ../Mio-Music && npx vite --port 1420 --host 127.0.0.1
```

Tauri 桥由 `tools/spec-extract/tauri_mock.js` 替代（所有命令返回空值），所以只有不依赖后端数据的页面有意义。当前（浅色 1200×800，均未通过 0.97）：设置 0.948（外观分区 25 个块几何全部通过，最大 0.53px）、下载管理 0.966（12 个元素 10 个几何通过，2 个差 1.22px）、本地歌单 0.967（7 个元素几何全部通过）。
