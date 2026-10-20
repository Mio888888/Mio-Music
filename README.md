<p align="center">
  <img src="crates/mio-app/assets/images/icon.png" width="120" alt="Mio Music Logo" />
</p>

<h1 align="center">Mio Music</h1>

<p align="center">One music player for your local files, personal music servers and plugin-powered online sources.</p>

<p align="center">
  <b>English</b> · <a href="README.zh-CN.md">简体中文</a>
</p>

[Getting Started](#getting-started) · [Default Plugins](#default-plugins) · [Features](#features) · [Platform Support](#platform-support) · [Development](#development) · [Docs](https://mio888888.github.io/Mio-Music/)

---

## Introduction

**Mio Music is a local-first, cross-platform music player with plugin support.** Play your local files, connect to a personal music server you already use, or add compatible plugins for other online sources.

- **Local music:** scan files and manage tags, covers, favorites and playlists. No online plugin setup is required.
- **Personal libraries:** built-in connectors for Songloft, Subsonic, Navidrome, Jellyfin, Emby and Plex let you search and play your server's music in one player.
- **Listening experience:** word-by-word and TTML lyrics, translations and romanization, animated backgrounds, an equalizer, gapless playback and crossfade.
- **Multiple clients:** desktop apps for macOS, Windows and Linux; a Flutter client for Android phones, tablets and TVs, plus iPhone and iPad. See [Platform Support](#platform-support) for each platform's verification status.

> **Mio Music is a player, not a music content service. It does not include a public music catalog or service accounts.** Local playback needs no server. Built-in connectors require your server address and the appropriate account or access token; available content and permissions depend on the server and plugin.

<p align="center">
  <img src="website/assets/screenshots/home.png" width="100%" alt="Mio Music main interface" />
</p>

## Getting Started

Check [Platform Support](#platform-support) and [Releases](#releases) before choosing a package and version. To try the current source, follow [Development](#development).

| What you want to do | Where to start |
|---|---|
| Play local music | Open the local music page and import or scan your music files; no online plugins are needed |
| Connect a personal music server | Configure the matching built-in plugin under **Settings → Plugins → Music list plugins** |
| Add other online sources | Install a compatible music-list plugin; if it does not supply playback URLs, pair it with a source plugin that supports the same source |

See the [user guide](https://mio888888.github.io/Mio-Music/docs/user-guide.html) for detailed steps.

## Default Plugins

These **6 music-list plugins ship with the app; no separate script download is needed**. All provide song search, album and playlist browsing, discovery, and playback and download URL resolution. Extra features such as artist pages and lyrics vary by plugin.

| Built-in plugin | Service and extra features | Connection settings |
|---|---|---|
| **Songloft** | Songloft personal music server; artists, genres and lyrics | Server URL, username and password |
| **Subsonic** | Subsonic API-compatible music servers | Server URL, username and password |
| **Navidrome** | Navidrome; adds artist pages and lyrics beyond the generic Subsonic plugin | Server URL, username and password |
| **Jellyfin** | Jellyfin music library; artist pages and lyrics | Server URL and either a username login or access key; password as required by the account |
| **Emby** | Emby music library; artist pages | Server URL and either a username login or access key; password as required by the account |
| **Plex** | Plex Media Server music library; artist pages | Server URL and `X-Plex-Token`; optional library name, otherwise the first music library is used |

### Set up a built-in plugin

1. Open **Settings → Plugins → Music list plugins**, find your server's plugin and choose **Configure**.
2. Enter the server URL and authentication details from the table. For a non-loopback `http://` server, also enable **Allow HTTP (unencrypted)**; loopback addresses such as `localhost` do not need this option.
3. Use **Test connection** to check the current form, then save it and make sure the plugin is enabled.
4. Select the source on the search or discovery page to browse your server's library. Built-in plugins supply playback and download URLs by default, so **no additional source plugin is needed**.

At startup, the app installs any missing built-in plugins and upgrades older copies when it ships a newer version, preserving their settings and enabled state. Built-in plugins can be disabled but not uninstalled. Newly installed plugins that have not been configured do not offer selectable sources.

Features also depend on server versions and account permissions. Scripts, configuration details and server verification coverage are documented in [mio-music-plugins](https://github.com/Mio888888/mio-music-plugins).

### Other supported plugins

| Plugin type | What it does | How it is supported |
|---|---|---|
| **Music-list plugin** | Supplies search, playlists, albums and discovery, and may resolve its own playback and download URLs | In addition to the built-in plugins, compatible scripts can be installed from a local file or URL; multiple plugins can be enabled together |
| **Source plugin** | Resolves a playback URL from track information; does not supply search or playlists | Supports LanYin scripts; LuoXue / LX scripts are converted on installation. Explicitly select the plugin to use |

Plugins run in a resource-limited QuickJS sandbox. To write your own connector, see the [plugin developer guide](https://mio888888.github.io/Mio-Music/docs/plugins/).

## Features

### Music Playback

- Native audio engine on Rodio, cpal and Symphonia for local and online tracks
- Next-track prefetch, gapless playback with configurable crossfade; online tracks decode while downloading
- 10-band equalizer with presets and AutoEq import, plus effects and stereo balance
- Real-time FFT spectrum
- System media keys, tray and global hotkeys on the desktop; lock-screen and notification controls on mobile

### Lyrics

- Word-by-word and TTML lyrics rendered in the Apple Music style (AMLL)
- Translation, romanization, background and duet vocals
- Desktop lyrics window on macOS; custom lyric font

### Interface

- Dynamic theme from the album cover, with brand and error colors kept at 4.5:1 contrast so buttons and text stay readable
- Audio-reactive animated background
- Light and dark modes; Chinese and English

### Library & Data

- Local library scanning, tags and covers, tag editing, favorites and playlists
- Downloads with pause/resume, retry, speed display and tag writing
- S3-compatible backup and restore (AES-GCM encrypted), partial import of legacy app data
- DLNA casting, song recognition, signed auto-update
- Local first: library and settings live in local SQLite; no cloud service is required

## Platform Support

| Platform | Package | Status |
|---|---|---|
| macOS (Apple Silicon) | `.dmg`, updater `.app.tar.gz` | **Reference platform.** Playback, lyrics, downloads, backup and the main pages verified in the native app |
| Windows x64 | NSIS `-setup.exe` | Built, type-checked and unit-tested in CI; not yet run on a device |
| Linux x86_64 | `.AppImage`, `.deb` | Built, type-checked and unit-tested in CI (Ubuntu 24.04, glibc 2.39+); not yet run on a device |
| Android phone / tablet / TV (arm64-v8a, armeabi-v7a) | `.apk` | Acceptance runs on API 36 phone, tablet and TV emulators; physical devices and the armv7 runtime are pending |
| iPhone / iPad | unsigned `.ipa` | Runs in the iOS simulator; code signing and physical devices are pending |
| HarmonyOS | — | Experimental, outside `main`: an unsigned debug `.hap` builds; not yet run on a device |

There is no Intel macOS or x86_64 Android build. macOS packages are ad-hoc signed and not notarized.

## Releases

Builds are published on [GitHub Releases](https://github.com/Mio888888/Mio-Music/releases) by the `Build` workflow: whenever the version in `Cargo.toml` has no `v<version>` tag, it builds every platform, uploads the packages and `latest.json`, and publishes the release.

- **Current state:** no release of this rewrite is published yet. GitHub Releases still lists the `v0.3.x` releases of the previous Tauri app, which its users keep updating from.
- **Update notes:** every release carries the changes since the previous one, generated from commit messages. Both apps show them in their update dialog.
- **Updates:** the desktop updater reads `latest.json` and verifies Ed25519 signatures; the format matches the Tauri app, so its installs update in place.

The release workflow is [`.github/workflows/build.yml`](.github/workflows/build.yml).

## Tech Stack

This repository is the native rewrite of the [Tauri-based Mio Music](https://github.com/Mio888888/Mio-Music/releases/tag/v0.3.6), itself rebuilt from [CeruMusic](https://github.com/timeshiftsauce/CeruMusic). The desktop app uses Rust and [GPUI Kit](https://gpui-kit.com/), while the Flutter client shares its Rust core (`mio-core`). The rewrite follows the original features, layouts and data formats; the AI assistant is out of scope.

| Layer | Technology |
|-------|-----------|
| Desktop UI | GPUI Kit 0.7.2 (`gpui-fast`) |
| Mobile UI | Flutter + flutter_rust_bridge 2.13 |
| Language | Rust (edition 2024), Dart |
| Audio Engine | Rodio + cpal + Symphonia |
| Audio Processing | rustfft (spectrum), equalizer and effects in `mio-core` |
| Plugin Runtime | QuickJS (rquickjs), sandboxed |
| Database | SQLite (rusqlite) |
| UI Tokens & Icons | TDesign |
| CI | GitHub Actions |

## Development

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) stable, 1.85 or newer
- Git, and the native build dependencies from the [GPUI Kit docs](https://gpui-kit.com/docs/) (the exact packages CI installs are in [`.github/workflows/check.yml`](.github/workflows/check.yml))
- For mobile: Flutter, Android SDK/NDK or Xcode — see [`apps/mobile/README.md`](apps/mobile/README.md)

### Install & Run

The built-in plugins are a Git submodule ([mio-music-plugins](https://github.com/Mio888888/mio-music-plugins)), so clone with submodules:

```bash
git clone --recurse-submodules https://github.com/Mio888888/Mio-Music.git
cd Mio-Music
# already cloned? git submodule update --init

# Run the desktop app
cargo run --locked -p mio-app

# Build a macOS development bundle (target/debug/Mio Music GPUI.app)
sh tools/bundle-macos.sh
```

### Checks

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo check --workspace --locked
```

| What | Where | When |
|---|---|---|
| `cargo fmt --check`, `cargo check`, `cargo test` for the whole workspace | macOS 15, Windows, Ubuntu 24.04 (`Desktop checks`) | Every push to `main` |
| Release builds of all five platforms | `Build` workflow | When the version has no tag yet, or on demand with `build_only` |
| Interactive acceptance | macOS app; Android emulators; iOS simulator | Manually, during development |

Mobile acceptance evidence is described in [`apps/mobile/README.md`](apps/mobile/README.md). Real-device checks are explicit cases that the default offline tests skip.

### Data Locations

| Platform | Directory |
|---|---|
| macOS | `~/Library/Application Support/Mio Music GPUI` |
| Windows | `%APPDATA%/Mio Music GPUI` |
| Linux | `$XDG_CONFIG_HOME/mio-music-gpui` (default `~/.config/mio-music-gpui`) |

Set `MIO_GPUI_DATA_DIR` to use a separate directory for testing. Data from the Tauri app is not read by default; import it from **Settings → Storage → Legacy app data**.

### Writing Plugins

The [plugin developer guide](https://mio888888.github.io/Mio-Music/docs/plugins/) covers both plugin types, the data structures, the sandboxed runtime API and publishing; the scripts in [mio-music-plugins](https://github.com/Mio888888/mio-music-plugins) are complete working examples. `tools/golden/subsonic_fixture.py` is a loopback-only Subsonic server with synthetic data for trying plugins without a real server.

## Project Structure

```
├── crates/
│   ├── mio-app/             # Desktop app: GPUI interface, tray, hotkeys, media keys, desktop lyrics, DLNA, updater
│   ├── mio-core/            # Shared core: playback and effects, library (SQLite), downloads, backup, lyrics, themes, plugins
│   ├── mio-music-sdk/       # Music-list contract: provider trait, capabilities, data types, lyric/comment/cover decoding
│   ├── mio-plugin/          # Plugin host: sandboxed QuickJS runtime, list-plugin loader, LuoXue converter
│   ├── mio-tokens/          # UI tokens (light and dark palettes)
│   └── mio-mobile-bridge/   # flutter_rust_bridge API for the mobile app
├── apps/mobile/             # Flutter app for Android and iOS
├── plugins/                 # Submodule with the built-in list plugins
├── tools/                   # Packaging, CI helpers, release notes, golden-image tooling, Subsonic fixture
└── website/                 # Static site and docs (GitHub Pages)
```

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) first; the pull request and issue templates list what to include. Report security problems privately, as described in [SECURITY.md](SECURITY.md).

## Credits

- [CeruMusic](https://github.com/timeshiftsauce/CeruMusic) — Original project by **ShiQianJiang**
- [Mio Music (Tauri)](https://github.com/Mio888888/Mio-Music/releases/tag/v0.3.6) — The app this project re-implements
- [GPUI Kit](https://gpui-kit.com/) — Native desktop UI framework
- [Flutter](https://flutter.dev/) and [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) — Mobile app and its Rust bridge
- [TDesign](https://tdesign.tencent.com/) — Design tokens and icons
- [Apple Music-like Lyrics](https://github.com/Steve-xmh/applemusic-like-lyrics) — Lyrics and background style
- [AutoEq](https://github.com/jaakkopasanen/AutoEq) — Equalizer preset data

## Support the Current Author

Ongoing maintenance, cross-platform adaptation and issue fixes for Mio Music are contributed by the current project author. If this project helps you, you can support future development with the appreciation code below.

<p align="center">
  <img src="crates/mio-app/assets/images/current-author-sponsor-qr.jpg" width="240" alt="Current project author's appreciation code" />
</p>

> All donations are voluntary and go directly toward the current author's continued development and maintenance.

## Disclaimer

Mio Music provides playback, library management and a plugin runtime; it does not provide or host a public music catalog. Online playback and downloads access third-party services or personal servers through user-configured plugins. You are responsible for having permission to access, play and download the content, following local laws and service terms, and respecting third-party rights.

## License

Mio Music is free software, licensed under the [GNU Affero General Public License v3.0](LICENSE) (`AGPL-3.0-only`). You may use, study, change and share it, including commercially, as long as works derived from it are released under the same license with their source code, which also applies when users interact with them over a network.

It is derived from [CeruMusic](https://github.com/timeshiftsauce/CeruMusic) (Copyright © 2026 时迁酱, `AGPL-3.0-only`) through the Tauri-based Mio Music. Third-party code keeps its own license: the libraries vendored under `vendor/` and `apps/mobile/vendor/` are Apache-2.0 or MIT, and every other dependency is resolved in `Cargo.lock` and `apps/mobile/pubspec.lock`. Contributions are accepted under the same license; see [CONTRIBUTING.md](CONTRIBUTING.md).
