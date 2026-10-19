#!/bin/sh
# Local development bundle, with an ad-hoc signature and no publishing.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
    echo 'This development bundle requires macOS.' >&2
    exit 1
fi
cargo build --locked -p mio-app
bundle='target/debug/Mio Music GPUI.app'
mkdir -p "$bundle/Contents/MacOS"
mkdir -p "$bundle/Contents/Resources"
cp target/debug/mio-app "$bundle/Contents/MacOS/mio-app.new"
mv -f "$bundle/Contents/MacOS/mio-app.new" "$bundle/Contents/MacOS/mio-app"
# AVIF decoding uses dav1d. Copy the actual linked library, independent of the
# Homebrew installation path, before signing both the library and bundle.
dav1d_path=$(otool -L target/debug/mio-app | awk '/libdav1d.*dylib/ {print $1; exit}')
if [ -n "$dav1d_path" ]; then
    mkdir -p "$bundle/Contents/Frameworks"
    dav1d_name=$(basename "$dav1d_path")
    cp "$dav1d_path" "$bundle/Contents/Frameworks/$dav1d_name.new"
    mv -f "$bundle/Contents/Frameworks/$dav1d_name.new" "$bundle/Contents/Frameworks/$dav1d_name"
    chmod u+w "$bundle/Contents/Frameworks/$dav1d_name"
    install_name_tool -id "@executable_path/../Frameworks/$dav1d_name" "$bundle/Contents/Frameworks/$dav1d_name"
    install_name_tool -change "$dav1d_path" "@executable_path/../Frameworks/$dav1d_name" "$bundle/Contents/MacOS/mio-app"
    cp "$(pkg-config --variable=prefix dav1d)/COPYING" "$bundle/Contents/Resources/dav1d-COPYING.txt"
    codesign --force --sign - "$bundle/Contents/Frameworks/$dav1d_name"
fi
cp packaging/icons/icon.icns "$bundle/Contents/Resources/icon.icns"
cp crates/mio-app/assets/icons/TDesign-LICENSE.txt "$bundle/Contents/Resources/TDesign-LICENSE.txt"
cp crates/mio-app/src/shell/background-LICENSES.txt "$bundle/Contents/Resources/BackgroundFilterLicenses.txt"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>mio-app</string>
<key>CFBundleIdentifier</key><string>com.vant.Mio.Music.GPUI</string>
<key>CFBundleName</key><string>Mio Music GPUI</string>
<key>CFBundleDisplayName</key><string>Mio Music GPUI</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSMicrophoneUsageDescription</key><string>听歌识曲需要录制最多 15 秒的麦克风声音，用于生成音乐指纹。</string>
</dict></plist>
PLIST
codesign --force --sign - "$bundle"
printf '%s\n' "$PWD/$bundle"
