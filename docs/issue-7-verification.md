# Issue #7 修复与验证

核对时间：2026-09-13。对应 [Issue #7](https://github.com/Mio888888/Mio-Music/issues/7)。修复版本为 0.3.6，尚未发布安装包。下述功能验证在更新版本号前完成。

## 修复内容

- LX 兼容层按真实平台传入 `desktop` / `mobile`，等待异步 `inited` 与请求处理器就绪，再读取音质和获取播放链接；初始化 15 秒超时。同步执行、定时器及 HTTP 回调中可捕获的初始化错误会向上传递。脚本自行启动且未返回的异步任务仍可能只能通过超时发现失败。
- `send` 返回 Promise，插件的受限全局提供 `BigInt`。运行时音源声明替换默认列表，不添加未声明音源。已安装的本项目 LX 包装在读取时从原始 JSON 重新转换，磁盘文件保持原样。
- Android 本地音乐入口改为系统多选文件，流式复制到应用持久目录，再按实际音频内容识别格式、建立索引；同名文件分开保存，失败清理本次副本，取消不提示导入成功。复制会占用设备空间，界面在选择前提示；清空索引不删除副本，可重新扫描恢复。
- 基础布局提供 `vh` 和固定间距回退，播放器避免只使用动态视口单位，搜索布局允许收缩；构建转换 CSS 原生嵌套规则。Android 原生 WebView 使用系统栏/刘海 Insets 留出边界，消费 Insets 避免重复留白。

## 验证证据

- `node --test scripts/plugin-lx-compat.test.mjs scripts/plugin-store-sources.test.mjs`：29 项通过，包含异步初始化、两端环境、`BigInt`、`send().then()` 和音质/替换竞态回归。LX 测试使用真实 Rust 转换器与 Worker，需先在本机构建 Rust 依赖。
- Issue 公开样本经生产转换器与 Worker，在 desktop/mobile 各五源共 10 组 FLAC 链接解析中通过；HTTP 全部模拟，未请求真实歌曲接口。公开初始化服务的只读核对显示 `nodejs` 返回失败，而 desktop/mobile 返回成功。样本原文仅保存在临时目录，未纳入仓库。
- `cargo test --lib local_music::importer::tests`：3 项通过，覆盖无文件名、重名/路径边界、音频内容校验、读取中断清理，以及复制文件→数据库索引→清空索引→重新扫描恢复。另有 2 项转换包装提取测试通过。
- `node scripts/check-bridge-contracts.mjs`、`cargo check --lib`、`npm run build` 通过。
- 浏览器使用模拟 Tauri 接口检查 320px、360px、768px、1280px 页面，并禁用动态视口单位检查回退。加载测试音源和歌单列表后，下载/发现页面无横向溢出，分类与歌单卡片没有重叠；320×640 全屏播放器高度为 640px，控制栏位于 570–640px。模拟取消导入后，歌曲数量保持不变且没有成功提示。

## 尚需真机复验

本机没有配置 Android SDK，未构建 APK、未连接 vivo X21iA/Android 9。浏览器模拟不能验证系统选择器 URI 授权、原生 Insets 分发、系统三键/手势导航、旋转与软键盘，也不能替代真实网络下的音源播放。需在安装修复构建后逐项核对；不要据此直接关闭 Issue。

## 原始参考

- [LX 自定义源接口](https://github.com/lyswhut/lx-music-doc/blob/master/docs/desktop/custom-source.mdx)：环境值、初始化时机和事件 API。
- [Android edge-to-edge / Insets 官方指南](https://developer.android.com/develop/ui/views/layout/edge-to-edge)：使用 `WindowInsetsCompat` 应用系统栏边距并消费 Insets。
- 锁定依赖源码：`tauri-plugin-fs 2.5.1` 的 Android `Fs::open(FilePath, OpenOptions)` 支持 ContentResolver；`tauri-plugin-dialog 2.7.1` 提供移动端多选文件。Android 不依赖 iOS 专属的自动复制选项。
