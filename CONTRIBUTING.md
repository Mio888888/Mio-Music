# Contributing to Mio Music

**English** · [简体中文](CONTRIBUTING.zh-CN.md)

Thanks for helping. Bug reports, fixes, documentation and plugin work are all welcome. Please read the [Code of Conduct](CODE_OF_CONDUCT.md) first, and report security problems privately as described in [SECURITY.md](SECURITY.md).

## Before you start

- **Bugs and ideas:** search the [issues](https://github.com/Mio888888/Mio-Music/issues) first, then open one with the template. For a large change, open an issue before you write code so the approach can be agreed.
- **Music sources belong in plugins.** The app ships no online source. Do not add code, assets, API keys or endpoints that bundle or hard-code access to a copyrighted music service. Bundled plugins live in [mio-music-plugins](https://github.com/Mio888888/mio-music-plugins) (the `plugins/` submodule).
- **Licensing.** The project is licensed under [AGPL-3.0-only](LICENSE). By submitting a contribution you agree to license it under the same terms, and you confirm you have the right to do so. Do not copy in code or assets whose license is not compatible with AGPL-3.0.

## Set up

Requirements and run commands are in the [README](README.md#development): Rust 1.85 or newer, the native build dependencies from the GPUI Kit docs, and Git. Clone with submodules:

```bash
git clone --recurse-submodules https://github.com/Mio888888/Mio-Music.git
cd Mio-Music
cargo run --locked -p mio-app
```

The mobile app (`apps/mobile`) also needs Flutter and the Android SDK or Xcode; see [`apps/mobile/README.md`](apps/mobile/README.md).

## Making a change

1. Fork the repository and branch from `main` (HarmonyOS work targets `experiment/ohos`).
2. Keep the change focused. Code shared by the desktop and mobile apps lives in `crates/mio-core`; check that Android and iOS still behave when you touch it.
3. Run the same checks as CI before you push:

   ```bash
   cargo fmt --all -- --check
   cargo test --workspace --locked
   cargo check --workspace --locked
   ```

   For mobile changes also run `flutter analyze` in `apps/mobile`.
4. Regenerate what is generated, and commit the result:
   - After changing `crates/mio-mobile-bridge/src/api.rs`: `flutter_rust_bridge_codegen generate` (version 2.13.0) in `apps/mobile`.
   - After changing locales, tokens or icons: `python3 tools/export-mobile-assets.py` (needs Pillow). The mobile locale files are copies of `crates/mio-core/locales/`.
5. Try the change in the real app on the platform you touched, and say in the pull request what you ran and on what.
6. Update the user-facing documentation in [`website/docs`](website/docs) and the README when behavior changes.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `perf:`, `docs:`, and so on, with an optional scope such as `fix(player):`. Release notes are generated from them, so write the subject for a user. Bullet lines (`- ...`) in the body become sub-items. `chore`, `ci`, `build`, `style`, `test` and `refactor` commits are left out of the notes. The release workflow is [`.github/workflows/build.yml`](.github/workflows/build.yml).

## Pull requests

Open the pull request against `main` and fill in the template. Small, reviewable pull requests are merged faster than large ones. A maintainer may ask for changes; please keep the conversation on the pull request.

## Writing plugins

Plugin authors do not need to change this repository. The [plugin developer guide](https://mio888888.github.io/Mio-Music/docs/plugins/) explains both plugin types and the sandboxed runtime API.
