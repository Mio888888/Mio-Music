#!/bin/sh
# Release bundle for macOS: "<product>.app", a .dmg and the updater archive (.app.tar.gz).
#
#   MIO_PRODUCT_NAME   bundle name                 (default: Mio Music)
#   MIO_BUNDLE_ID      CFBundleIdentifier          (default: com.vant.Mio.Music)
#   MIO_PROFILE        cargo profile directory     (default: release; debug for a quick local run)
#   MIO_OUT            where the files are written (default: dist)
#   MIO_CODESIGN_IDENTITY  codesign identity       (default: - , an ad-hoc signature)
#
# Asset names follow tauri-action's: spaces in the product name become dots (GitHub does this
# to uploads), and the updater archive carries no version.
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
    echo 'This bundle requires macOS.' >&2
    exit 1
fi
product=${MIO_PRODUCT_NAME:-Mio Music}
identifier=${MIO_BUNDLE_ID:-com.vant.Mio.Music}
profile=${MIO_PROFILE:-release}
out=${MIO_OUT:-dist}
identity=${MIO_CODESIGN_IDENTITY:--}
version=${MIO_VERSION:-$(python3 -I tools/ci/version.py)}
stem=$(printf '%s' "$product" | tr ' ' '.')
case "$(uname -m)" in
    arm64) arch=aarch64 ;;
    *) arch=x86_64 ;;
esac

if [ "$profile" = release ]; then
    cargo build --release --locked -p mio-app
else
    cargo build --locked -p mio-app
fi
binary="target/$profile/mio-app"

work="$out/macos"
bundle="$work/$product.app"
rm -rf "$work"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$binary" "$bundle/Contents/MacOS/mio-app"
cp packaging/icons/icon.icns "$bundle/Contents/Resources/icon.icns"
# AVIF decoding uses dav1d. Ship the actual linked library, independent of the Homebrew
# installation path, so the app runs on machines without Homebrew.
dav1d_path=$(otool -L "$binary" | awk '/libdav1d.*dylib/ {print $1; exit}')
if [ -n "$dav1d_path" ]; then
    mkdir -p "$bundle/Contents/Frameworks"
    dav1d_name=$(basename "$dav1d_path")
    cp "$dav1d_path" "$bundle/Contents/Frameworks/$dav1d_name"
    chmod u+w "$bundle/Contents/Frameworks/$dav1d_name"
    install_name_tool -id "@executable_path/../Frameworks/$dav1d_name" "$bundle/Contents/Frameworks/$dav1d_name"
    install_name_tool -change "$dav1d_path" "@executable_path/../Frameworks/$dav1d_name" "$bundle/Contents/MacOS/mio-app"
    cp "$(pkg-config --variable=prefix dav1d)/COPYING" "$bundle/Contents/Resources/dav1d-COPYING.txt"
    codesign --force --sign "$identity" "$bundle/Contents/Frameworks/$dav1d_name"
fi
cp crates/mio-app/assets/icons/TDesign-LICENSE.txt "$bundle/Contents/Resources/TDesign-LICENSE.txt"
cp crates/mio-app/src/shell/background-LICENSES.txt "$bundle/Contents/Resources/BackgroundFilterLicenses.txt"
cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>mio-app</string>
<key>CFBundleIdentifier</key><string>$identifier</string>
<key>CFBundleName</key><string>$product</string>
<key>CFBundleDisplayName</key><string>$product</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>LSApplicationCategoryType</key><string>public.app-category.music</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSMicrophoneUsageDescription</key><string>听歌识曲需要录制最多 15 秒的麦克风声音，用于生成音乐指纹。</string>
</dict></plist>
PLIST
codesign --force --sign "$identity" "$bundle"

# Updater archive: a gzip tar whose only top-level entry is the .app folder.
COPYFILE_DISABLE=1 tar -czf "$out/${stem}_${arch}.app.tar.gz" -C "$work" "$product.app"

# Disk image: the app next to an /Applications link.
dmg_root="$work/dmg"
mkdir -p "$dmg_root"
cp -R "$bundle" "$dmg_root/"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -volname "$product" -srcfolder "$dmg_root" -ov -format UDZO "$out/${stem}_${version}_${arch}.dmg" > /dev/null
rm -rf "$dmg_root"

printf '%s\n' "$out/${stem}_${version}_${arch}.dmg" "$out/${stem}_${arch}.app.tar.gz"
