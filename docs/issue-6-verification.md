# Issue #6 验证记录

核实日期：2026-09-06。修复状态：本地实现，未发布。

## 根因与修复

- Android 文件选择器返回的 `content://` URI 不能通过 `as_path()` 取得普通路径。改为将 `FilePath` 原样交给 Tauri FS 原生读取，文件名不可获得时使用安全兜底名称。未增加前端文件系统权限。
- 静态音质解析未匹配带引号的 `"qualitys"` 字段。补齐该格式，并通过现有 Worker 执行器取得经过校验的运行时声明，更新插件列表和用户设置；旧插件无元数据或加载失败时保留静态兜底。
- 音质同步仅应用最新选择的结果，保留有效偏好及内置源；插件替换同时清除已完成和在途的加载缓存。取消导入不作废有效的选择请求。

## 两个真实样本

用户提供的 Ceru 与 LX 样本均自报 v26。原始样本及包含密钥的链接不入库。

| 音源 | Ceru 声明 | LX 声明 |
| --- | --- | --- |
| kw | 128k、320k、flac、hires | 128k、320k、flac、flac24bit、hires |
| kg、wy | 128k、320k、flac、hires、atmos、master | 128k、320k、flac、flac24bit、hires、atmos、master |
| tx | 128k、320k、flac、hires、atmos、master | 128k、320k、flac、flac24bit、hires、atmos、atmos_plus、master |
| git | 未声明 | 128k、320k、flac |

两个样本均通过生产 Rust 解析器、Worker 执行器和设置 store 核对，三层音质列表一致。LX 先使用生产转换器转换。逐一验证 Ceru 的 22 个、LX 的 30 个音源/音质组合，确认参数原样送入插件请求。所有更新检查和取歌 HTTP 请求均替换为本地测试响应，未测试真实服务返回或音频编码。

全局音质菜单按全部音源交集显示：此 Ceru 样本为 128k/320k/flac/hires，LX 样本因含 git 源而为 128k/320k/flac。更高音质应查看单音源菜单；这不是解析丢失。

## 可重复验证

```sh
node --test scripts/plugin-sources.test.mjs scripts/plugin-store-sources.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml --lib plugin::
yarn build
```

结果：20 项前端定向回归测试、8 项 Rust 插件测试通过，前端类型检查与生产构建通过。Rust 测试在 macOS 主机执行；其中 Android URI 测试验证 URI 原样传递，未调用真机 ContentResolver。主机测试可能重写平台 schema，提交前须核对移动端 ACL 不含 updater，并同步 FS 的 Android Gradle 依赖与 schema。

本地同步生成文件使用已锁定的 Tauri 源码生成函数（`generate_capability_schema`、`generate_gradle_files`），没有手工拼接 schema。Android ACL 基于原有移动端 manifest 加入 FS，仍排除桌面 updater。

## 验证边界

音质解析和 Worker/store 逻辑由 Windows、macOS、Linux、Android 共用，本次未新增按平台区别处理的音质分支。但没有 Windows/Linux/Android 真机及已配置的 Chrome DevTools 测试环境，不能据此宣称所有端已完成端到端验证。

发布前仍需各端导入/选择样本并检查单音源菜单；Android 需通过系统文件选择器分别导入两种格式及取消选择。真实音乐可用性、授权和实际音频码率需使用有效曲目及服务端响应另行确认。
