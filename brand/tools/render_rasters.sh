#!/usr/bin/env bash
# Render brand/icons/*.png, brand/icons/favicon.ico and brand/og.png from the
# SVGs, with headless Chrome. Run after make_logos.py. Needs: Chrome, python3
# with Pillow (pip install pillow). CHROME may point at another Chrome binary.
set -euo pipefail
brand="$(cd "$(dirname "$0")/.." && pwd)"
chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

shot() { # out width height url
  "$chrome" --headless=new --disable-gpu --allow-file-access-from-files --hide-scrollbars \
    --force-device-scale-factor=1 --default-background-color=00000000 --virtual-time-budget=2000 \
    --window-size="$2,$3" --screenshot="$1" "$4" 2>/dev/null
}
icon() { # svg size out
  printf '<html><body style="margin:0;background:transparent"><img src="%s" width="%s" height="%s" style="display:block"></body></html>' \
    "$brand/$1" "$2" "$2" > "$tmp/i.html"
  shot "$3" "$2" "$2" "file://$tmp/i.html"
}

for s in 16 32 48; do icon logo/mark.svg "$s" "$tmp/m$s.png"; done
icon icons/icon.svg 180 "$brand/icons/apple-touch-icon.png"
icon icons/icon.svg 192 "$brand/icons/icon-192.png"
icon icons/icon.svg 512 "$brand/icons/icon-512.png"
python3 - "$tmp" "$brand/icons/favicon.ico" <<'PY'
import sys
from PIL import Image
d, out = sys.argv[1], sys.argv[2]
ims = [Image.open(f'{d}/m{s}.png').convert('RGBA') for s in (16, 32, 48)]
ims[2].save(out, sizes=[(16, 16), (32, 32), (48, 48)], append_images=ims[:2])
PY
"$chrome" --headless=new --disable-gpu --allow-file-access-from-files --hide-scrollbars \
  --force-device-scale-factor=1 --virtual-time-budget=2000 --window-size=1200,630 \
  --screenshot="$brand/og.png" "file://$brand/tools/og.html" 2>/dev/null
echo "rendered icons and og.png in $brand"
echo "copy changed files into site/public and site/docs/public (site/scripts/check.mjs verifies them)"
