#!/bin/bash
# Release packages for Linux x86_64: an AppImage (the updater's target) and a .deb.
#
#   MIO_PRODUCT_NAME   product name       (default: Mio Music)
#   MIO_OUT            output directory   (default: dist)
#   MIO_APPIMAGETOOL   path of an already installed appimagetool (otherwise a pinned build is fetched)
#
# Asset names follow tauri-action's: dots instead of spaces, `_<version>_amd64` suffixes.
# The AppImage carries every shared library the binary links against (xkbcommon, xcb, dav1d, ...)
# except glibc, the C++ runtime and the GPU stack, which must come from the host: a minimal
# system must be able to start it.
set -euo pipefail
cd "$(dirname "$0")/.."
product=${MIO_PRODUCT_NAME:-Mio Music}
out=$(realpath -m "${MIO_OUT:-dist}")
version=${MIO_VERSION:-$(python3 -I tools/ci/version.py)}
stem=${product// /.}
binary=target/release/mio-app
appimagetool_version=1.9.0

cargo build --release --locked -p mio-app
work="$out/linux"
rm -rf "$work"
mkdir -p "$out" "$work"

# Libraries that have to match the host's kernel, drivers or C library are never bundled.
system_libs='^(ld-linux.*|linux-vdso|libc|libm|libdl|libpthread|librt|libutil|libresolv|libnsl|libcrypt|libanl|libBrokenLocale|libthread_db|libstdc\+\+|libgcc_s|libGL|libGLX|libGLdispatch|libEGL|libGLESv2|libOpenGL|libvulkan|libdrm|libgbm)\.so'

# --- AppImage ---------------------------------------------------------------------------------
appdir="$work/AppDir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/lib" "$appdir/usr/share/applications"
install -m 755 "$binary" "$appdir/usr/bin/mio-app"
ldd "$binary" | awk '/=> \// {print $3}' | while read -r lib; do
    [[ $(basename "$lib") =~ $system_libs ]] || cp -L "$lib" "$appdir/usr/lib/"
done
# The desktop file is named after the windows' app_id, which Wayland compositors use to find the
# window's icon.
desktop_id=com.vant.Mio.Music.GPUI.desktop
install -m 644 packaging/linux/mio-music.desktop "$appdir/$desktop_id"
install -m 644 packaging/linux/mio-music.desktop "$appdir/usr/share/applications/$desktop_id"
install -m 644 packaging/icons/icon.png "$appdir/mio-music.png"
install -m 644 packaging/icons/icon.png "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
export LD_LIBRARY_PATH="$here/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$here/usr/bin/mio-app" "$@"
RUN
chmod 755 "$appdir/AppRun"
# AppImage's Exec line must name something on PATH inside the image.
ln -s mio-app "$appdir/usr/bin/mio-music"

tool=${MIO_APPIMAGETOOL:-}
if [ -z "$tool" ]; then
    tool="$work/appimagetool"
    curl -fsSL --retry 3 -o "$tool" \
        "https://github.com/AppImage/appimagetool/releases/download/$appimagetool_version/appimagetool-x86_64.AppImage"
    chmod +x "$tool"
fi
appimage="$out/${stem}_${version}_amd64.AppImage"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$tool" "$appdir" "$appimage"

# --- deb --------------------------------------------------------------------------------------
pkg="$work/deb"
mkdir -p "$pkg/DEBIAN" "$pkg/usr/lib/mio-music" "$pkg/usr/bin" "$pkg/usr/share/applications" \
    "$pkg/usr/share/icons/hicolor/512x512/apps"
install -m 755 "$binary" "$pkg/usr/lib/mio-music/mio-app"
ln -s /usr/lib/mio-music/mio-app "$pkg/usr/bin/mio-music"
install -m 644 packaging/linux/mio-music.desktop "$pkg/usr/share/applications/$desktop_id"
install -m 644 packaging/icons/icon.png "$pkg/usr/share/icons/hicolor/512x512/apps/mio-music.png"
# Let dpkg work out the shared-library dependencies of the binary.
shlibs="$work/shlibs"
mkdir -p "$shlibs/debian"
printf 'Source: mio-music\n\nPackage: mio-music\nArchitecture: amd64\n' > "$shlibs/debian/control"
depends=$(cd "$shlibs" && dpkg-shlibdeps -O --ignore-missing-info -e"$OLDPWD/$binary" 2>/dev/null | sed -n 's/^shlibs:Depends=//p' || true)
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: mio-music
Version: $version
Architecture: amd64
Maintainer: Mio <noreply@users.noreply.github.com>
Section: sound
Priority: optional
Depends: ${depends:-libc6}
Description: $product
 Desktop music player with plugin-based music sources.
CONTROL
dpkg-deb --root-owner-group --build "$pkg" "$out/${stem}_${version}_amd64.deb" > /dev/null

rm -rf "$work"
printf '%s\n' "$appimage" "$out/${stem}_${version}_amd64.deb"
