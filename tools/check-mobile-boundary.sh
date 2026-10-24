#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# Check the shipped bridge closure; the dependency probe is only for diagnosis.
manifest="${MIO_MOBILE_MANIFEST:-$root/crates/mio-mobile-bridge/Cargo.toml}"
targets=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android aarch64-apple-ios aarch64-apple-ios-sim)
if (($#)); then targets=("$@"); fi
for target in "${targets[@]}"; do
  tree="$(cargo tree --manifest-path "$manifest" --target "$target" --edges normal --prefix none)"
  if printf '%s\n' "$tree" | grep -E '^(gpui[^ ]*|tray-icon|global-hotkey|souvlaki|ksni|x11rb|cocoa|dav1d[^ ]*) v'; then
    printf 'Desktop dependency leaked into %s\n' "$target" >&2
    exit 1
  fi
  cargo check --manifest-path "$manifest" --locked --lib --target "$target"
done
