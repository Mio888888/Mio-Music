# Mio Music mobile

Flutter + flutter_rust_bridge + shared `mio-core`, for Android phones/tablets/TV
and iPhone/iPad. GPUI remains desktop-only. Status and acceptance exits are in
`../../docs/mobile/progress.md` and `../../docs/mobile/plan.md` (local documents).

## Current development acceptance (2026-10-17)

Latest acceptance on unchanged `8df82582` adds read-only EROFS, independent fsync EIO and first-byte write EIO. Recognition and clear failures preserve old history bytes and current results; removing each fault permits a fresh save without late failed-attempt append. Normal recovery traces confirm complete writes, fsync and history replacement; clear retry keeps the current 3 results, and empty history survives a new PID. See `docs/mobile/evidence/20261017/recognition-io-phone-acceptance.json` and `recognition-io-phone-cleanup.json`. The old tracer's unsupported default replacement caused a separately recorded SIGSYS; supported injection uses allowed getpid. This is isolated-volume/syscall-return evidence, not physical-media or power-loss proof. Partial/short writes, delayed writeback, crash/concurrency cases and physical P8 remain separate. Original 18 files and system settings match after an independent cold boot; tracers, mounts, audio, emulator and forward are removed/stopped. No product/wide source, APK or test changes; no tests run, commit, push or publication.

Earlier acceptance uses the unchanged `8df82582` APK: real block exhaustion, inode exhaustion and a bind-mounted history destination that blocks atomic replacement were exercised on an isolated 8 MiB tmpfs over app files. Recognition and clear failures preserve old history and current results; freeing each obstruction permits a fresh recognition save with no late failed-attempt append. Recovered 20 rows and cleared empty history each survive a new app PID. See `docs/mobile/evidence/20261017/recognition-storage-phone-acceptance.json` and `recognition-storage-phone-cleanup.json`. This is scoped emulator evidence, not whole-userdata ext4/F2FS exhaustion or power-loss durability. Read-only and independent fsync failures were still open in that batch; physical P8 stays separate. The original 18 files and system settings match after a separate cold boot without launching the app; mounts/audio are removed and the emulator/forward stopped. No product/wide source or APK changes, test code, commit, push or publication.

Earlier acceptance addition uses the **unchanged `8df82582` APK** below. Phone microphone source-switch cancellation,19+3→20 and20+3→20 ordered retention,newPID persistence,denied-save/clear preservation and restored-permission retry passed. Injected21row metadata stays untouched until explicit clear. See `docs/mobile/evidence/20261017/recognition-retention-phone-acceptance.json` and `recognition-retention-phone-cleanup.json`. No product/wide source or APK changes;253recorded input hashes and installed readback match. Original18files,directory mode771 and microphone permission/UID mode restored and cold-verified;fixture removed,emulator/forward stopped. Other write-failure stages and physical P8 remain separate. No tests added,modified or run;no commit/push/publication.

The current Phone development APK is arm64 Debug `8df82582` (SHA-256
`8df825823a7e5b5c87398e36cb1c0c6e373347dfb858d5b12b0064a68db427d6`). Narrow recognition now closes its session when the selected-source revision changes;wide dialog/shell and native workers remain unchanged. The same APK passed offline failure,20s cellular-delay bounded failure,in-flight source cancellation and normal retry,plus malformed/unreadable/externally changed history preservation and recovery. Timeout evidence bounds completion,not a DNS/connect/TLS/response taxonomy. See `docs/mobile/evidence/20261017/recognition-failures-phone-acceptance.json`. Original18files,registry/source preference and network settings restored and cold-verified;microphone permission unchanged,shared audio fixture removed,emulator/forward stopped. Physical microphone,Bluetooth/calls,OEM behavior and power remain P8. No test code added,modified or run;no commit/push/publication.

The previous recognition-flow APK is arm64 Debug `e52f1f7d` (SHA-256
`e52f1f7d503542a014d047b02c7f8c56e357d79c847fb6dfef153ebf0e4cabc2`). Narrow recognition now reports an explicit user stop while preserving playback-restore failures;wide header/dialog and native workers are unchanged. Final-package Phone recording/stop/automatic no-match,3 real file matches,matched30s playback,clear cancellation/confirmation and empty-history restart passed. Earlier `6cb7530b` separately covers permission denial/retry,history persistence/play,recording pause/resume,Home cancellation,search and6row history flows. See `docs/mobile/evidence/20261017/recognition-flow-phone-acceptance.json` for exact package attribution. Temporary authorized source/audio fixtures and original18files/permission state restored and cold-verified;latest APK retained,emulator/forward stopped. Physical microphone/acoustic success,Bluetooth/calls,OEM behavior and power remain P8. No test code added,modified or run;no commit/push/publication.

