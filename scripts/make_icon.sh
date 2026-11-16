#!/bin/sh
# Renders the 1024 SVG icon master with headless Chrome (it uses SVG filters rsvg gets wrong),
# downscales it into an iconset with sips and packs assets/RuneMD.icns with iconutil.
set -eu
cd "$(dirname "$0")/.."
src="$PWD/design/mockups/2026-11-16-brand-icon/icons/forge-md-outfit-plate-fire-low.svg"
out=assets/RuneMD.icns
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
cat > "$D/icon.html" <<HTML
<!doctype html><html><head><style>html,body{margin:0;background:transparent}img{display:block;width:1024px;height:1024px}</style></head>
<body><img src="file://$src"></body></html>
HTML
"$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --window-size=1024,1024 --default-background-color=00000000 \
  --screenshot="$D/master.png" "file://$D/icon.html" >/dev/null 2>&1
[ "$(sips -g pixelWidth -g hasAlpha "$D/master.png" | awk '/pixelWidth|hasAlpha/{print $2}' | tr '\n' ' ')" = "1024 yes " ] \
  || { echo "render failed: $D/master.png is not a 1024 px PNG with alpha" >&2; exit 1; }
set_=$D/RuneMD.iconset; mkdir "$set_"
for s in 16 32 128 256 512; do
  sips -z $s $s "$D/master.png" --out "$set_/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  if [ $d -eq 1024 ]; then cp "$D/master.png" "$set_/icon_${s}x${s}@2x.png"
  else sips -z $d $d "$D/master.png" --out "$set_/icon_${s}x${s}@2x.png" >/dev/null; fi
done
iconutil -c icns "$set_" -o "$out"
[ -n "${KEEP_ICONSET:-}" ] && cp -R "$set_" "$KEEP_ICONSET"
echo "$out"
