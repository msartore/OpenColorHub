#!/usr/bin/env bash
# Regenerates the Tauri icon set from src-tauri/icon-source.png using ffmpeg.
#
# The artwork is 1-3 px neon line art at 1024 px. Plain downscaling averages
# those lines into grey, so the small sizes get a lanczos scale plus an unsharp
# mask and a brightness/saturation lift, while 128 px and up stay faithful.
set -euo pipefail

cd "$(dirname "$0")/.."

SRC="src-tauri/icon-source.png"
OUT="src-tauri/icons"
TMP="$(pwd)/.icons-tmp"
rm -rf "$TMP"
mkdir -p "$TMP"
# ffmpeg and python are native Windows binaries: they cannot open MSYS paths
# like /e/... or /tmp/..., so hand them the Windows form.
TMPW="$(cygpath -w "$TMP" 2>/dev/null || echo "$TMP")"
trap 'rm -rf "$TMP"' EXIT

command -v ffmpeg >/dev/null || { echo "ffmpeg not found on PATH" >&2; exit 1; }
[ -f "$SRC" ] || { echo "missing $SRC" >&2; exit 1; }

# size:filter
render() {
  local size="$1" filter
  if [ "$size" -le 64 ]; then
    # gamma, not brightness: additive lightening turns the tile's black margins
    # into grey, gamma lifts the neon lines and leaves true black alone.
    filter="scale=${size}:${size}:flags=lanczos+accurate_rnd+full_chroma_int,\
unsharp=5:5:1.2:5:5:0.0,eq=gamma=1.30:contrast=1.12:saturation=1.40"
  else
    filter="scale=${size}:${size}:flags=lanczos+accurate_rnd+full_chroma_int"
  fi
  ffmpeg -nostdin -loglevel error -y -i "$SRC" -vf "$filter" \
    -frames:v 1 -pix_fmt rgba "${TMPW}\\${size}.png"
}

for s in 16 24 32 48 64 128 256; do
  render "$s"
done

cp "$TMP/32.png"   "$OUT/32x32.png"
cp "$TMP/64.png"   "$OUT/64x64.png"
cp "$TMP/128.png"  "$OUT/128x128.png"
cp "$TMP/256.png"  "$OUT/128x128@2x.png"
cp "$TMP/256.png"  "$OUT/icon.png"

# Store logos Windows uses in the Start menu / taskbar tiles.
for s in 30 44 71 89 107 142 150 284 310; do
  render "$s"
  cp "$TMP/$s.png" "$OUT/Square${s}x${s}Logo.png"
done

# .icns and the Android/iOS sets come from the Tauri CLI, which needs its own
# run; it also rewrites the PNGs above, so it goes first in a clean run.
if command -v npx >/dev/null; then
  npx tauri icon "$SRC" >/dev/null 2>&1 || true
  # Re-apply the ffmpeg renders that the CLI just overwrote.
  cp "$TMP/32.png"  "$OUT/32x32.png"
  cp "$TMP/64.png"  "$OUT/64x64.png"
  cp "$TMP/128.png" "$OUT/128x128.png"
  cp "$TMP/256.png" "$OUT/128x128@2x.png"
  cp "$TMP/256.png" "$OUT/icon.png"
fi

python3 scripts/pack-ico.py "$OUT/icon.ico" \
  "${TMPW}\\16.png" "${TMPW}\\24.png" "${TMPW}\\32.png" "${TMPW}\\48.png" \
  "${TMPW}\\64.png" "${TMPW}\\128.png" "${TMPW}\\256.png" 2>/dev/null \
  || python scripts/pack-ico.py "$OUT/icon.ico" \
     "${TMPW}\\16.png" "${TMPW}\\24.png" "${TMPW}\\32.png" "${TMPW}\\48.png" \
     "${TMPW}\\64.png" "${TMPW}\\128.png" "${TMPW}\\256.png"

echo "icons written to $OUT"
ls -l "$OUT"/32x32.png "$OUT"/icon.ico
