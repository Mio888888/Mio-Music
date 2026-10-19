<p align="center">
  <img src="crates/mio-app/assets/images/icon.png" width="120" alt="Mio Music Logo" />
</p>

<h1 align="center">Mio Music</h1>

<p align="center">A native cross-platform music player: Rust + GPUI on the desktop, Flutter on mobile, one shared Rust core</p>

<p align="center">
  <b>English</b> · <a href="README.zh-CN.md">简体中文</a>
</p>

---

## Introduction

Mio Music is the native rewrite of the [Tauri-based Mio Music](https://github.com/Mio888888/Mio-Music/releases/tag/v0.3.6), itself rebuilt from [CeruMusic](https://github.com/timeshiftsauce/CeruMusic). The desktop app is written in Rust on [GPUI Kit](https://gpui-kit.com/) 0.7.2 (with `gpui-fast`) and re-implements the original feature for feature, layout for layout and data format for data format; the AI assistant is out of scope. Phones, tablets and TVs run a Flutter app that shares the same Rust core (`mio-core`). Online music comes only from plugins.

> This project does not host, fetch or transmit any music data by itself. All online content comes from third-party services through plugins; whether that content may be used is the responsibility of plugin providers and users.

Docs and the plugin developer guide are published at <https://mio888888.github.io/Mio-Music/>.

<p align="center">
  <img src="website/assets/screenshots/home.png" width="100%" alt="Mio Music" />
</p>

## Features

### Music Playback

- Native audio engine on Rodio, cpal and Symphonia for local and online tracks
- Gapless playback with configurable crossfade
- 10-band equalizer with presets and AutoEq import, plus effects and stereo balance
- Real-time FFT spectrum
- System media keys, tray and global hotkeys on the desktop; lock-screen and notification controls on mobile

### Lyrics

- Word-by-word and TTML lyrics rendered in the Apple Music style (AMLL)
- Translation, romanization, background and duet vocals
- Desktop lyrics window on macOS; custom lyric font

### Music Sources & Plugins

- Search, playlists, albums, discovery and artists come from *music-list plugins*; playback addresses come from the same plugins or from *source plugins* (LanYin scripts; LuoXue scripts are converted automatically)
- **Built-in plugins for Songloft, Subsonic, Navidrome, Jellyfin, Emby and Plex** are installed and updated automatically — enter a server address in plugin management and they are ready
- The source list, its order and its icons all come from the enabled plugins; an unconfigured plugin offers no sources
- Plugins run in a bounded, sandboxed QuickJS runtime

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

## Testing

| What | Where | When |
|---|---|---|
| `cargo fmt --check`, `cargo check`, `cargo test` for the whole workspace | macOS 15, Windows, Ubuntu 24.04 (`Desktop checks`) | Every push to `main` |
| Release builds of all five platforms | `Build` workflow | When the version has no tag yet, or on demand with `build_only` |
| Interactive acceptance | macOS app; Android emulators; iOS simulator | Manually, during development |

Mobile acceptance evidence is described in [`apps/mobile/README.md`](apps/mobile/README.md). Real-device checks are explicit cases that the default offline tests skip.

## Releases

Builds are published on [GitHub Releases](https://github.com/Mio888888/Mio-Music/releases) by the `Build` workflow: whenever the version in `Cargo.toml` has no `v<version>` tag, it builds every platform, uploads the packages and `latest.json`, and publishes the release.

- **Current state:** no release of this rewrite is published yet. GitHub Releases still lists the `v0.3.x` releases of the previous Tauri app, which its users keep updating from.
- **Update notes:** every release carries the changes since the previous one, generated from commit messages (or a hand-written `docs/release-notes/v<version>.md`). Both apps show them in their update dialog.
- **Updates:** the desktop updater reads `latest.json` and verifies Ed25519 signatures; the format matches the Tauri app, so its installs update in place.

Release process and package names: [`docs/release.md`](docs/release.md).

## Tech Stack

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
├── docs/                    # Release process and development notes
└── website/                 # Static site and docs (GitHub Pages)
```

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

This project is for educational and personal use only. It does not fetch, store or transmit any music data or copyrighted content by itself — it only provides a plugin runtime framework. Use for any commercial purpose or infringement of third-party rights is prohibited.

## License

There is no license file yet; the project and dependency licenses are being reviewed before the first release of this rewrite. Assets and converters reused from the original project keep the original repository's license terms.
