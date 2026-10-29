#!/bin/bash
# Builds and installs dav1d from source on Ubuntu runners. `dav1d-sys` (AVIF/MP4 decoding in gpui)
# needs dav1d 1.3 or newer, and the oldest LTS we build on ships 0.9.
set -euo pipefail
version=1.4.3

sudo apt-get install -y meson ninja-build nasm
src=$(mktemp -d)
git clone --quiet --depth 1 --branch "$version" https://code.videolan.org/videolan/dav1d.git "$src/dav1d"
meson setup "$src/build" "$src/dav1d" --prefix=/usr/local --libdir=lib --buildtype=release \
    -Denable_tools=false -Denable_tests=false
ninja -C "$src/build"
sudo ninja -C "$src/build" install
sudo ldconfig
rm -rf "$src"

# `/usr/local/lib/pkgconfig` is not on pkg-config's default path with `--libdir=lib`.
echo "PKG_CONFIG_PATH=/usr/local/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}" >> "$GITHUB_ENV"
