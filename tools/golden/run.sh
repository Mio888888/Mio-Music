#!/bin/sh
# One command for one screen: capture the GPUI window, compare it with the reference, and say
# where the whole-screen SSIM is lost.
#
#   tools/golden/run.sh                      # light, 1200x800 (the idle discovery screen)
#   THEME=dark tools/golden/run.sh           # 1200x800 dark
#   WIDTH=768 tools/golden/run.sh            # 768x800 light
#
# Needs Python with Pillow and numpy (PYTHON=… to choose one). Uses a throw-away data folder, so
# no user settings, credentials or playlists are read. Exit 1 is a failed parity gate.
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
PYTHON=${PYTHON:-python3}
THEME=${THEME:-light}
WIDTH=${WIDTH:-1200}
OUT=${OUT:-$ROOT/tools/golden/captures}
NAME=$THEME-${WIDTH}x800
DATA=$(mktemp -d)
trap 'rm -rf "$DATA"' EXIT
mkdir -p "$OUT"
rm -f "$OUT/gpui-$NAME.json" "$OUT/gpui-$NAME.png"
cd "$ROOT"
MIO_GPUI_DATA_DIR="$DATA" MIO_GPUI_CAPTURE_DIR="$OUT" MIO_GPUI_CAPTURE_THEME="$THEME" \
  MIO_GPUI_CAPTURE_WIDTH="$WIDTH" cargo run -q --locked -p mio-app --features parity
status=0
"$PYTHON" tools/golden/compare.py \
  --reference "docs/parity/baselines/reference-$NAME.json" \
  --actual "${OUT#"$ROOT"/}/gpui-$NAME.json" \
  --report "docs/parity/results/$NAME.json" || status=$?
if [ "$WIDTH" = 1200 ]; then
  "$PYTHON" tools/golden/regions.py \
    --reference "docs/parity/baselines/reference-$NAME.png" \
    --actual "${OUT#"$ROOT"/}/gpui-$NAME.png"
fi
exit $status