The previous recognition-page UI APK is arm64 Debug `714fea8a` (SHA-256
`714fea8ae6032179285a1d2ff8bd1dd2f088e9df1946d471e962b114d263e070`). Narrow header actions are microphone,backup and download icons. Recognition now opens an embedded shell page based on the macOS wide UI:280px stage,120px brand circle,48px microphone,prompt,upload,progress and result/history rows. Wide header,backup control and dialog view remain unchanged. Final-package Phone entry,picker cancellation,Back/reentry,analyze,build and install readback passed; original18files were restored and cold-verified. See `docs/mobile/evidence/20261017/recognition-page-phone-acceptance.json` and `recognition-page-phone-cleanup.json`. Recording/service/results/history workflows were not rerun in this UI batch;physical P8 and earlier package evidence remain separate. No test code added,modified or run;no commit,push/publication.

The previous player-mode Phone development APK is arm64 Debug `39d0443f` (SHA-256
`39d0443ff1bf6ef958ddbd2a29f3c9dfe2441a6c6ba66eda6dfb8dbb9e866a66`). The narrow player now has a 44dp circular swap button matching the left collapse control. It replaces the top-right toolbar and toggles artwork/lyrics in both directions. API36 Phone manual checks, analyze, build and install readback passed; wide toolbar source and native library are unchanged. See `docs/mobile/evidence/20261017/player-mode-phone-acceptance.json`. Earlier startup and business evidence stays attributed to its original package; physical-device P8 remains open. No test code was added, changed or run; no commit, push or publication.

For the previous startup package `c8c4393d` (SHA-256 `c8c4393dab9729f99bb0de48ddb0ccfb3690d529f29f4907883f47c37316285b`), actual API36Phone day/night COLD launch recordings,installed readback,resource compilation and arm64 Debug build passed. Dart kernel and Rust native library are byte-identical to7772b0b9;wide business UI unchanged. See `docs/mobile/evidence/20261017/particle-launch-phone-acceptance.json` and `particle-launch-phone-cleanup.json`. Original18files and system night mode restored and independently cold-verified;emulator/forwards stopped. No tests/commit/push/publish. Older Android/OEM/physical-device startup remains separate.

The previous header-row Phone development APK is arm64 Debug `7772b0b9` (SHA-256
`7772b0b930f45bed3ff57ca97f85cce2ed7ab72768871a05a7e9a1263e3e1d5b`). Only `_phoneHeader` changed:32dp round artist avatar,one-line title and44dp aligned touch targets;centered vector More icon replaces baseline-shifted text. Phone avatar/row/menu/expand and play-confirmation cancellation checked,original paused queue unchanged. Source outside the phone-header method is byte-identical;wide/expanded/detail code and Rust native library unchanged. Analyze/build/installed readback passed. See `docs/mobile/evidence/20261017/header-row-phone-acceptance.json` and `header-row-phone-cleanup.json`. Original18files restored and independently cold-verified;no tests/commit/push/publish. Other fonts/names/devices and physical P8 remain separate.

The previous collapse Phone development APK is arm64 Debug `1988026e` (SHA-256
`1988026e445cb9aac33addc3b3706b6dd812b7c99a11a6dfb01bce0cfb1687a6`). Narrow artist profiles collapse on upward scrolling/dragging into a compact Back/title/play row with retained tabs. Pull to the list top or tap the title to expand. The native220ms transition respects disableAnimations. This frees about270dp on384×800dp Phone; the original shared header and wide content branch remain unchanged, and album details keep their original header.

