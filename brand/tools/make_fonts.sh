#!/usr/bin/env bash
# Rebuild brand/fonts/*.woff2 from the variable TTFs in google/fonts (OFL).
# Each output is a variable font subset to Latin plus the few symbols the
# brand uses (· → ✓ ● ▪ …). Needs: pip install fonttools brotli
set -euo pipefail
dir=$(mktemp -d)
out="$(cd "$(dirname "$0")/.." && pwd)/fonts"
base=https://github.com/google/fonts/raw/main/ofl
curl -fsSL -o "$dir/sg.ttf" "$base/schibstedgrotesk/SchibstedGrotesk%5Bwght%5D.ttf"
curl -fsSL -o "$dir/mm.ttf" "$base/martianmono/MartianMono%5Bwdth,wght%5D.ttf"
curl -fsSL -o "$dir/ge.ttf" "$base/geist/Geist%5Bwght%5D.ttf"
fonttools varLib.instancer -q "$dir/sg.ttf" wght=400:900 -o "$dir/sg-i.ttf"
fonttools varLib.instancer -q "$dir/mm.ttf" wdth=100 wght=400:700 -o "$dir/mm-i.ttf"
fonttools varLib.instancer -q "$dir/ge.ttf" wght=400:700 -o "$dir/ge-i.ttf"
unicodes="U+0020-007E,U+00A0-00FF,U+2010-2027,U+2030-203A,U+20AC,U+2122,U+2190-2193,U+2212,U+2713,U+25AA,U+25CF,U+2026"
for pair in sg:schibsted-grotesk mm:martian-mono ge:geist; do
  pyftsubset "$dir/${pair%%:*}-i.ttf" --unicodes="$unicodes" --layout-features='*' \
    --flavor=woff2 --output-file="$out/${pair##*:}.woff2"
done
rm -rf "$dir"
echo "fonts written to $out"
