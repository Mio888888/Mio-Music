# 参与贡献 Mio Music

[English](CONTRIBUTING.md) · **简体中文**

感谢你的参与。无论是问题反馈、修复、文档还是插件，我们都欢迎。请先阅读[行为准则](CODE_OF_CONDUCT.zh-CN.md)，安全问题请按 [SECURITY.zh-CN.md](SECURITY.zh-CN.md) 私下报告。

## 开始之前

- **问题与想法：** 先在 [issues](https://github.com/Mio888888/Mio-Music/issues) 里搜索，再用模板新建。改动较大时，请先开 issue 讨论方案，再动手写代码。
- **音乐来源属于插件。** 应用本身不带任何在线音源。请不要加入内置或写死某个受版权保护的音乐服务的代码、资源、密钥或接口地址。内置插件放在 [mio-music-plugins](https://github.com/Mio888888/mio-music-plugins)（即 `plugins/` 子模块）。
- **许可。** 项目采用 [AGPL-3.0-only](LICENSE) 许可。提交贡献即表示你同意以相同条款授权，并确认你有权这样做。请不要引入许可证与 AGPL-3.0 不兼容的代码或资源。

## 搭建环境

环境要求与运行命令见 [README](README.zh-CN.md#开发)：Rust 1.85 或更新、GPUI Kit 文档列出的原生构建依赖，以及 Git。克隆时带上子模块：

```bash
git clone --recurse-submodules https://github.com/Mio888888/Mio-Music.git
cd Mio-Music
cargo run --locked -p mio-app
```

移动端（`apps/mobile`）还需要 Flutter 以及 Android SDK 或 Xcode，见 [`apps/mobile/README.md`](apps/mobile/README.md)。

## 提交改动

1. Fork 仓库，从 `main` 创建分支（鸿蒙相关的工作以 `experiment/ohos` 为目标）。
2. 保持改动聚焦。桌面端与移动端共用的代码在 `crates/mio-core`，改动它时请确认 Android 与 iOS 仍然正常。
3. 推送前运行与 CI 相同的检查：

   ```bash
   cargo fmt --all -- --check
   cargo test --workspace --locked
   cargo check --workspace --locked
   ```

   改动了移动端时，还要在 `apps/mobile` 里运行 `flutter analyze`。
4. 重新生成需要生成的内容，并一起提交：
   - 改了 `crates/mio-mobile-bridge/src/api.rs`：在 `apps/mobile` 里运行 `flutter_rust_bridge_codegen generate`（版本 2.13.0）。
   - 改了语言文件、令牌或图标：运行 `python3 tools/export-mobile-assets.py`（需要 Pillow）。移动端的语言文件是 `crates/mio-core/locales/` 的副本。
5. 在你改动涉及的平台上，用真实的应用试一遍，并在 PR 里写清楚你运行了什么、在什么环境上。
6. 行为有变化时，同步更新 [`website/docs`](website/docs) 里的用户文档与 README。

## 提交信息

提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)：`feat:`、`fix:`、`perf:`、`docs:` 等，可带范围，如 `fix(player):`。发布说明由它们自动生成，所以标题请站在用户的角度来写；正文里以 `- ` 开头的行会成为子条目。`chore`、`ci`、`build`、`style`、`test`、`refactor` 类型的提交不会出现在发布说明中。发布流程见 [`.github/workflows/build.yml`](.github/workflows/build.yml)。

## 拉取请求

请向 `main` 提交 PR，并按模板填写。小而易于评审的 PR 合并得更快。维护者可能会请你修改，请把讨论放在 PR 里进行。

## 编写插件

写插件不需要改动本仓库。[插件开发者指南](https://mio888888.github.io/Mio-Music/docs/plugins/) 介绍了两类插件和沙箱运行时 API。