Actual Phone expand/collapse,compact menu/play confirmation cancellation,songs30→54,albums30→56,detail Back and Home return passed. Analyze/build/installed readback and251recorded input checks passed;Rust native unchanged. Original18files were restored from the previous independently cold-verified baseline after this run's startup archive proved invalid, then independently cold-verified again without launching Mio. See `docs/mobile/evidence/20261017/collapse-phone-acceptance.json`, `collapse-phone-source.json` and `collapse-phone-cleanup.json`. Emulator/forwards stopped;no test code added,modified or run;no commit,push or publish. Other font/device states,frame-time profiling and real-device P8 remain separate.

The previous paging Phone development APK is arm64 Debug `b1d3c8a7` (SHA-256
`b1d3c8a726165d65bbb137cc92bdf438bb399402f014eee1195deae5fc1cee7a`). On API36 arm64 Phone384×800dp/default font, real online-source paging passed for DouDou songs30→54 and albums30→56, Jay Chou albums30→43, and Taylor Swift THE ANTHOLOGY album songs30→31. All three types retain the initial30 items during an offline second-page failure; same-page Retry restores the exact prefix, unique IDs and terminal state. Existing-list failures now render at the footer, including album-detail songs; first-page empty errors stay at the top. Only singer.dart changed; the Rust library is unchanged.

A controlled CELLULAR-delay run observed next-page loading before Home, session closure, no append in the later background snapshot, and a fresh30-card foreground rebuild. Original network settings and18files were restored and independently cold-verified without launching Mio; emulator/forwards stopped. See `docs/mobile/evidence/20261017/paging-phone-acceptance.json`, `paging-phone-source.json` (251 recorded inputs, not a full dependency closure) and `paging-phone-cleanup.json`. Analyze/build/installed readback/diff checks passed; no test code added,modified or run; no commit,push or publish. Other providers, all large catalogs, cross-page batch operations and physical-device P8 remain separate.

The previous playback/status Phone development APK is arm64 Debug `c6ee2308` (SHA-256
`c6ee23089e0821c8cac9bbe89991761fc0bb306fe6e7303a94ad9b124e5488ab`). Unconfigured quality now prefers128k across the shared playback/source defaults; explicit preferences remain authoritative. The original master URL returnedHTTP403 while128k played. Failure UI exposes the status and recovery guidance. The singer top gradient extends through the44dp status inset with readable icons.

Final-package uncached default playback advanced35seconds, native background another14seconds, and paused track/queue/position survived restart. Red DouDou status/tab/player-return/settings flows and two other tracks were observed. Detailed seek/master403/recovery/30loaded-track queue and restart acceptance belongs to intermediate `eda88362`, with the same Rust native library; it is not relabeled as final-package reruns. See `docs/mobile/evidence/20261017/http-phone-acceptance.json`, `http-phone-source.json` (251 recorded inputs, not a complete dependency closure) and `http-phone-cleanup.json`. Original18files were restored and independently verified after no-App/no-snapshot cold boot; forwards/emulator stopped. No test code added,modified or run; no commit,push or publish. Master availability, other providers, full paging, long playback and physical audio remain separate.

The previous singer UI Phone development APK is arm64 Debug `439deb39` (SHA-256
`439deb391c9017e0c7836b3ee4cd90abedc8f6c525f0d32b5450ef69195a6ae9`). The singer page now matches the running
macOS narrow reference: circular avatar, centered profile and primary action,
left-aligned tabs, badged song rows and two-column album covers. Phone artist
navigation retains the shared playback/footer; secondary operations stay in More,
and play-all confirmation covers the whole window. Actual default-font Phone
entry, nested/detail Back, batch exit, download cancellation, player return and
navigation exit passed with installed readback and clean inspected Flutter logs.
See `docs/mobile/evidence/20261017/singer-phone-acceptance.json` and
`singer-phone-cleanup.json`. Full paging, other states/devices and physical audio
remain separate. Earlier packages below retain their own acceptance attribution.

The previous Phone S3/Range development APK is arm64 Debug `e8123b71` (SHA-256
`e8123b71f3ad394a3120491032b64ac86d67e42f3248661853ad94fcf1c96710`). It fixes
password focus when the S3 upload preview replaces the form. Installed readback,
preview input, actual encrypted upload, independent decryption and retention2
passed against local Moto. Single-page nonempty merge/duplicate/overwrite,
plugin skip/add and restore restart retain `15a0ad57` attribution; compact settings
and local persistence also retain that package's evidence. Original18files, shared preferences and external/shared Downloads were restored
and verified after no-App, no-snapshot cold boot. Overlay permission is back to
default after explicit appops persistence; emulator/services/forwards are stopped.
See `docs/mobile/evidence/20261017/phone-final-cleanup.json`.

