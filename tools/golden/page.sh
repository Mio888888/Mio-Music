#!/bin/sh
# One page other than the idle discovery screen: reference from the original frontend rendered by
# WKWebView (tools/spec-extract/wk_capture.swift, needs the original's dev server on :1420), the
# GPUI capture of the same page, and the comparison.
#
#   tools/golden/page.sh settings            # appearance category of the settings page
#   CATEGORY=storage tools/golden/page.sh settings
#   tools/golden/page.sh downloads
#   tools/golden/page.sh playlists
#
# THEME=dark, WIDTH=768 as for run.sh. REFERENCE=0 reuses the stored reference.
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
PYTHON=${PYTHON:-python3}
PAGE=$1
CATEGORY=${CATEGORY:-}
THEME=${THEME:-light}
WIDTH=${WIDTH:-1200}
OUT=${OUT:-$ROOT/tools/golden/captures}
case "$PAGE" in
  settings) ROUTE="/settings"; [ -n "$CATEGORY" ] && ROUTE="/settings?category=$CATEGORY" ;;
  downloads) ROUTE="/home/download" ;;
  playlists) ROUTE="/home/songlist" ;;
  *) echo "unknown page: $PAGE" >&2; exit 2 ;;
esac
NAME=$PAGE${CATEGORY:+-$CATEGORY}-$THEME-${WIDTH}x800
cd "$ROOT"
if [ "${REFERENCE:-1}" = 1 ]; then
  [ -x /tmp/wk_capture ] || swiftc -O tools/spec-extract/wk_capture.swift -o /tmp/wk_capture
  SELECTORS=tools/spec-extract/selectors-$PAGE.json
  [ -f "$SELECTORS" ] || SELECTORS=tools/spec-extract/selectors-none.json
  /tmp/wk_capture "http://127.0.0.1:1420/#$ROUTE" "$WIDTH" 800 "docs/parity/baselines/reference-$NAME" \
    "$THEME" tools/spec-extract/tauri_mock.js "$SELECTORS" 5
fi
DATA=$(mktemp -d)
trap 'rm -rf "$DATA"' EXIT
mkdir -p "$OUT"
GPUI_NAME=$PAGE${CATEGORY:+-$CATEGORY}-$THEME-${WIDTH}x800
rm -f "$OUT/gpui-$GPUI_NAME.json" "$OUT/gpui-$GPUI_NAME.png"
MIO_GPUI_DATA_DIR="$DATA" MIO_GPUI_CAPTURE_DIR="$OUT" MIO_GPUI_CAPTURE_THEME="$THEME" \
  MIO_GPUI_CAPTURE_WIDTH="$WIDTH" MIO_GPUI_CAPTURE_PAGE="$PAGE" MIO_GPUI_CAPTURE_CATEGORY="$CATEGORY" \
  cargo run -q --locked -p mio-app --features parity
status=0
"$PYTHON" tools/golden/compare.py \
  --reference "docs/parity/baselines/reference-$NAME.json" \
  --actual "${OUT#"$ROOT"/}/gpui-$GPUI_NAME.json" \
  --report "docs/parity/results/$NAME.json" || status=$?
exit $status
