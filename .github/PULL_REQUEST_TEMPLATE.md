<!-- Thanks for contributing! / 感谢你的贡献！ Please fill in each section. / 请填写每一部分。 -->

## What does this change? / 改动内容

<!-- What and why, in a few sentences. Link the issue: Fixes #123 / 说明改了什么、为什么；关联 issue：Fixes #123 -->

## Type / 类型

- [ ] Bug fix / 修复问题
- [ ] New feature / 新功能
- [ ] Performance / 性能优化
- [ ] Documentation / 文档
- [ ] Build, CI or tooling / 构建、CI 或工具
- [ ] Other / 其他

## Platforms touched and how you verified / 涉及的平台与验证方式

<!-- Which platforms does this affect, and what did you run? Example: "macOS 15: played a local FLAC, switched themes". / 影响哪些平台，你实际运行了什么。 -->

- [ ] macOS
- [ ] Windows
- [ ] Linux
- [ ] Android
- [ ] iOS
- [ ] HarmonyOS (`experiment/ohos`)
- [ ] Website / docs only / 仅网站或文档

## Screenshots / 截图

<!-- For UI changes: before and after, light and dark. Remove this section otherwise. / 界面改动请附前后对比，含浅色和深色；否则删除本节。 -->

## Checklist / 自查清单

- [ ] `cargo fmt --all -- --check`, `cargo test --workspace --locked` and `cargo check --workspace --locked` pass / 以上三项检查通过
- [ ] For mobile changes, `flutter analyze` passes in `apps/mobile` / 移动端改动已通过 `flutter analyze`
- [ ] Generated files are updated (`flutter_rust_bridge_codegen generate`, `tools/export-mobile-assets.py`) / 已更新生成的文件
- [ ] Android and iOS still behave if I changed shared code in `crates/mio-core` / 改了共用代码时，已确认 Android 与 iOS 正常
- [ ] User-facing docs and the README are updated where behavior changed / 行为有变化的地方已更新文档与 README
- [ ] Commit messages follow Conventional Commits (`feat:`, `fix:`, ...) / 提交信息遵循 Conventional Commits
- [ ] No copyrighted music source, key, token or credential is added / 没有加入受版权保护的音源、密钥、令牌或凭据
- [ ] I agree my contribution is licensed under AGPL-3.0-only / 我同意以 AGPL-3.0-only 授权我的贡献