Normal multi-page success remains open: Moto's second page omitted the required
ContinuationToken echo, so the app refused before download/write. No parser was
weakened. See `docs/mobile/evidence/20261017/settings-phone-acceptance.json` and
`s3-phone-acceptance.json`; `latest_development_*` identifies the current Debug
package. The last verified dual-ABI Release remains `bddefdf4`; historical
Release evidence below keeps its package attribution. Real cloud, physical audio,
timed floating lyrics/touch-through and broader failure/concurrency work
remain separate. No tests, commit, push or publication.

The same `e8123b71` also passed Phone controlled Range206 resume/prefix retention,
ignored200 and wrong206-start fallback, full PCM equality for three generated MP3s,
completed-task restart and native FileProvider audio preview/return. See
`docs/mobile/evidence/20261017/range-phone-acceptance.json`. Existing external/shared
files and original18files were restored; no physical audible-quality claim.

The same package passed small-playlist name/description editing and one-song batch
removal retained after process restart (SQLite including WAL), plus actual system
overlay permission, native display over Home, dragging,125% sizing and locking.
See `docs/mobile/evidence/20261017/catalog-overlay-phone-acceptance.json`.
Artist/album pagination, cross-page batches and timed overlay/touch-through remain open.

## Local Android build

Run from the repository root with the existing SDKs and dependencies:

```sh
source tools/mobile-env.sh
cd apps/mobile
flutter analyze
flutter build apk --debug --no-pub --target-platform android-arm64
```

The debug APK is `build/app/outputs/flutter-apk/app-debug.apk`. The native-assets
hook builds `mio-mobile-bridge` with the locked Cargo dependencies. The project
uses Android API 26 or newer, compile/target SDK 36, NDK 28.2.13676358 and JDK 17.
Use the Flutter version recorded in `.flutter-version`. Source the environment
script in the same shell as every native build; it does not change the host's
default toolchains.

For native ABI validation from the repository root:

```sh
source tools/mobile-env.sh
cargo build --locked -p mio-mobile-bridge --lib --target aarch64-linux-android
cargo build --locked -p mio-mobile-bridge --lib --target armv7-linux-androideabi
```

Both arm64 and armv7 now have complete debug and Release-mode APK build evidence.
From `apps/mobile`, build the additional packages with:

```sh
flutter build apk --debug --no-pub --target-platform android-arm --split-per-abi
flutter build apk --release --no-pub --target-platform android-arm,android-arm64 --split-per-abi
```

For one universal Release APK with version code 1, use:

```sh
flutter build apk --release --no-pub --target-platform android-arm,android-arm64
```

This produces `app-release.apk`. The last verified dual-ABI Release package is `bddefdf4` (242 inputs,
81,789,492 bytes; SHA256
`bddefdf492e5e35f6d8efa04ca7f47db049a68eb2a34641da0310f1259a3839e`).
Only shared `recognition.rs` and Flutter `recognition.dart` changed from `de2450`:
the AFP host now reads/releases byte-value handles correctly, and modal text keeps
the same captured palette as its background. Previously stubbed emval conversions
produced all-zero fingerprints. Two real PCM inputs now match the original JS
fingerprints byte for byte; the WASM asset is unchanged. Independent read-only
review found no blocking ABI or palette issue.

On the final Phone APK, one native-picker selection and separate foreground
confirmation returned three real Imagine/John Lennon matches at 30 seconds.
Matched-position playback advanced from 30 to 44.811 seconds; real history bytes
and the paused position survived a new process. Modal text remained readable
through the cover-theme change. Intermediate `416203` has identical native
libraries and separately attributed history search (20 results) and history Play.
Original APK, 11 internal files, full external/shared data/settings/microphone
permission were restored and independently cold-verified without launching the app.
All batch emulators and forwards stopped. Format/check/analyze, dual-ABI Release,
signature, CRC, 16KiB alignment, source hashes and installed readback passed.
See `docs/mobile/evidence/20261016/recognition-fix-acceptance.json`.
No test code was added, modified or run; no commit, push or release.
Full final-package matrix, physical microphone/LAN/cloud, full Xcode/iOS,
production signing, armv7 runtime and physical-device acceptance remain open.
The same final APK also passed Tablet recognition/history/search/playback below;
TV file recognition remains blocked by its system DocumentsStub picker.

Previous package `de2450c0` (242 inputs,
81,773,108 bytes). Only `controls.dart`, `shell.dart` and the two ABI AOT libraries
changed from `32ec`: the TV header search field now accepts Up to escape after
keyboard dismissal. Actual DPAD reaches Settings; TV controlled DLNA cast,
Play/Pause, volume 50→60, Back focus and Stop passed. Tablet controlled casting,
volume 50→73, 700/1280dp reflow and real song replacement while casting passed.
See `docs/mobile/evidence/20261016/dlna-devices-acceptance.json`.

The same Tablet batch installed the user-authorized Ceru script and WY list
plugin through the native picker. Both distinct playback scripts actually played;
bidirectional selection, existing-stream continuity, uninstall cancellation,
confirmed removal/recovery and fresh-process persistence passed. Both scripts
are ikun v26 from the same vendor; no independent-vendor claim. See
`docs/mobile/evidence/20261016/playback-pair-acceptance.json`. The original APK,
9 TV/16 Tablet files, full external/shared Downloads/settings/microphone permission
were restored and independently cold-verified without launching the App. Protected
files were checked after reinstalling the original APK and before data restoration;
non-root Release runtime snapshots could not read internal files. All batch
processes/port/forwards stopped. Formatting/analyze/dual-ABI Release/signature/CRC/
16KiB alignment/source map and both installed readbacks passed. No tests or commits.

Previous `32ecd968` has two source changes from `b6ef`: recognition errors have
bounded categories and useful messages. Phone actual generated-file selection/Send distinguishes no match,
silence and unreadable audio; Home clears staging and ends the session. Two
seeded metadata history rows support preview/cancel, explicit clear and empty
history persistence after process restart. They are not live recognition matches.
Eight protected files and queue/current/position stayed unchanged. Original APK,
11 files, full external/shared Downloads/settings and microphone permission were
independently cold-restored without launching the app; emulators/forwards stopped.
Rust/Dart format, cargo check, analyze, dual-ABI Release, signature, CRC, 16KiB
alignment and install readback passed; 242 inputs match the package snapshot.
See `docs/mobile/evidence/20261016/recognition-acceptance.json`. Live successful
matching/history append/playback and microphone capture remain unverified; the
header-only packet capture does not establish transport behavior or a fingerprint
defect. Physical DLNA compatibility,
external services, full Xcode/iOS, signing and physical devices remain open.
Default font only; minor visual differences accepted. No test code was added,
modified or run; no commit, push or release.

The same `32ec` package now has Phone controlled DLNA runtime evidence:
actual guest SSDP discovery/deduplication, local-song rejection without SOAP,
paused and playing handoff, Play/Pause/seek to 178 seconds, remote position
reconciliation and a 30-second Home/background window. Normal Stop succeeds;
controlled SOAP500 Stop displays an unconfirmed warning. Discovery cancellation
joins its worker before releasing the native multicast lock; no-device and fresh
process cleanup also pass. The delivered demo URI independently returns206 to
host curl and decodes three seconds; host Python403 remains an unisolated client
condition. The receiver fixture returns controlled positions and does not render
sound. No physical/cross-device LAN or Tablet/TV DLNA compatibility is claimed.
Original APK/11 files/full external/shared Downloads/settings/microphone permission
were independently cold-restored; emulator/renderer/relay/port/forwards stopped.
All242 inputs/APK remain unchanged; no rebuild or test code. See
`docs/mobile/evidence/20261016/dlna-protocol-acceptance.json`. Build acceptance
remains the recognition package above; remaining platform/external gaps stay open.

The previous package is `b6ef160e`, with one
Dart input change from `21eb`: shared TV single-line inputs reveal their focused
field above the onscreen keyboard. Actual DPAD/onscreen-keyboard S3 merge of two
playlists/three songs, explicit encrypted upload, one retention deletion, header
focus return and restart persistence passed. Independent decrypt agrees; secrets
do not persist. Format/analyze/dual-ABI Release/signature/CRC/16KiB alignment and
install readback passed; only `controls.dart` and two `libapp.so` entries changed.
Original APK/nine files/full external/shared Downloads/settings were independently
no-App cold-restored; both emulators and loopback service/port stopped. See
`docs/mobile/evidence/20261016/tv-s3-acceptance.json`. Nonsecret connection fields
were fixture-prefilled; only secret/password entry was exercised on the TV keyboard.
Default font only; minor visual differences accepted. External cloud authentication,
SigV4/TLS, full paging/plugin restore, iOS/signing and physical devices remain open.

The previous `21ebddcf` package has one
Dart input change from `92b3`: singer album grids stay mounted while detail is
open, and Back restores the original TV card focus and scroll position. Actual
DPAD first/second-row return, immediate reopening, nested singer Back and explicit
nine-song play-all passed. MediaSession advanced from 11702 to 19770 ms and
paused at 20408 ms. Analyze, dual-ABI Release build, signature, CRC, alignment
and install readback passed; only the two `libapp.so` entries differ. Six original
protected files/audio/catalog and full external/shared Downloads stayed unchanged.
Original APK/nine files/settings were independently cold-restored without launching
the app, and both emulator processes stopped. See
`docs/mobile/evidence/20261016/tv-albums-acceptance.json`.

The historical `21eb` package has Tablet S3 UI/client runtime evidence against a
loopback Moto protocol simulator: wrong password and preview cancellation, explicit
merge of two playlists/three songs, independently decrypted actual upload, retention
of two backups with one old deletion and an unchanged unrelated object, duplicate
restore without new rows, and overwrite to one playlist/one song with settings.
Fresh-process data and last-upload timestamp persist; secrets do not. The original
APK/16 files/full external/shared three Downloads/settings were independently
cold-restored without launching the app; emulator/service/port stopped. See
`docs/mobile/evidence/20261016/s3-mobile-acceptance.json`. Product inputs and build
identity are unchanged. Moto does not verify real cloud authentication, SigV4/TLS
compatibility, full paging, partial failures or plugin restore.

Phone default-font S3 acceptance on the same package now covers PUT403 rejection,
an object stored before a PUT503 response, successful upload with List503, failed
old-object DELETE503, and Home before a held successful PUT response. UI distinguishes
unconfirmed/unknown upload from successful upload with skipped, unknown or cancelled
cleanup. Ten actual app requests and independent cloud/decrypt/local timestamp checks
agree; unknown upload is never automatically resent. The original APK/11 files/full
external/shared one Download/settings were independently cold-restored without app
launch; all task processes and port stopped. See
`docs/mobile/evidence/20261016/s3-failures-acceptance.json`. This is controlled
loopback fault evidence, not external cloud authentication or full paging/plugin
restore coverage. TV S3 on `b6ef` is recorded separately above.

Early remote input during welcome accidentally started recognition. The app was
stopped, original files reset, the authorized fixture reapplied and microphone
permission revoked; cold boot confirms permission denied. Prior permission prompt
flags were not backed up, so exact metadata restoration is unverified. This action
is excluded from recognition acceptance.

Historical `92b3` TV search/artist focus and Tablet singer acceptance remain tied
to that package (`tv-artist-*` and `singer-*`). They cover real profile/album loading,
offline album Retry, nested artist navigation, Tablet play-all cancel/confirm,
actual playback and background progression. Home closes Tablet album detail while
reopening its singer session. No full current-package three-device or full paging
matrix is claimed. Default font only; minor visual differences are accepted.

The previous `880be09c` package has a localized Dart fix retaining the catalog
session while the tag editor opens an artwork picker. Format, analyze and both-ABI Release build passed; only the
two Dart AOT archive entries differ from the earlier `06bd` package. Phone
acceptance covers invalid-image recovery, preview/explicit tag and artwork save,
Home invalidation, year/artwork removal while playing and process recovery.
Decoded PCM and the original protected files remain unchanged. Original APK,
11 files, external/shared Downloads and settings were independently cold-restored.
See `docs/mobile/evidence/20261016/local-tags-acceptance.json`. The debug `c9ed`
and isolated build `20dbe` were not rebuilt for this fix; earlier runtime evidence
retains its original package identity. Only default font scale is required and
minor visual differences are accepted.

The previous `880be` package also passed Tablet controlled HTTP Range
acceptance: matching 206 preserves the exact existing prefix, ignored Range/200
restarts from zero, and a mismatched 206 start triggers a fresh no-Range request.
All three tasks finish with no partial residue and decode to the source PCM.
Actual completed-file Open grants its content URI to a native external audio
preview; progress advances, pause/Back releases it and the app queue remains
paused. See `docs/mobile/evidence/20261016/range-download-acceptance.json`.
This uses loopback synthetic audio, not the demo server's HTTP behavior. Original
APK, 16 files, external/shared Downloads and settings were independently restored
and cold-verified without launching the app. No product source changed.

The earlier `06bd` universal package has actual
arm64 TV foreground/background media-key and fresh-process playback evidence,
plus Phone/Tablet local playback, lyrics, background controls and process recovery.
Tablet media keys also switched between two synthetic tracks. These are local
workflow slices. The same universal Release also passed Tablet real Subsonic
search/playback/background, download-choice cancellation, actual transfer progress
without duplicated bytes, partial preservation through process restart, offline
failure and same-task retry to a fully decoded MP3, active cancellation, and
third-process completed/cancelled record recovery. See
`docs/mobile/evidence/20261016/release-online-acceptance.json`. Original APK/data
and system settings were independently cold-boot verified after restoration.
The same Release also passed TV LAN receiver plugin-URL fetch/install, HTTP404
rejection, duplicate single-entry persistence and a visible card after restart;
see `docs/mobile/evidence/20261016/release-lan-url-acceptance.json`. This used a
task loopback script server and guest TCP, not a browser or physical LAN.
Phone/TV real Subsonic search, playback, 30 seconds of Home background advance,
pause/return retention and exact-position playback in a fresh process also
passed on this same package; see
`docs/mobile/evidence/20261016/release-online-devices-acceptance.json`. Audio-cache
lookup was temporarily disabled without deleting original cache files. Together
with Tablet evidence, three-device online slices are covered. Original APKs/data
and settings were independently cold-boot restored without launching the app.
HTTP206/Range reuse and the complete Release business matrix remain unverified.
The same `06bd` Release also passed Phone system-picker rejection of an
oversize file, a batch over 512 MiB, 101 selected files, empty/invalid audio and
a valid/invalid mixed selection. Each failure preserved the original catalog
and audio with no staging residue; a subsequent valid import in the same
process produced an identical, fully decoded MP3. See
`docs/mobile/evidence/20261016/import-bounds-acceptance.json`. Blocked providers,
concurrent cancellation, storage-full and iOS/physical-device cases remain open.
Activity media keys route through the existing MusicService controller before
Flutter, including when a text field has focus. Physical remote/headset input,
multi-track key skipping on TV and input before a media session exists remain unverified.

Split outputs are `app-armeabi-v7a-debug.apk`, `app-armeabi-v7a-release.apk` and
`app-arm64-v8a-release.apk` in `build/app/outputs/flutter-apk`. Release mode still
uses the local debug certificate. The arm64 Release package has a TV-emulator
startup/local playback/background/pause/restart slice; armv7 installation and
playback remain unverified. x86 Android is not supported by the native-assets
hook. Do not change an emulator's ABI to work around this.

Keep `NativeMedia` annotated with `@Keep`: Rust calls its callbacks by JNI name,
which R8 cannot discover. The first Release build removed seven callbacks and
could not play audio; retaining the class fixes that failure without disabling
shrinking for the rest of the app.

Split release version codes are 1001/2001 for version `0.1.0+1`. Directly replacing
Release version 2001 with debug version 1 was rejected even with `adb install -d`.
Acceptance recovery used a same-certificate temporary debug version 2001, then
the original debug version 1, without uninstalling or clearing data; original
files were restored from a fresh backup and independently cold-boot verified.

When changing the FRB API, regenerate from this directory with
`flutter_rust_bridge_codegen generate`, then repeat the affected checks/builds.
For a Dart-only change, do not regenerate the bridge unnecessarily.

## Install and record the exact package

```sh
adb devices
adb -s DEVICE_SERIAL install -r build/app/outputs/flutter-apk/app-debug.apk
adb -s DEVICE_SERIAL shell am start -n com.vant.mio_music/.MainActivity
shasum -a 256 build/app/outputs/flutter-apk/app-debug.apk
adb -s DEVICE_SERIAL shell pm path com.vant.mio_music
```

Run `adb -s DEVICE_SERIAL shell sha256sum APK_PATH` with the installed path
returned by `pm path`; it must match the local APK. Record device/API, package
hash, source fingerprints, logs and screenshots with each acceptance batch.
Use isolated app data and synthetic audio. Emulator/MediaSession results do
not establish audible quality, physical-device behavior or GPU performance.

## iOS build boundary

The current host has Command Line Tools only. Full Xcode, the iOS SDK and a
simulator runtime are required before running the following build commands;
they have not passed on this host:

```sh
source tools/mobile-env.sh
cd apps/mobile
flutter build ios --simulator --debug
flutter build ios --release --no-codesign
```

The deployment target is iOS 15.0. Swift syntax parsing and plist lint do not
replace type checking, device/simulator linking, audio-session verification or
iPhone/iPad workflows. Device signing, IPA export and re-signing require their
own evidence and authorized signing material.

## Distribution status

Version is `0.1.0+1`. This is a development package, not a release candidate.
Android's template release configuration still uses the debug signing key;
do not distribute it as a production-signed release. No production signing
material has been configured or requested. Release signing, iOS packaging and
the remaining functional/visual acceptance
exits must be completed before P7 closes. Building a debug APK does not close
P4/P5/P6/P7 or physical-device P8. Do not commit keys or passwords, or publish
artifacts automatically.

A separate source snapshot now builds the universal optimized Release from fresh
project output directories. Offline locked dependencies, both native ABIs,
package contents and R8 JNI retention passed; see
`docs/mobile/evidence/20261016/clean-release-acceptance.json`. Installed toolchains
and dependency download caches were reused. This closes isolated project-output
rebuilding. Pristine-host and byte reproducibility were not verified and are not
additional acceptance gates for this batch. The rebuilt APK starts
`20dbe40b`; it has not been installed or run. The runtime-accepted `06bd987c` APK
is preserved, and its runtime evidence remains attached to that exact package.

The previous `880be` Release also passed actual Tablet transient audio-focus loss
and automatic resume with another app's native preview, SystemUI notification
play/pause/previous/next, and continued foreground playback after an actual
recent-task dismissal. After 32.58 seconds, notification Pause and reopening
preserved the same Core and paused position. See
`docs/mobile/evidence/20261016/native-focus-acceptance.json`. These emulator
results do not establish real call/headset/Bluetooth, power-policy, long-run,
or physical audio behavior. Original APK/data/system were cold-restored.


The unchanged `de2450` Release also has a real Phone file-recognition attempt:
a 15-second authorized Festival/The Hardkiss WAV was selected through the native
picker and confirmed once in the foreground. The result was no match and no
history entry; successful identification and matched playback were unverified in that batch.
The local queue and paused position were preserved, microphone permission stayed
denied, and the original APK, 11 internal files, complete external/shared data
and system settings were independently cold-restored without starting the app.
Both emulator processes are stopped. See
`docs/mobile/evidence/20261016/recognition-live-acceptance.json`.

Follow-up root cause: `recognition-fix-*` confirms the old AFP host emitted all-zero
fingerprints. Earlier no-match attempts retain their original UI/cleanup attribution;
they do not establish valid-fingerprint catalog or transport coverage. The current
`bddefdf4` Phone success above supersedes that success-path gap.

The unchanged final `bddefdf4` also passed actual Tablet native-file recognition:
three Imagine/John Lennon matches at 30 seconds, matched playback progression,
1280dp wide and 700dp narrow history, real history search with 20 results,
byte-identical history and exact paused-position persistence across a new process,
and history Play from zero through 24.668 seconds. TV DPAD entry, Choose/Close,
reopen and Back restored header focus. Its system resolves file selection to
DocumentsStub and reports no app can handle it; no TV fingerprint was submitted
and TV recognition/result actions are unverified. The TV emulator exposes a
microphone capability, but it was never authorized or captured.

See `docs/mobile/evidence/20261016/recognition-devices-acceptance.json`.
Original Tablet/TV APKs, 16/9 internal files, full external/shared data, settings
and microphone permissions were restored and independently cold-verified without
starting Mio. All emulators/forwards stopped. No product inputs changed; the same
242-input source snapshot and installed APK hashes match. No tests or commits.
